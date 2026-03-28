#[path = "source_error/util.rs"]
mod util;
use super::*;
use util::{error_addr, is_report_wrapper_type};

/// Iterator over source errors with depth/cycle control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceErrorEntry {
    pub message: String,
    pub type_name: Option<String>,
    pub depth: usize,
}

/// Iterator over source errors in a report.
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
    pub(crate) fn new_origin(
        report: &'a crate::report::Report<impl Error + 'static>,
        options: CauseCollectOptions,
    ) -> Self {
        Self {
            walk: ReportSourceErrorTraversal::from_origin_report(report, options),
        }
    }

    pub(crate) fn new_diagnostic(
        report: &'a crate::report::Report<impl Error + 'static>,
        options: CauseCollectOptions,
    ) -> Self {
        Self {
            walk: ReportSourceErrorTraversal::from_diagnostic_report(report, options),
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

enum TraversalAction<'a> {
    Return(SourceErrorVisit<'a>, Option<SourceErrorNode<'a>>),
    PopContinue,
    StopCycle,
}

pub(crate) struct ReportSourceErrorTraversal<'a> {
    stack: Vec<SourceErrorNode<'a>>,
    options: CauseCollectOptions,
    seen: SeenErrorAddrs,
    state: CauseTraversalState,
}

impl<'a> ReportSourceErrorTraversal<'a> {
    pub(crate) fn from_origin_report(
        report: &'a crate::report::Report<impl Error + 'static>,
        options: CauseCollectOptions,
    ) -> Self {
        let mut stack = Vec::new();
        if let Some(inner_source) = report.inner().source() {
            stack.push(SourceErrorNode::error(inner_source, 0));
        }
        if let Some(source_errors) = report
            .diagnostics()
            .and_then(|diag| diag.origin_source_errors.as_ref())
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

    pub(crate) fn from_diagnostic_report(
        report: &'a crate::report::Report<impl Error + 'static>,
        options: CauseCollectOptions,
    ) -> Self {
        let mut stack = Vec::new();
        if let Some(source_errors) = report
            .diagnostics()
            .and_then(|diag| diag.diagnostic_source_errors.as_ref())
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
        loop {
            let stack = &mut self.stack;
            let seen = &mut self.seen;
            let state = &mut self.state;
            let options = self.options;
            let action = {
                let node = stack.last_mut()?;
                match node {
                    SourceErrorNode::Chain { items, depth } => {
                        report_visit_chain_node(items, *depth, options, seen, state)
                    }
                    SourceErrorNode::Error { current, depth } => {
                        report_visit_error_node(current, depth, options, seen, state)
                    }
                }
            };

            match action {
                TraversalAction::Return(visit, push) => {
                    if let Some(push) = push {
                        stack.push(push);
                    }
                    return Some(visit);
                }
                TraversalAction::PopContinue => {
                    stack.pop();
                }
                TraversalAction::StopCycle => {
                    state.cycle_detected = true;
                    stack.clear();
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
        loop {
            let stack = &mut self.stack;
            let seen = &mut self.seen;
            let state = &mut self.state;
            let action = {
                let node = stack.last_mut()?;
                match node {
                    SourceErrorNode::Chain { items, depth } => {
                        chain_visit_chain_node(items, *depth, seen)
                    }
                    SourceErrorNode::Error { .. } => TraversalAction::PopContinue,
                }
            };

            match action {
                TraversalAction::Return(visit, push) => {
                    if let Some(push) = push {
                        stack.push(push);
                    }
                    return Some(visit);
                }
                TraversalAction::PopContinue => {
                    stack.pop();
                }
                TraversalAction::StopCycle => {
                    state.cycle_detected = true;
                    stack.clear();
                    return None;
                }
            }
        }
    }
}

fn report_visit_chain_node<'a>(
    items: &mut core::slice::Iter<'a, SourceErrorItem>,
    depth: usize,
    options: CauseCollectOptions,
    seen: &mut SeenErrorAddrs,
    state: &mut CauseTraversalState,
) -> TraversalAction<'a> {
    if depth >= options.max_depth {
        state.truncated = true;
        return TraversalAction::PopContinue;
    }
    let Some(item) = items.next() else {
        return TraversalAction::PopContinue;
    };
    if options.detect_cycle && !seen.insert(error_addr(item.error.as_ref())) {
        return TraversalAction::StopCycle;
    }
    let push = item.source.as_ref().and_then(|source| {
        if depth + 1 < options.max_depth {
            Some(SourceErrorNode::chain(source.items.iter(), depth + 1))
        } else {
            state.truncated = true;
            None
        }
    });
    TraversalAction::Return(SourceErrorVisit::Item { item, depth }, push)
}

fn report_visit_error_node<'a>(
    current: &mut Option<&'a (dyn Error + 'static)>,
    depth: &mut usize,
    options: CauseCollectOptions,
    seen: &mut SeenErrorAddrs,
    state: &mut CauseTraversalState,
) -> TraversalAction<'a> {
    if *depth >= options.max_depth {
        state.truncated = true;
        return TraversalAction::PopContinue;
    }
    let Some(error) = current.take() else {
        return TraversalAction::PopContinue;
    };
    if options.detect_cycle && !seen.insert(error_addr(error)) {
        return TraversalAction::StopCycle;
    }
    *current = error.source();
    let entry_depth = *depth;
    *depth += 1;
    TraversalAction::Return(
        SourceErrorVisit::Error {
            error,
            depth: entry_depth,
        },
        None,
    )
}

fn chain_visit_chain_node<'a>(
    items: &mut core::slice::Iter<'a, SourceErrorItem>,
    depth: usize,
    seen: &mut SeenErrorAddrs,
) -> TraversalAction<'a> {
    let Some(item) = items.next() else {
        return TraversalAction::PopContinue;
    };
    if !seen.insert(error_addr(item.error.as_ref())) {
        return TraversalAction::StopCycle;
    }
    let push = item
        .source
        .as_ref()
        .map(|source| SourceErrorNode::chain(source.items.iter(), depth + 1));
    TraversalAction::Return(SourceErrorVisit::Item { item, depth }, push)
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

#[derive(Debug, Default)]
pub(crate) struct ColdData {
    pub(crate) metadata: ReportMetadata,
    pub(crate) diagnostics: DiagnosticBag,
}

#[derive(Debug, Default)]
pub(crate) struct DiagnosticBag {
    #[cfg(feature = "trace")]
    pub(crate) trace: Option<ReportTrace>,
    pub(crate) stack_trace: Option<StackTrace>,
    pub(crate) attachments: Vec<Attachment>,
    pub(crate) display_causes: Option<DisplayCauseChain>,
    pub(crate) origin_source_errors: Option<SourceErrorChain>,
    pub(crate) diagnostic_source_errors: Option<SourceErrorChain>,
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
    pub context: Vec<(StaticRefStr, AttachmentValue)>,
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
    pub items: Vec<Arc<dyn Display + Send + Sync + 'static>>,
    pub truncated: bool,
    pub cycle_detected: bool,
}

fn display_cause_strings(items: &[Arc<dyn Display + Send + Sync + 'static>]) -> Vec<String> {
    items.iter().map(ToString::to_string).collect()
}

impl Clone for DisplayCauseChain {
    fn clone(&self) -> Self {
        Self {
            items: self.items.clone(),
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

/// Runtime source-error chain captured in diagnostic bag.
#[derive(Debug, Clone)]
pub struct SourceErrorItem {
    pub error: Arc<dyn Error + Send + Sync + 'static>,
    pub type_name: Option<StaticRefStr>,
    pub source: Option<Arc<SourceErrorChain>>,
}

impl SourceErrorItem {
    /// Creates a source error item from an error value.
    pub fn new<T>(error: T) -> Self
    where
        T: Error + Send + Sync + 'static,
    {
        Self {
            error: Arc::new(error),
            type_name: Some(any::type_name::<T>().into()),
            source: None,
        }
    }

    pub(crate) fn with_source(mut self, source: Option<Arc<SourceErrorChain>>) -> Self {
        self.source = source;
        self
    }

    pub(crate) fn display_type_name(&self) -> Option<&str> {
        let type_name = self.type_name.as_deref()?;
        if is_report_wrapper_type(type_name) {
            None
        } else {
            Some(type_name)
        }
    }

    fn from_error<T>(error: T, options: CauseCollectOptions) -> (Self, bool)
    where
        T: Error + Send + Sync + 'static,
    {
        let (source, state) = SourceErrorChain::from_borrowed_srcs(error.source(), options);
        let item = Self::new(error).with_source(source);
        (item, state.cycle_detected)
    }
}

/// Hierarchical source-error chain captured in diagnostics.
pub struct SourceErrorChain {
    pub items: Arc<[SourceErrorItem]>,
    pub truncated: bool,
    pub cycle_detected: bool,
}

impl Default for SourceErrorChain {
    fn default() -> Self {
        Self {
            items: Vec::new().into(),
            truncated: false,
            cycle_detected: false,
        }
    }
}

/// Iterator over flattened entries in a source-error chain.
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
