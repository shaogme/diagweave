#[path = "source_error/util.rs"]
mod util;
use super::*;
use crate::utils::{FastSet, fast_set_with_capacity};
use alloc::borrow::ToOwned;
use util::{error_addr, is_report_wrapper_type};

pub(crate) type SourceNodeId = usize;

/// Iterator over source errors with depth/cycle control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceErrorEntry {
    pub message: String,
    pub type_name: Option<String>,
    pub display_type_name: Option<String>,
    pub depth: usize,
}

#[derive(Default)]
struct PathSeenErrorAddrs {
    // Active DFS path represented as (depth, addr).
    path: Vec<(usize, ErrorIdentity)>,
    seen: FastSet<ErrorIdentity>,
}

impl PathSeenErrorAddrs {
    fn enter(&mut self, depth: usize, addr: ErrorIdentity) -> bool {
        while self.path.last().is_some_and(|(d, _)| *d >= depth) {
            if let Some((_, popped)) = self.path.pop() {
                self.seen.remove(&popped);
            }
        }
        if self.seen.contains(&addr) {
            return false;
        }
        self.path.push((depth, addr));
        self.seen.insert(addr);
        true
    }
}

trait SourceArenaChain: Sized {
    type Item;

    fn roots(&self) -> &[SourceNodeId];
    fn node(&self, id: SourceNodeId) -> Option<&Self::Item>;
    fn error_ref(item: &Self::Item) -> &(dyn Error + Send + Sync + 'static);
    fn type_name_for_entry_raw(item: &Self::Item) -> Option<&str>;
    fn source_roots(item: &Self::Item) -> &[SourceNodeId];
}

/// Iterator over source errors in a report.
pub struct ReportSourceErrorIter<'a> {
    walk: ReportSourceErrorTraversal<'a>,
}

enum ArenaNode<'a, C>
where
    C: SourceArenaChain,
{
    Chain {
        chain: &'a C,
        ids: &'a [SourceNodeId],
        index: usize,
        depth: usize,
    },
    Error {
        current: Option<&'a (dyn Error + 'static)>,
        depth: usize,
    },
}

impl<'a, C> ArenaNode<'a, C>
where
    C: SourceArenaChain,
{
    fn chain(chain: &'a C, ids: &'a [SourceNodeId], depth: usize) -> Self {
        Self::Chain {
            chain,
            ids,
            index: 0,
            depth,
        }
    }

    fn error(error: &'a (dyn Error + 'static), depth: usize) -> Self {
        Self::Error {
            current: Some(error),
            depth,
        }
    }
}

enum SourceErrorVisit<'a, C>
where
    C: SourceArenaChain,
{
    Error {
        error: &'a (dyn Error + 'static),
        depth: usize,
    },
    Item {
        item: &'a C::Item,
        depth: usize,
    },
}

enum TraversalAction<'a, C>
where
    C: SourceArenaChain,
{
    Return(SourceErrorVisit<'a, C>, Option<ArenaNode<'a, C>>),
    PopContinue,
}

struct ReportSourceErrorTraversalImpl<'a, C>
where
    C: SourceArenaChain,
{
    stack: Vec<ArenaNode<'a, C>>,
    options: CauseCollectOptions,
    hide_report_wrapper_types: bool,
    path_seen: PathSeenErrorAddrs,
    state: CauseTraversalState,
}

impl<'a, C> ReportSourceErrorTraversalImpl<'a, C>
where
    C: SourceArenaChain,
{
    fn with_stack(
        stack: Vec<ArenaNode<'a, C>>,
        options: CauseCollectOptions,
        hide_report_wrapper_types: bool,
    ) -> Self {
        Self {
            stack,
            options,
            hide_report_wrapper_types,
            path_seen: PathSeenErrorAddrs::default(),
            state: CauseTraversalState::default(),
        }
    }

    fn state(&self) -> CauseTraversalState {
        self.state
    }

    fn next_entry(&mut self) -> Option<SourceErrorEntry> {
        let hide_report_wrapper_types = self.hide_report_wrapper_types;
        self.next_visit()
            .map(|visit| SourceErrorEntry::from_visit::<C>(visit, hide_report_wrapper_types))
    }

    fn next_visit(&mut self) -> Option<SourceErrorVisit<'a, C>> {
        loop {
            let options = self.options;
            let action = {
                let node = self.stack.last_mut()?;
                match node {
                    ArenaNode::Chain {
                        chain,
                        ids,
                        index,
                        depth,
                    } => report_visit_chain_node::<C>(
                        chain,
                        ids,
                        index,
                        *depth,
                        options,
                        &mut self.path_seen,
                        &mut self.state,
                    ),
                    ArenaNode::Error { current, depth } => report_visit_error_node::<C>(
                        current,
                        depth,
                        options,
                        &mut self.path_seen,
                        &mut self.state,
                    ),
                }
            };

            match action {
                TraversalAction::Return(visit, push) => {
                    if let Some(push) = push {
                        self.stack.push(push);
                    }
                    return Some(visit);
                }
                TraversalAction::PopContinue => {
                    self.stack.pop();
                }
            }
        }
    }
}

struct ReportSourceErrorTraversal<'a> {
    walk: ReportSourceErrorTraversalImpl<'a, SourceErrorChain>,
}

fn report_traversal_from_chain<'a, C>(
    source_errors: Option<&'a C>,
    inner_source: Option<&'a (dyn Error + 'static)>,
    options: CauseCollectOptions,
    hide_report_wrapper_types: bool,
) -> ReportSourceErrorTraversalImpl<'a, C>
where
    C: SourceArenaChain,
{
    let mut stack = Vec::new();
    if let Some(error) = inner_source {
        stack.push(ArenaNode::<C>::error(error, 0));
    }
    if let Some(chain) = source_errors {
        stack.push(ArenaNode::<C>::chain(chain, chain.roots(), 0));
    }
    ReportSourceErrorTraversalImpl::with_stack(stack, options, hide_report_wrapper_types)
}

impl<'a> ReportSourceErrorTraversal<'a> {
    fn from_report(
        report: &'a crate::report::Report<impl Error + 'static>,
        options: CauseCollectOptions,
        source_errors: Option<&'a SourceErrorChain>,
        include_inner_source: bool,
        hide_report_wrapper_types: bool,
    ) -> Self {
        let inner_source = if include_inner_source {
            report.inner().source()
        } else {
            None
        };
        Self {
            walk: report_traversal_from_chain::<SourceErrorChain>(
                source_errors,
                inner_source,
                options,
                hide_report_wrapper_types,
            ),
        }
    }

    fn from_origin_report(
        report: &'a crate::report::Report<impl Error + 'static>,
        options: CauseCollectOptions,
    ) -> Self {
        Self::from_report(
            report,
            options,
            report
                .diagnostics()
                .and_then(|diag| diag.origin_source_errors.as_ref()),
            true,
            true,
        )
    }

    fn from_diagnostic_report(
        report: &'a crate::report::Report<impl Error + 'static>,
        options: CauseCollectOptions,
    ) -> Self {
        Self::from_report(
            report,
            options,
            report
                .diagnostics()
                .and_then(|diag| diag.diagnostic_source_errors.as_ref()),
            false,
            false,
        )
    }

    fn state(&self) -> CauseTraversalState {
        self.walk.state()
    }

    fn next_entry(&mut self) -> Option<SourceErrorEntry> {
        self.walk.next_entry()
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

struct SourceErrorChainTraversal<'a, C>
where
    C: SourceArenaChain,
{
    stack: Vec<ArenaNode<'a, C>>,
    hide_report_wrapper_types: bool,
    path_seen: PathSeenErrorAddrs,
    state: CauseTraversalState,
}

impl<'a, C> SourceErrorChainTraversal<'a, C>
where
    C: SourceArenaChain,
{
    fn from_chain(chain: &'a C, hide_report_wrapper_types: bool) -> Self {
        Self {
            stack: vec![ArenaNode::<C>::chain(chain, chain.roots(), 0)],
            hide_report_wrapper_types,
            path_seen: PathSeenErrorAddrs::default(),
            state: CauseTraversalState::default(),
        }
    }

    fn next_entry(&mut self) -> Option<SourceErrorEntry> {
        let hide_report_wrapper_types = self.hide_report_wrapper_types;
        self.next_visit()
            .map(|visit| SourceErrorEntry::from_visit::<C>(visit, hide_report_wrapper_types))
    }

    fn next_visit(&mut self) -> Option<SourceErrorVisit<'a, C>> {
        loop {
            let action = {
                let node = self.stack.last_mut()?;
                match node {
                    ArenaNode::Chain {
                        chain,
                        ids,
                        index,
                        depth,
                    } => chain_visit_chain_node::<C>(
                        chain,
                        ids,
                        index,
                        *depth,
                        &mut self.path_seen,
                        &mut self.state,
                    ),
                    ArenaNode::Error { .. } => TraversalAction::PopContinue,
                }
            };

            match action {
                TraversalAction::Return(visit, push) => {
                    if let Some(push) = push {
                        self.stack.push(push);
                    }
                    return Some(visit);
                }
                TraversalAction::PopContinue => {
                    self.stack.pop();
                }
            }
        }
    }
}

fn report_visit_chain_node<'a, C>(
    chain: &'a C,
    ids: &'a [SourceNodeId],
    index: &mut usize,
    depth: usize,
    options: CauseCollectOptions,
    path_seen: &mut PathSeenErrorAddrs,
    state: &mut CauseTraversalState,
) -> TraversalAction<'a, C>
where
    C: SourceArenaChain,
{
    if depth >= options.max_depth {
        state.truncated = true;
        return TraversalAction::PopContinue;
    }
    let Some(&node_id) = ids.get(*index) else {
        return TraversalAction::PopContinue;
    };
    *index += 1;
    let Some(item) = chain.node(node_id) else {
        return TraversalAction::PopContinue;
    };
    if options.detect_cycle && !path_seen.enter(depth, error_addr(C::error_ref(item))) {
        state.cycle_detected = true;
        return TraversalAction::PopContinue;
    }
    let source_ids = C::source_roots(item);
    let push = if source_ids.is_empty() {
        None
    } else if depth + 1 < options.max_depth {
        Some(ArenaNode::<C>::chain(chain, source_ids, depth + 1))
    } else {
        state.truncated = true;
        None
    };
    TraversalAction::Return(SourceErrorVisit::Item { item, depth }, push)
}

fn report_visit_error_node<'a, C>(
    current: &mut Option<&'a (dyn Error + 'static)>,
    depth: &mut usize,
    options: CauseCollectOptions,
    path_seen: &mut PathSeenErrorAddrs,
    state: &mut CauseTraversalState,
) -> TraversalAction<'a, C>
where
    C: SourceArenaChain,
{
    if *depth >= options.max_depth {
        state.truncated = true;
        return TraversalAction::PopContinue;
    }
    let Some(error) = current.take() else {
        return TraversalAction::PopContinue;
    };
    if options.detect_cycle && !path_seen.enter(*depth, error_addr(error)) {
        state.cycle_detected = true;
        return TraversalAction::PopContinue;
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

fn chain_visit_chain_node<'a, C>(
    chain: &'a C,
    ids: &'a [SourceNodeId],
    index: &mut usize,
    depth: usize,
    path_seen: &mut PathSeenErrorAddrs,
    state: &mut CauseTraversalState,
) -> TraversalAction<'a, C>
where
    C: SourceArenaChain,
{
    let Some(&node_id) = ids.get(*index) else {
        return TraversalAction::PopContinue;
    };
    *index += 1;
    let Some(item) = chain.node(node_id) else {
        return TraversalAction::PopContinue;
    };
    if !path_seen.enter(depth, error_addr(C::error_ref(item))) {
        state.cycle_detected = true;
        return TraversalAction::PopContinue;
    }
    let source_ids = C::source_roots(item);
    let push = if source_ids.is_empty() {
        None
    } else {
        Some(ArenaNode::<C>::chain(chain, source_ids, depth + 1))
    };
    TraversalAction::Return(SourceErrorVisit::Item { item, depth }, push)
}

impl SourceErrorEntry {
    fn from_visit<C>(visit: SourceErrorVisit<'_, C>, hide_report_wrapper_types: bool) -> Self
    where
        C: SourceArenaChain,
    {
        match visit {
            SourceErrorVisit::Error { error, depth } => Self {
                message: error.to_string(),
                type_name: None,
                display_type_name: None,
                depth,
            },
            SourceErrorVisit::Item { item, depth } => Self {
                message: C::error_ref(item).to_string(),
                type_name: C::type_name_for_entry_raw(item).map(ToOwned::to_owned),
                display_type_name: C::type_name_for_entry_raw(item)
                    .and_then(|name| {
                        if hide_report_wrapper_types && is_report_wrapper_type(name) {
                            None
                        } else {
                            Some(name)
                        }
                    })
                    .map(ToOwned::to_owned),
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
    inline: Vec<ErrorIdentity>,
    spill: Option<FastSet<ErrorIdentity>>,
}

impl SeenErrorAddrs {
    pub(crate) fn new() -> Self {
        Self {
            inline: Vec::with_capacity(8),
            spill: None,
        }
    }

    pub(crate) fn insert(&mut self, addr: ErrorIdentity) -> bool {
        if let Some(spill) = self.spill.as_mut() {
            return spill.insert(addr);
        }
        if self.contains(addr) {
            return false;
        }
        if self.inline.len() < 8 {
            self.inline.push(addr);
            return true;
        }
        let mut spill = fast_set_with_capacity(self.inline.len() * 2 + 1);
        spill.extend(self.inline.drain(..));
        spill.insert(addr);
        self.spill = Some(spill);
        true
    }

    pub(crate) fn contains(&self, addr: ErrorIdentity) -> bool {
        if let Some(spill) = self.spill.as_ref() {
            return spill.contains(&addr);
        }
        self.inline.contains(&addr)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ErrorIdentity {
    data: *const (),
    vtable: *const (),
}

impl ErrorIdentity {
    pub(crate) fn from_error(error: &dyn Error) -> Self {
        // Use both data and vtable pointers for identity to reduce false
        // positives in cycle detection when only data pointers alias.
        let raw = error as *const dyn Error;
        let (data, vtable): (*const (), *const ()) = unsafe { core::mem::transmute(raw) };
        Self { data, vtable }
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

struct DisplayAsDebug<'a>(&'a (dyn Display + Send + Sync + 'static));

impl core::fmt::Debug for DisplayAsDebug<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

struct DisplayCauseItemsDebug<'a>(&'a [Arc<dyn Display + Send + Sync + 'static>]);

impl core::fmt::Debug for DisplayCauseItemsDebug<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();
        for item in self.0 {
            list.entry(&DisplayAsDebug(item.as_ref()));
        }
        list.finish()
    }
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
        f.debug_struct("DisplayCauseChain")
            .field("items", &DisplayCauseItemsDebug(&self.items))
            .field("truncated", &self.truncated)
            .field("cycle_detected", &self.cycle_detected)
            .finish()
    }
}

impl PartialEq for DisplayCauseChain {
    fn eq(&self, other: &Self) -> bool {
        if self.truncated != other.truncated
            || self.cycle_detected != other.cycle_detected
            || self.items.len() != other.items.len()
        {
            return false;
        }
        self.items
            .iter()
            .zip(other.items.iter())
            .all(|(left, right)| left.to_string() == right.to_string())
    }
}

impl Eq for DisplayCauseChain {}

/// Runtime source-error node captured in diagnostics.
#[derive(Debug, Clone)]
pub struct SourceErrorItem {
    pub error: Arc<dyn Error + Send + Sync + 'static>,
    pub type_name: Option<StaticRefStr>,
    pub(crate) source_roots: Vec<SourceNodeId>,
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
            source_roots: Vec::new(),
        }
    }

    pub(crate) fn type_name_for_display(&self, hide_report_wrapper_types: bool) -> Option<&str> {
        let type_name = self.type_name.as_deref()?;
        if hide_report_wrapper_types && is_report_wrapper_type(type_name) {
            None
        } else {
            Some(type_name)
        }
    }
}

/// Arena-backed source-error chain captured in diagnostics.
pub struct SourceErrorChain {
    pub(crate) nodes: Arc<[SourceErrorItem]>,
    pub(crate) roots: Arc<[SourceNodeId]>,
    pub truncated: bool,
    pub cycle_detected: bool,
}

#[cfg(any(feature = "json", feature = "trace", feature = "otel"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExportedSourceErrorNode {
    pub message: String,
    pub type_name: Option<String>,
    pub source_roots: Vec<usize>,
}

#[cfg(any(feature = "json", feature = "trace", feature = "otel"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExportedSourceErrorChain {
    pub roots: Vec<usize>,
    pub nodes: Vec<ExportedSourceErrorNode>,
    pub truncated: bool,
    pub cycle_detected: bool,
}

impl Default for SourceErrorChain {
    fn default() -> Self {
        Self {
            nodes: Vec::new().into(),
            roots: Vec::new().into(),
            truncated: false,
            cycle_detected: false,
        }
    }
}

impl SourceArenaChain for SourceErrorChain {
    type Item = SourceErrorItem;

    fn roots(&self) -> &[SourceNodeId] {
        &self.roots
    }

    fn node(&self, id: SourceNodeId) -> Option<&Self::Item> {
        self.nodes.get(id)
    }

    fn error_ref(item: &Self::Item) -> &(dyn Error + Send + Sync + 'static) {
        item.error.as_ref()
    }

    fn type_name_for_entry_raw(item: &Self::Item) -> Option<&str> {
        item.type_name.as_deref()
    }

    fn source_roots(item: &Self::Item) -> &[SourceNodeId] {
        &item.source_roots
    }
}

pub struct SourceErrorItemIter<'a> {
    chain: &'a SourceErrorChain,
    index: usize,
}

impl<'a> Iterator for SourceErrorItemIter<'a> {
    type Item = &'a SourceErrorItem;

    fn next(&mut self) -> Option<Self::Item> {
        let id = *self.chain.roots.get(self.index)?;
        self.index += 1;
        self.chain.node(id)
    }
}

/// Iterator over flattened entries in a source-error chain.
pub struct SourceErrorChainEntries<'a> {
    walk: SourceErrorChainTraversal<'a, SourceErrorChain>,
}

impl<'a> SourceErrorChainEntries<'a> {
    pub(crate) fn new(chain: &'a SourceErrorChain, hide_report_wrapper_types: bool) -> Self {
        Self {
            walk: SourceErrorChainTraversal::from_chain(chain, hide_report_wrapper_types),
        }
    }
}

impl<'a> Iterator for SourceErrorChainEntries<'a> {
    type Item = SourceErrorEntry;

    fn next(&mut self) -> Option<Self::Item> {
        self.walk.next_entry()
    }
}
