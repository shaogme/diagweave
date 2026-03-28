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

struct ChainFrame<'a, C>
where
    C: SourceArenaChain,
{
    chain: &'a C,
    ids: &'a [SourceNodeId],
    index: usize,
    depth: usize,
}

impl<'a, C> ChainFrame<'a, C>
where
    C: SourceArenaChain,
{
    fn new(chain: &'a C, ids: &'a [SourceNodeId], depth: usize) -> Self {
        Self {
            chain,
            ids,
            index: 0,
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

struct ChainWalker<'a, C>
where
    C: SourceArenaChain,
{
    stack: Vec<ChainFrame<'a, C>>,
    options: CauseCollectOptions,
    path_seen: PathSeenErrorAddrs,
    state: CauseTraversalState,
}

impl<'a, C> ChainWalker<'a, C>
where
    C: SourceArenaChain,
{
    fn from_roots(chain: &'a C, roots: &'a [SourceNodeId], options: CauseCollectOptions) -> Self {
        let mut stack = Vec::new();
        if !roots.is_empty() {
            stack.push(ChainFrame::new(chain, roots, 0));
        }
        Self {
            stack,
            options,
            path_seen: PathSeenErrorAddrs::default(),
            state: CauseTraversalState::default(),
        }
    }

    fn state(&self) -> CauseTraversalState {
        self.state
    }

    fn next_visit(&mut self) -> Option<SourceErrorVisit<'a, C>> {
        loop {
            let options = self.options;
            let (item, depth, chain, source_ids) = {
                let Some(frame) = self.stack.last_mut() else {
                    return None;
                };

                if frame.depth >= options.max_depth {
                    self.state.truncated = true;
                    self.stack.pop();
                    continue;
                }

                let Some(&node_id) = frame.ids.get(frame.index) else {
                    self.stack.pop();
                    continue;
                };
                frame.index += 1;

                let Some(item) = frame.chain.node(node_id) else {
                    continue;
                };

                if options.detect_cycle
                    && !self
                        .path_seen
                        .enter(frame.depth, error_addr(C::error_ref(item)))
                {
                    self.state.cycle_detected = true;
                    continue;
                }

                (item, frame.depth, frame.chain, C::source_roots(item))
            };

            if !source_ids.is_empty() {
                if depth + 1 < options.max_depth {
                    self.stack
                        .push(ChainFrame::new(chain, source_ids, depth + 1));
                } else {
                    self.state.truncated = true;
                }
            }

            return Some(SourceErrorVisit::Item { item, depth });
        }
    }
}

struct ErrorWalker<'a> {
    current: Option<&'a (dyn Error + 'static)>,
    depth: usize,
    options: CauseCollectOptions,
    path_seen: PathSeenErrorAddrs,
    state: CauseTraversalState,
}

impl<'a> ErrorWalker<'a> {
    fn new(current: Option<&'a (dyn Error + 'static)>, options: CauseCollectOptions) -> Self {
        Self {
            current,
            depth: 0,
            options,
            path_seen: PathSeenErrorAddrs::default(),
            state: CauseTraversalState::default(),
        }
    }

    fn state(&self) -> CauseTraversalState {
        self.state
    }

    fn next_visit<C>(&mut self) -> Option<SourceErrorVisit<'a, C>>
    where
        C: SourceArenaChain,
    {
        if self.depth >= self.options.max_depth {
            self.state.truncated = true;
            self.current = None;
            return None;
        }

        let error = self.current.take()?;

        if self.options.detect_cycle && !self.path_seen.enter(self.depth, error_addr(error)) {
            self.state.cycle_detected = true;
            return None;
        }

        self.current = error.source();
        let entry_depth = self.depth;
        self.depth += 1;

        Some(SourceErrorVisit::Error {
            error,
            depth: entry_depth,
        })
    }
}

fn merge_state(base: &mut CauseTraversalState, next: CauseTraversalState) {
    base.truncated |= next.truncated;
    base.cycle_detected |= next.cycle_detected;
}

struct ReportSourceErrorTraversalImpl<'a, C>
where
    C: SourceArenaChain,
{
    chain_walk: Option<ChainWalker<'a, C>>,
    error_walk: Option<ErrorWalker<'a>>,
    hide_report_wrapper_types: bool,
    finished_state: CauseTraversalState,
}

impl<'a, C> ReportSourceErrorTraversalImpl<'a, C>
where
    C: SourceArenaChain,
{
    fn with_walkers(
        chain_walk: Option<ChainWalker<'a, C>>,
        error_walk: Option<ErrorWalker<'a>>,
        hide_report_wrapper_types: bool,
    ) -> Self {
        Self {
            chain_walk,
            error_walk,
            hide_report_wrapper_types,
            finished_state: CauseTraversalState::default(),
        }
    }

    fn state(&self) -> CauseTraversalState {
        let mut state = self.finished_state;
        if let Some(walk) = self.chain_walk.as_ref() {
            merge_state(&mut state, walk.state());
        }
        if let Some(walk) = self.error_walk.as_ref() {
            merge_state(&mut state, walk.state());
        }
        state
    }

    fn next_entry(&mut self) -> Option<SourceErrorEntry> {
        let hide_report_wrapper_types = self.hide_report_wrapper_types;
        self.next_visit()
            .map(|visit| SourceErrorEntry::from_visit::<C>(visit, hide_report_wrapper_types))
    }

    fn next_visit(&mut self) -> Option<SourceErrorVisit<'a, C>> {
        if let Some(chain_walk) = self.chain_walk.as_mut() {
            if let Some(visit) = chain_walk.next_visit() {
                return Some(visit);
            }
            merge_state(&mut self.finished_state, chain_walk.state());
            self.chain_walk = None;
        }

        if let Some(error_walk) = self.error_walk.as_mut() {
            if let Some(visit) = error_walk.next_visit::<C>() {
                return Some(visit);
            }
            merge_state(&mut self.finished_state, error_walk.state());
            self.error_walk = None;
        }

        None
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
    let chain_walk =
        source_errors.map(|chain| ChainWalker::from_roots(chain, chain.roots(), options));
    let error_walk = if inner_source.is_some() {
        Some(ErrorWalker::new(inner_source, options))
    } else {
        None
    };

    ReportSourceErrorTraversalImpl::with_walkers(chain_walk, error_walk, hide_report_wrapper_types)
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
    walk: ChainWalker<'a, C>,
    hide_report_wrapper_types: bool,
}

impl<'a, C> SourceErrorChainTraversal<'a, C>
where
    C: SourceArenaChain,
{
    fn from_chain(chain: &'a C, hide_report_wrapper_types: bool) -> Self {
        Self {
            walk: ChainWalker::from_roots(
                chain,
                chain.roots(),
                CauseCollectOptions {
                    max_depth: usize::MAX,
                    detect_cycle: true,
                },
            ),
            hide_report_wrapper_types,
        }
    }

    fn next_entry(&mut self) -> Option<SourceErrorEntry> {
        let hide_report_wrapper_types = self.hide_report_wrapper_types;
        self.walk
            .next_visit()
            .map(|visit| SourceErrorEntry::from_visit::<C>(visit, hide_report_wrapper_types))
    }
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
