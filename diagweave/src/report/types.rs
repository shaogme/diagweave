#[path = "types/attachment.rs"]
pub mod attachment;
#[path = "types/error.rs"]
pub mod error;

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use core::any;
use core::error::Error;
use core::fmt::{self, Display, Formatter};

#[cfg(feature = "trace")]
use super::trace::{ParentSpanId, ReportTrace, SpanId, TraceId};

pub use attachment::*;
pub use error::*;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct ReportMetadata {
    pub error_code: Option<ErrorCode>,
    pub severity: Option<Severity>,
    pub category: Option<Cow<'static, str>>,
    pub retryable: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(rename_all = "snake_case"))]
pub enum StackTraceFormat {
    Native,
    Raw,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct StackFrame {
    pub symbol: Option<String>,
    pub module_path: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct StackTrace {
    pub format: StackTraceFormat,
    pub frames: Vec<StackFrame>,
    pub raw: Option<String>,
}

impl Default for StackTrace {
    fn default() -> Self {
        Self {
            format: StackTraceFormat::Native,
            frames: Vec::new(),
            raw: None,
        }
    }
}

impl StackTrace {
    /// Creates a new [`StackTrace`] with the specified format.
    pub fn new(format: StackTraceFormat) -> Self {
        Self {
            format,
            ..Self::default()
        }
    }

    /// Appends frames to the stack trace.
    pub fn with_frames(mut self, frames: Vec<StackFrame>) -> Self {
        self.frames = frames;
        self
    }

    /// Sets the raw stack trace string.
    pub fn with_raw(mut self, raw: impl Into<String>) -> Self {
        self.raw = Some(raw.into());
        self
    }

    /// Captures the current stack trace as a raw string (requires `std` feature).
    #[cfg(feature = "std")]
    pub fn capture_raw() -> Self {
        let backtrace = std::backtrace::Backtrace::force_capture();
        Self {
            format: StackTraceFormat::Raw,
            frames: Vec::new(),
            raw: Some(backtrace.to_string()),
        }
    }
}

/// Traversal state observed during cause collection.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CauseTraversalState {
    /// Whether the traversal was truncated due to depth limit.
    pub truncated: bool,
    /// Whether a circular reference cycle was detected.
    pub cycle_detected: bool,
}

/// A streamed attachment item for visitor-based traversal.
pub enum AttachmentVisit<'a> {
    Context {
        key: &'a Cow<'static, str>,
        value: &'a AttachmentValue,
    },
    Note {
        message: &'a (dyn Display + 'static),
    },
    Payload {
        name: &'a Cow<'static, str>,
        value: &'a AttachmentValue,
        media_type: Option<&'a Cow<'static, str>>,
    },
}

/// Iterator over source errors with depth/cycle control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceErrorEntry {
    pub message: String,
    pub type_name: Option<String>,
    pub depth: usize,
}

pub struct ReportSourceErrorIter<'a> {
    walk: ReportSourceErrorTraversal<'a>,
}

pub(crate) enum SourceErrorNode<'a> {
    Chain {
        items: core::slice::Iter<'a, SourceErrorItem>,
        depth: usize,
    },
    Error {
        current: Option<&'a (dyn Error + 'static)>,
        depth: usize,
    },
}

impl<'a> SourceErrorNode<'a> {
    pub(crate) fn chain(items: core::slice::Iter<'a, SourceErrorItem>, depth: usize) -> Self {
        Self::Chain { items, depth }
    }

    pub(crate) fn error(error: &'a (dyn Error + 'static), depth: usize) -> Self {
        Self::Error {
            current: Some(error),
            depth,
        }
    }
}

impl<'a> ReportSourceErrorIter<'a> {
    pub(crate) fn new(
        report: &'a crate::report::Report<impl Error + 'static>,
        options: CauseCollectOptions,
    ) -> Self {
        Self {
            walk: ReportSourceErrorTraversal::from_report(report, options),
        }
    }

    /// Returns traversal state observed so far.
    pub fn state(&self) -> CauseTraversalState {
        self.walk.state()
    }
}

impl<'a> Iterator for ReportSourceErrorIter<'a> {
    type Item = SourceErrorEntry;

    fn next(&mut self) -> Option<Self::Item> {
        self.walk.next_entry()
    }
}

enum SourceErrorVisit<'a> {
    Error {
        error: &'a (dyn Error + 'static),
        depth: usize,
    },
    Item {
        item: &'a SourceErrorItem,
        depth: usize,
    },
}

pub(crate) struct ReportSourceErrorTraversal<'a> {
    stack: Vec<SourceErrorNode<'a>>,
    options: CauseCollectOptions,
    seen: SeenErrorAddrs,
    state: CauseTraversalState,
}

impl<'a> ReportSourceErrorTraversal<'a> {
    pub(crate) fn from_report(
        report: &'a crate::report::Report<impl Error + 'static>,
        options: CauseCollectOptions,
    ) -> Self {
        let mut stack = Vec::new();
        if let Some(inner_source) = report.inner().source() {
            stack.push(SourceErrorNode::error(inner_source, 0));
        }
        if let Some(source_errors) = report
            .diagnostics()
            .and_then(|diag| diag.source_errors.as_ref())
        {
            stack.push(SourceErrorNode::chain(source_errors.items.iter(), 0));
        }
        Self {
            stack,
            options,
            seen: SeenErrorAddrs::new(),
            state: CauseTraversalState::default(),
        }
    }

    fn state(&self) -> CauseTraversalState {
        self.state
    }

    fn next_entry(&mut self) -> Option<SourceErrorEntry> {
        self.next_visit().map(SourceErrorEntry::from_visit)
    }

    fn next_visit(&mut self) -> Option<SourceErrorVisit<'a>> {
        enum NextAction<'a> {
            Return(SourceErrorVisit<'a>, Option<SourceErrorNode<'a>>),
            PopContinue,
            StopCycle,
        }

        loop {
            let action = {
                let Some(node) = self.stack.last_mut() else {
                    return None;
                };
                match node {
                    SourceErrorNode::Chain { items, depth } => {
                        if *depth >= self.options.max_depth {
                            self.state.truncated = true;
                            NextAction::PopContinue
                        } else {
                            match items.next() {
                                Some(item) => {
                                    if self.options.detect_cycle
                                        && !self.seen.insert(error_addr(item.error.as_ref()))
                                    {
                                        NextAction::StopCycle
                                    } else {
                                        let push = item.source.as_ref().and_then(|source| {
                                            if *depth + 1 < self.options.max_depth {
                                                Some(SourceErrorNode::chain(
                                                    source.items.iter(),
                                                    *depth + 1,
                                                ))
                                            } else {
                                                self.state.truncated = true;
                                                None
                                            }
                                        });
                                        NextAction::Return(
                                            SourceErrorVisit::Item {
                                                item,
                                                depth: *depth,
                                            },
                                            push,
                                        )
                                    }
                                }
                                None => NextAction::PopContinue,
                            }
                        }
                    }
                    SourceErrorNode::Error { current, depth } => {
                        if *depth >= self.options.max_depth {
                            self.state.truncated = true;
                            NextAction::PopContinue
                        } else {
                            match current.take() {
                                Some(error) => {
                                    if self.options.detect_cycle
                                        && !self.seen.insert(error_addr(error))
                                    {
                                        NextAction::StopCycle
                                    } else {
                                        let next = error.source();
                                        *current = next;
                                        let entry_depth = *depth;
                                        *depth += 1;
                                        NextAction::Return(
                                            SourceErrorVisit::Error {
                                                error,
                                                depth: entry_depth,
                                            },
                                            None,
                                        )
                                    }
                                }
                                None => NextAction::PopContinue,
                            }
                        }
                    }
                }
            };

            match action {
                NextAction::Return(visit, push) => {
                    if let Some(push) = push {
                        self.stack.push(push);
                    }
                    return Some(visit);
                }
                NextAction::PopContinue => {
                    self.stack.pop();
                }
                NextAction::StopCycle => {
                    self.state.cycle_detected = true;
                    self.stack.clear();
                    return None;
                }
            }
        }
    }
}

pub(crate) struct SourceErrorChainTraversal<'a> {
    stack: Vec<SourceErrorNode<'a>>,
    seen: SeenErrorAddrs,
    state: CauseTraversalState,
}

impl<'a> SourceErrorChainTraversal<'a> {
    pub(crate) fn from_chain(chain: &'a SourceErrorChain) -> Self {
        Self {
            stack: vec![SourceErrorNode::chain(chain.items.iter(), 0)],
            seen: SeenErrorAddrs::new(),
            state: CauseTraversalState::default(),
        }
    }

    fn next_entry(&mut self) -> Option<SourceErrorEntry> {
        self.next_visit().map(SourceErrorEntry::from_visit)
    }

    fn next_visit(&mut self) -> Option<SourceErrorVisit<'a>> {
        enum NextAction<'a> {
            Return(SourceErrorVisit<'a>, Option<SourceErrorNode<'a>>),
            PopContinue,
            StopCycle,
        }

        loop {
            let action = {
                let Some(node) = self.stack.last_mut() else {
                    return None;
                };
                match node {
                    SourceErrorNode::Chain { items, depth } => match items.next() {
                        Some(item) => {
                            if !self.seen.insert(error_addr(item.error.as_ref())) {
                                NextAction::StopCycle
                            } else {
                                let push = item.source.as_ref().map(|source| {
                                    SourceErrorNode::chain(source.items.iter(), *depth + 1)
                                });
                                NextAction::Return(
                                    SourceErrorVisit::Item {
                                        item,
                                        depth: *depth,
                                    },
                                    push,
                                )
                            }
                        }
                        None => NextAction::PopContinue,
                    },
                    SourceErrorNode::Error { .. } => NextAction::PopContinue,
                }
            };

            match action {
                NextAction::Return(visit, push) => {
                    if let Some(push) = push {
                        self.stack.push(push);
                    }
                    return Some(visit);
                }
                NextAction::PopContinue => {
                    self.stack.pop();
                }
                NextAction::StopCycle => {
                    self.state.cycle_detected = true;
                    self.stack.clear();
                    return None;
                }
            }
        }
    }
}

impl SourceErrorEntry {
    fn from_visit(visit: SourceErrorVisit<'_>) -> Self {
        match visit {
            SourceErrorVisit::Error { error, depth } => Self {
                message: error.to_string(),
                type_name: None,
                depth,
            },
            SourceErrorVisit::Item { item, depth } => Self {
                message: item.error.to_string(),
                type_name: item
                    .type_name
                    .as_ref()
                    .map(|type_name| type_name.to_string()),
                depth,
            },
        }
    }
}

#[derive(Default)]
pub(crate) struct ColdData {
    pub(crate) metadata: ReportMetadata,
    pub(crate) diagnostics: DiagnosticBag,
}

#[derive(Default)]
pub(crate) struct DiagnosticBag {
    #[cfg(feature = "trace")]
    pub(crate) trace: Option<ReportTrace>,
    pub(crate) stack_trace: Option<StackTrace>,
    pub(crate) attachments: Vec<Attachment>,
    pub(crate) display_causes: Option<DisplayCauseChain>,
    pub(crate) source_errors: Option<SourceErrorChain>,
}

pub(crate) const EMPTY_REPORT_METADATA: ReportMetadata = ReportMetadata {
    error_code: None,
    severity: None,
    category: None,
    retryable: None,
};

/// Global context information that can be injected into reports.
#[derive(Debug, Clone, Default)]
pub struct GlobalContext {
    /// Context key-value pairs.
    pub context: Vec<(Cow<'static, str>, AttachmentValue)>,
    /// Global trace ID if available.
    #[cfg(feature = "trace")]
    pub trace_id: Option<TraceId>,
    /// Global span ID if available.
    #[cfg(feature = "trace")]
    pub span_id: Option<SpanId>,
    /// Global parent span ID if available.
    #[cfg(feature = "trace")]
    pub parent_span_id: Option<ParentSpanId>,
}

pub(crate) struct SeenErrorAddrs {
    inline: [usize; 8],
    len: usize,
    spill: Vec<usize>,
}

impl SeenErrorAddrs {
    pub(crate) fn new() -> Self {
        Self {
            inline: [0usize; 8],
            len: 0,
            spill: Vec::new(),
        }
    }

    pub(crate) fn insert(&mut self, addr: usize) -> bool {
        if self.contains(addr) {
            return false;
        }
        if self.len < self.inline.len() {
            self.inline[self.len] = addr;
            self.len += 1;
            return true;
        }
        self.spill.push(addr);
        true
    }

    pub(crate) fn contains(&self, addr: usize) -> bool {
        if self.inline[..self.len].contains(&addr) {
            return true;
        }
        self.spill.contains(&addr)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(rename_all = "snake_case"))]
pub enum CauseKind {
    Error,
    Event,
}

impl Display for CauseKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Error => "error",
            Self::Event => "event",
        };
        write!(f, "{label}")
    }
}

/// Runtime display-cause chain captured in diagnostic bag.
#[derive(Default)]
pub struct DisplayCauseChain {
    pub items: Vec<Box<dyn Display + 'static>>,
    pub truncated: bool,
    pub cycle_detected: bool,
}

fn display_cause_strings(items: &[Box<dyn Display + 'static>]) -> Vec<String> {
    items.iter().map(ToString::to_string).collect()
}

#[cfg(feature = "json")]
#[derive(serde::Serialize)]
struct DisplayCauseChainSerdeHelper {
    items: Vec<String>,
    truncated: bool,
    cycle_detected: bool,
}

impl Clone for DisplayCauseChain {
    fn clone(&self) -> Self {
        Self {
            items: display_cause_strings(&self.items)
                .into_iter()
                .map(|item| Box::new(item) as Box<dyn Display + 'static>)
                .collect(),
            truncated: self.truncated,
            cycle_detected: self.cycle_detected,
        }
    }
}

impl core::fmt::Debug for DisplayCauseChain {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let items = display_cause_strings(&self.items);
        f.debug_struct("DisplayCauseChain")
            .field("items", &items)
            .field("truncated", &self.truncated)
            .field("cycle_detected", &self.cycle_detected)
            .finish()
    }
}

impl PartialEq for DisplayCauseChain {
    fn eq(&self, other: &Self) -> bool {
        display_cause_strings(&self.items) == display_cause_strings(&other.items)
            && self.truncated == other.truncated
            && self.cycle_detected == other.cycle_detected
    }
}

impl Eq for DisplayCauseChain {}

#[cfg(feature = "json")]
impl serde::Serialize for DisplayCauseChain {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        DisplayCauseChainSerdeHelper {
            items: display_cause_strings(&self.items),
            truncated: self.truncated,
            cycle_detected: self.cycle_detected,
        }
        .serialize(serializer)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StringError(String);

impl Display for StringError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for StringError {}

/// Runtime source-error chain captured in diagnostic bag.
#[derive(Debug)]
pub struct SourceErrorItem {
    pub error: Box<dyn Error + 'static>,
    pub type_name: Option<Cow<'static, str>>,
    pub source: Option<Box<SourceErrorChain>>,
}

impl SourceErrorItem {
    pub fn new<T>(error: T) -> Self
    where
        T: Error + 'static,
    {
        Self {
            error: Box::new(error),
            type_name: Some(Cow::Borrowed(any::type_name::<T>())),
            source: None,
        }
    }

    pub(crate) fn with_source(mut self, source: Option<Box<SourceErrorChain>>) -> Self {
        self.source = source;
        self
    }

    pub(crate) fn display_type_name(&self) -> Option<&str> {
        let type_name = self.type_name.as_deref()?;
        if is_report_wrapper_type_name(type_name) {
            None
        } else {
            Some(type_name)
        }
    }

    fn cloned(&self) -> Self {
        Self {
            error: Box::new(StringError(self.error.to_string())),
            type_name: self.type_name.clone(),
            source: self
                .source
                .as_ref()
                .map(|chain| Box::new((**chain).clone())),
        }
    }

    fn from_error<T>(error: T, options: CauseCollectOptions) -> (Self, bool)
    where
        T: Error + 'static,
    {
        let (source, state) = SourceErrorChain::from_borrowed_sources(error.source(), options);
        let item = Self::new(error).with_source(source);
        (item, state.cycle_detected)
    }
}

#[derive(Default)]
pub struct SourceErrorChain {
    pub items: Vec<SourceErrorItem>,
    pub truncated: bool,
    pub cycle_detected: bool,
}

fn source_error_debug_items(items: &[SourceErrorItem]) -> Vec<(String, Option<String>)> {
    items
        .iter()
        .map(|item| {
            (
                item.error.to_string(),
                item.type_name
                    .as_ref()
                    .map(|type_name| type_name.to_string()),
            )
        })
        .collect()
}

fn source_error_eq_items<'a>(
    items: &'a [SourceErrorItem],
) -> Vec<(String, Option<String>, Option<&'a SourceErrorChain>)> {
    items
        .iter()
        .map(|item| {
            (
                item.error.to_string(),
                item.type_name
                    .as_ref()
                    .map(|type_name| type_name.to_string()),
                item.source.as_deref(),
            )
        })
        .collect()
}

#[cfg(feature = "json")]
#[derive(serde::Serialize)]
struct SourceErrorChainSerdeItem {
    message: String,
    #[serde(rename = "type")]
    type_name: Option<String>,
    source: Option<Box<SourceErrorChainSerdeHelper>>,
}

#[cfg(feature = "json")]
#[derive(serde::Serialize)]
struct SourceErrorChainSerdeHelper {
    items: Vec<SourceErrorChainSerdeItem>,
    truncated: bool,
    cycle_detected: bool,
}

impl Clone for SourceErrorChain {
    fn clone(&self) -> Self {
        Self {
            items: self.items.iter().map(SourceErrorItem::cloned).collect(),
            truncated: self.truncated,
            cycle_detected: self.cycle_detected,
        }
    }
}

impl core::fmt::Debug for SourceErrorChain {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let items = source_error_debug_items(&self.items);
        f.debug_struct("SourceErrorChain")
            .field("items", &items)
            .field("truncated", &self.truncated)
            .field("cycle_detected", &self.cycle_detected)
            .finish()
    }
}

impl PartialEq for SourceErrorChain {
    fn eq(&self, other: &Self) -> bool {
        source_error_eq_items(&self.items) == source_error_eq_items(&other.items)
            && self.truncated == other.truncated
            && self.cycle_detected == other.cycle_detected
    }
}

impl Eq for SourceErrorChain {}

impl SourceErrorChain {
    pub(crate) fn from_error<T>(error: T) -> Self
    where
        T: Error + 'static,
    {
        let (item, cycle_detected) = SourceErrorItem::from_error(
            error,
            CauseCollectOptions {
                max_depth: usize::MAX,
                detect_cycle: true,
            },
        );
        Self {
            items: vec![item],
            truncated: false,
            cycle_detected,
        }
    }

    pub(crate) fn from_source(error: &dyn Error, options: CauseCollectOptions) -> Self {
        Self::from_borrowed_error(error, options)
    }

    pub(crate) fn append(&mut self, mut other: SourceErrorChain) {
        let state = other.state();
        self.truncated |= state.truncated;
        self.cycle_detected |= state.cycle_detected;
        self.items.append(&mut other.items);
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn iter_entries(&self) -> SourceErrorChainEntries<'_> {
        SourceErrorChainEntries::new(self)
    }

    pub fn iter(&self) -> core::slice::Iter<'_, SourceErrorItem> {
        self.items.iter()
    }

    pub(crate) fn state(&self) -> CauseTraversalState {
        let mut state = CauseTraversalState {
            truncated: self.truncated,
            cycle_detected: self.cycle_detected,
        };
        walk_source_chains(self, 0, &mut |chain, _depth| {
            state.truncated |= chain.truncated;
            state.cycle_detected |= chain.cycle_detected;
        });
        state
    }

    pub(crate) fn clear_cycle_flags(&mut self) {
        self.cycle_detected = false;
        for item in &mut self.items {
            if let Some(source) = item.source.as_mut() {
                source.clear_cycle_flags();
            }
        }
    }

    pub(crate) fn limit_depth(&mut self, options: CauseCollectOptions, depth: usize) -> bool {
        let mut truncated = self.truncated;
        if depth >= options.max_depth {
            self.items.clear();
            self.truncated = true;
            return true;
        }

        for item in &mut self.items {
            if let Some(source) = item.source.as_mut() {
                truncated |= source.limit_depth(options, depth + 1);
            }
        }
        self.truncated = truncated;
        truncated
    }

    fn from_borrowed_error(error: &dyn Error, options: CauseCollectOptions) -> Self {
        let (source, state) = Self::from_borrowed_sources(error.source(), options);
        Self {
            items: vec![SourceErrorItem {
                error: Box::new(StringError(error.to_string())),
                type_name: None,
                source,
            }],
            truncated: state.truncated,
            cycle_detected: state.cycle_detected,
        }
    }

    fn from_borrowed_sources(
        next: Option<&dyn Error>,
        options: CauseCollectOptions,
    ) -> (Option<Box<SourceErrorChain>>, CauseTraversalState) {
        let Some(mut current) = next else {
            return (None, CauseTraversalState::default());
        };

        let mut seen = SeenErrorAddrs::new();
        let mut state = CauseTraversalState::default();
        let mut items = Vec::new();
        let mut depth = 0usize;

        loop {
            if depth >= options.max_depth {
                state.truncated = true;
                break;
            }

            let addr = error_addr(current);
            if options.detect_cycle && seen.contains(addr) {
                state.cycle_detected = true;
                break;
            }
            if options.detect_cycle {
                let _ = seen.insert(addr);
            }

            items.push(SourceErrorItem {
                error: Box::new(StringError(current.to_string())),
                type_name: None,
                source: None,
            });

            let Some(next) = current.source() else {
                break;
            };
            current = next;
            depth += 1;
        }

        if items.is_empty() {
            return (
                Some(Box::new(SourceErrorChain {
                    items: Vec::new(),
                    truncated: state.truncated,
                    cycle_detected: state.cycle_detected,
                })),
                state,
            );
        }

        let mut chain: Option<Box<SourceErrorChain>> = None;
        for mut item in items.into_iter().rev() {
            item.source = chain;
            chain = Some(Box::new(SourceErrorChain {
                items: vec![item],
                truncated: false,
                cycle_detected: false,
            }));
        }
        if let Some(chain) = chain.as_mut() {
            chain.truncated = state.truncated;
            chain.cycle_detected = state.cycle_detected;
        }
        (chain, state)
    }
}

pub struct SourceErrorChainEntries<'a> {
    walk: SourceErrorChainTraversal<'a>,
}

impl<'a> SourceErrorChainEntries<'a> {
    fn new(chain: &'a SourceErrorChain) -> Self {
        Self {
            walk: SourceErrorChainTraversal::from_chain(chain),
        }
    }
}

impl<'a> Iterator for SourceErrorChainEntries<'a> {
    type Item = SourceErrorEntry;

    fn next(&mut self) -> Option<Self::Item> {
        self.walk.next_entry()
    }
}

fn error_addr(error: &dyn Error) -> usize {
    let ptr = (error as *const dyn Error) as *const ();
    ptr as usize
}

fn walk_source_chains<F>(chain: &SourceErrorChain, depth: usize, visit: &mut F)
where
    F: FnMut(&SourceErrorChain, usize),
{
    let mut stack = vec![(chain, depth)];
    while let Some((current, current_depth)) = stack.pop() {
        visit(current, current_depth);
        for item in current.items.iter().rev() {
            if let Some(source) = item.source.as_ref() {
                stack.push((source, current_depth + 1));
            }
        }
    }
}

fn is_report_wrapper_type_name(type_name: &str) -> bool {
    let report_prefix = core::any::type_name::<crate::report::Report<()>>();
    let report_prefix = report_prefix
        .split_once('<')
        .map(|(prefix, _)| prefix)
        .unwrap_or(report_prefix);
    type_name.starts_with(report_prefix) && type_name[report_prefix.len()..].starts_with('<')
}

#[cfg(feature = "json")]
impl serde::Serialize for SourceErrorChain {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        fn serialize_chain(chain: &SourceErrorChain) -> SourceErrorChainSerdeHelper {
            SourceErrorChainSerdeHelper {
                items: chain
                    .items
                    .iter()
                    .map(|v| SourceErrorChainSerdeItem {
                        message: v.error.to_string(),
                        type_name: v.type_name.as_ref().map(|type_name| type_name.to_string()),
                        source: v
                            .source
                            .as_ref()
                            .map(|source| Box::new(serialize_chain(source))),
                    })
                    .collect(),
                truncated: chain.truncated,
                cycle_detected: chain.cycle_detected,
            }
        }
        serialize_chain(self).serialize(serializer)
    }
}

/// Options for collecting cause messages from an error report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CauseCollectOptions {
    /// Maximum depth of causes to collect.
    pub max_depth: usize,
    /// Whether to detect cycles in the cause chain.
    pub detect_cycle: bool,
}

impl Default for CauseCollectOptions {
    fn default() -> Self {
        Self {
            max_depth: 16,
            detect_cycle: true,
        }
    }
}

impl CauseCollectOptions {
    /// Sets the maximum depth for cause collection.
    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Enables or disables cycle detection during cause collection.
    pub fn with_cycle_detection(mut self, detect_cycle: bool) -> Self {
        self.detect_cycle = detect_cycle;
        self
    }
}
