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
    pub(crate) stack: Vec<SourceErrorFrame<'a>>,
    pub(crate) options: CauseCollectOptions,
    pub(crate) seen: SeenErrorAddrs,
    pub(crate) state: CauseTraversalState,
}

pub(crate) enum SourceErrorFrame<'a> {
    Chain {
        items: core::slice::Iter<'a, SourceErrorItem>,
        depth: usize,
    },
    Error {
        current: Option<&'a (dyn Error + 'static)>,
        depth: usize,
    },
}

impl<'a> SourceErrorFrame<'a> {
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
    /// Returns traversal state observed so far.
    pub fn state(&self) -> CauseTraversalState {
        self.state
    }
}

impl<'a> Iterator for ReportSourceErrorIter<'a> {
    type Item = SourceErrorEntry;

    fn next(&mut self) -> Option<Self::Item> {
        enum NextAction<'a> {
            Return {
                entry: SourceErrorEntry,
                push: Option<SourceErrorFrame<'a>>,
            },
            PopContinue,
            StopCycle,
        }

        loop {
            let action = {
                let Some(frame) = self.stack.last_mut() else {
                    return None;
                };
                match frame {
                    SourceErrorFrame::Chain { items, depth } => {
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
                                                Some(SourceErrorFrame::chain(
                                                    source.items.iter(),
                                                    *depth + 1,
                                                ))
                                            } else {
                                                self.state.truncated = true;
                                                None
                                            }
                                        });
                                        NextAction::Return {
                                            entry: SourceErrorEntry {
                                                message: item.error.to_string(),
                                                type_name: item
                                                    .type_name
                                                    .as_ref()
                                                    .map(|type_name| type_name.to_string()),
                                                depth: *depth,
                                            },
                                            push,
                                        }
                                    }
                                }
                                None => NextAction::PopContinue,
                            }
                        }
                    }
                    SourceErrorFrame::Error { current, depth } => {
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
                                        let entry = SourceErrorEntry {
                                            message: error.to_string(),
                                            type_name: None,
                                            depth: *depth,
                                        };
                                        *current = error.source();
                                        *depth += 1;
                                        NextAction::Return {
                                            entry,
                                            push: None,
                                        }
                                    }
                                }
                                None => NextAction::PopContinue,
                            }
                        }
                    }
                }
            };

            match action {
                NextAction::Return { entry, push } => {
                    if let Some(push) = push {
                        self.stack.push(push);
                    }
                    return Some(entry);
                }
                NextAction::PopContinue => {
                    self.stack.pop();
                    continue;
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

impl Clone for DisplayCauseChain {
    fn clone(&self) -> Self {
        Self {
            items: self
                .items
                .iter()
                .map(|item| Box::new(item.to_string()) as Box<dyn Display + 'static>)
                .collect(),
            truncated: self.truncated,
            cycle_detected: self.cycle_detected,
        }
    }
}

impl core::fmt::Debug for DisplayCauseChain {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let items: Vec<String> = self.items.iter().map(ToString::to_string).collect();
        f.debug_struct("DisplayCauseChain")
            .field("items", &items)
            .field("truncated", &self.truncated)
            .field("cycle_detected", &self.cycle_detected)
            .finish()
    }
}

impl PartialEq for DisplayCauseChain {
    fn eq(&self, other: &Self) -> bool {
        self.items
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            == other
                .items
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
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
        #[derive(serde::Serialize)]
        struct Helper {
            items: Vec<String>,
            truncated: bool,
            cycle_detected: bool,
        }
        Helper {
            items: self.items.iter().map(ToString::to_string).collect(),
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
        let items: Vec<(String, Option<String>)> = self
            .items
            .iter()
            .map(|item| {
                (
                    item.error.to_string(),
                    item.type_name
                        .as_ref()
                        .map(|type_name| type_name.to_string()),
                )
            })
            .collect();
        f.debug_struct("SourceErrorChain")
            .field("items", &items)
            .field("truncated", &self.truncated)
            .field("cycle_detected", &self.cycle_detected)
            .finish()
    }
}

impl PartialEq for SourceErrorChain {
    fn eq(&self, other: &Self) -> bool {
        self.items
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
            .collect::<Vec<_>>()
            == other
                .items
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
                .collect::<Vec<_>>()
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
        let state = source_chain_state(&other);
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
        mut next: Option<&dyn Error>,
        options: CauseCollectOptions,
    ) -> (Option<Box<SourceErrorChain>>, CauseTraversalState) {
        let mut seeds = Vec::new();
        let mut state = CauseTraversalState::default();
        let mut seen = SeenErrorAddrs::new();
        let mut depth = 0usize;

        while let Some(error) = next {
            if depth >= options.max_depth {
                state.truncated = true;
                break;
            }
            let addr = error_addr(error);
            if options.detect_cycle {
                if seen.contains(addr) {
                    state.cycle_detected = true;
                    break;
                }
            }
            seeds.push(SourceErrorSeed {
                message: error.to_string(),
                type_name: None,
            });
            if options.detect_cycle {
                let _ = seen.insert(addr);
            }
            depth += 1;
            next = error.source();
        }

        let mut chain = None;
        for seed in seeds.into_iter().rev() {
            let item = SourceErrorItem {
                error: Box::new(StringError(seed.message)),
                type_name: seed.type_name,
                source: chain,
            };
            chain = Some(Box::new(SourceErrorChain {
                items: vec![item],
                truncated: false,
                cycle_detected: false,
            }));
        }

        if let Some(chain) = chain.as_mut() {
            chain.truncated = state.truncated;
            chain.cycle_detected = state.cycle_detected;
        } else if state.truncated || state.cycle_detected {
            chain = Some(Box::new(SourceErrorChain {
                items: Vec::new(),
                truncated: state.truncated,
                cycle_detected: state.cycle_detected,
            }));
        }

        (chain, state)
    }
}

struct SourceErrorSeed {
    message: String,
    type_name: Option<Cow<'static, str>>,
}

pub struct SourceErrorChainEntries<'a> {
    stack: Vec<SourceErrorChainFrame<'a>>,
}

enum SourceErrorChainFrame<'a> {
    Chain {
        items: core::slice::Iter<'a, SourceErrorItem>,
        depth: usize,
    },
}

impl<'a> SourceErrorChainEntries<'a> {
    fn new(chain: &'a SourceErrorChain) -> Self {
        Self {
            stack: vec![SourceErrorChainFrame::Chain {
                items: chain.items.iter(),
                depth: 0,
            }],
        }
    }
}

impl<'a> Iterator for SourceErrorChainEntries<'a> {
    type Item = SourceErrorEntry;

    fn next(&mut self) -> Option<Self::Item> {
        enum NextAction<'a> {
            Return {
                entry: SourceErrorEntry,
                push: Option<SourceErrorChainFrame<'a>>,
            },
            PopContinue,
        }

        loop {
            let action = {
                let Some(frame) = self.stack.last_mut() else {
                    return None;
                };
                match frame {
                    SourceErrorChainFrame::Chain { items, depth } => match items.next() {
                        Some(item) => NextAction::Return {
                            entry: SourceErrorEntry {
                                message: item.error.to_string(),
                                type_name: item
                                    .type_name
                                    .as_ref()
                                    .map(|type_name| type_name.to_string()),
                                depth: *depth,
                            },
                            push: item.source.as_ref().map(|source| {
                                SourceErrorChainFrame::Chain {
                                    items: source.items.iter(),
                                    depth: *depth + 1,
                                }
                            }),
                        },
                        None => NextAction::PopContinue,
                    },
                }
            };

            match action {
                NextAction::Return { entry, push } => {
                    if let Some(push) = push {
                        self.stack.push(push);
                    }
                    return Some(entry);
                }
                NextAction::PopContinue => {
                    self.stack.pop();
                }
            }
        }
    }
}

fn source_chain_state(chain: &SourceErrorChain) -> CauseTraversalState {
    let mut state = CauseTraversalState {
        truncated: chain.truncated,
        cycle_detected: chain.cycle_detected,
    };
    for item in &chain.items {
        if let Some(source) = item.source.as_ref() {
            let nested = source_chain_state(source);
            state.truncated |= nested.truncated;
            state.cycle_detected |= nested.cycle_detected;
        }
    }
    state
}

fn error_addr(error: &dyn Error) -> usize {
    let ptr = (error as *const dyn Error) as *const ();
    ptr as usize
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
        #[derive(serde::Serialize)]
        struct Item {
            message: String,
            r#type: Option<String>,
            source: Option<Box<Helper>>,
        }
        #[derive(serde::Serialize)]
        struct Helper {
            items: Vec<Item>,
            truncated: bool,
            cycle_detected: bool,
        }
        fn serialize_chain(chain: &SourceErrorChain) -> Helper {
            Helper {
                items: chain
                    .items
                    .iter()
                    .map(|v| Item {
                        message: v.error.to_string(),
                        r#type: v.type_name.as_ref().map(|type_name| type_name.to_string()),
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
