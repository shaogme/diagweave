use super::util::{error_addr, is_report_wrapper_type};
use super::*;
use alloc::borrow::ToOwned;

#[derive(Default)]
struct PathSeenErrorAddrs {
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

fn traversal_from_chain<'a, C>(
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
            walk: traversal_from_chain::<SourceErrorChain>(
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

    fn from_diag_report(
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

/// Iterator over source errors in a report.
pub struct ReportSourceErrorIter<'a> {
    walk: ReportSourceErrorTraversal<'a>,
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
            walk: ReportSourceErrorTraversal::from_diag_report(report, options),
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
