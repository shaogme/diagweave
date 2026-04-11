use super::util::{error_addr, is_report_wrapper_type};
use super::*;

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

struct WalkerCore {
    options: CauseCollectOptions,
    path_seen: PathSeenErrorAddrs,
    state: CauseTraversalState,
}

impl WalkerCore {
    fn new(options: CauseCollectOptions) -> Self {
        Self {
            options,
            path_seen: PathSeenErrorAddrs::default(),
            state: CauseTraversalState::default(),
        }
    }

    fn options(&self) -> CauseCollectOptions {
        self.options
    }

    fn state(&self) -> CauseTraversalState {
        self.state
    }

    fn allow_depth(&mut self, depth: usize) -> bool {
        if depth >= self.options.max_depth {
            self.state.truncated = true;
            false
        } else {
            true
        }
    }

    fn allow_cycle(&mut self, depth: usize, addr: ErrorIdentity) -> bool {
        if self.options.detect_cycle && !self.path_seen.enter(depth, addr) {
            self.state.cycle_detected = true;
            false
        } else {
            true
        }
    }

    fn mark_truncated(&mut self) {
        self.state.truncated = true;
    }
}

struct ChainFrame<'a> {
    ids: &'a [SourceNodeId],
    index: usize,
    depth: usize,
}

impl<'a> ChainFrame<'a> {
    fn new(ids: &'a [SourceNodeId], depth: usize) -> Self {
        Self {
            ids,
            index: 0,
            depth,
        }
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

struct ChainWalker<'a> {
    chain: &'a SourceErrorChain,
    stack: Vec<ChainFrame<'a>>,
    core: WalkerCore,
}

impl<'a> ChainWalker<'a> {
    fn from_roots(
        chain: &'a SourceErrorChain,
        roots: &'a [SourceNodeId],
        options: CauseCollectOptions,
    ) -> Self {
        let mut stack = Vec::with_capacity(roots.len().min(options.max_depth));
        if !roots.is_empty() {
            stack.push(ChainFrame::new(roots, 0));
        }
        Self {
            chain,
            stack,
            core: WalkerCore::new(options),
        }
    }

    fn state(&self) -> CauseTraversalState {
        self.core.state()
    }

    fn next_visit(&mut self) -> Option<SourceErrorVisit<'a>> {
        loop {
            let options = self.core.options();
            let (item, depth, source_ids) = {
                let frame = self.stack.last_mut()?;

                if !self.core.allow_depth(frame.depth) {
                    self.stack.pop();
                    continue;
                }

                let Some(&node_id) = frame.ids.get(frame.index) else {
                    self.stack.pop();
                    continue;
                };
                frame.index += 1;

                let Some(item) = self.chain.nodes.get(node_id) else {
                    continue;
                };

                if !self
                    .core
                    .allow_cycle(frame.depth, error_addr(item.error.as_ref()))
                {
                    continue;
                }

                (item, frame.depth, &item.source_roots)
            };

            if !source_ids.is_empty() {
                if depth + 1 < options.max_depth {
                    self.stack.push(ChainFrame::new(source_ids, depth + 1));
                } else {
                    self.core.mark_truncated();
                }
            }

            return Some(SourceErrorVisit::Item { item, depth });
        }
    }
}

struct ErrorWalker<'a> {
    current: Option<&'a (dyn Error + 'static)>,
    depth: usize,
    core: WalkerCore,
}

impl<'a> ErrorWalker<'a> {
    fn new(current: Option<&'a (dyn Error + 'static)>, options: CauseCollectOptions) -> Self {
        Self {
            current,
            depth: 0,
            core: WalkerCore::new(options),
        }
    }

    fn state(&self) -> CauseTraversalState {
        self.core.state()
    }

    fn next_visit(&mut self) -> Option<SourceErrorVisit<'a>> {
        if !self.core.allow_depth(self.depth) {
            self.current = None;
            return None;
        }

        let error = self.current.take()?;

        if !self.core.allow_cycle(self.depth, error_addr(error)) {
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

struct ReportSourceErrorTraversalImpl<'a> {
    chain_walk: Option<ChainWalker<'a>>,
    error_walk: Option<ErrorWalker<'a>>,
    hide_report_wrapper_types: bool,
    finished_state: CauseTraversalState,
}

impl<'a> ReportSourceErrorTraversalImpl<'a> {
    fn with_walkers(
        chain_walk: Option<ChainWalker<'a>>,
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
        self.finished_state
    }

    fn next_entry(&mut self) -> Option<SourceErrorEntry<'a>> {
        let hide_report_wrapper_types = self.hide_report_wrapper_types;
        self.next_visit()
            .map(|visit| SourceErrorEntry::from_visit(visit, hide_report_wrapper_types))
    }

    fn next_visit(&mut self) -> Option<SourceErrorVisit<'a>> {
        if let Some(chain_walk) = self.chain_walk.as_mut() {
            if let Some(visit) = chain_walk.next_visit() {
                self.finished_state.merge_from(chain_walk.state());
                return Some(visit);
            }
            self.finished_state.merge_from(chain_walk.state());
            self.chain_walk = None;
        }

        if let Some(error_walk) = self.error_walk.as_mut() {
            if let Some(visit) = error_walk.next_visit() {
                self.finished_state.merge_from(error_walk.state());
                return Some(visit);
            }
            self.finished_state.merge_from(error_walk.state());
            self.error_walk = None;
        }

        None
    }
}

fn traversal_from_chain<'a>(
    source_errors: Option<&'a SourceErrorChain>,
    inner_source: Option<&'a (dyn Error + 'static)>,
    options: CauseCollectOptions,
    hide_report_wrapper_types: bool,
) -> ReportSourceErrorTraversalImpl<'a> {
    let chain_walk =
        source_errors.map(|chain| ChainWalker::from_roots(chain, &chain.roots, options));
    let error_walk = if inner_source.is_some() {
        Some(ErrorWalker::new(inner_source, options))
    } else {
        None
    };

    ReportSourceErrorTraversalImpl::with_walkers(chain_walk, error_walk, hide_report_wrapper_types)
}

#[derive(Clone, Copy)]
enum ReportSourceTraversalStrategy {
    Origin,
    Diagnostic,
}

impl ReportSourceTraversalStrategy {
    fn source_errors<E, State>(
        self,
        report: &crate::report::Report<E, State>,
    ) -> Option<&SourceErrorChain>
    where
        E: Error + 'static,
        State: crate::report::SeverityState,
    {
        match self {
            Self::Origin => report.diagnostics().origin_source_errors(),
            Self::Diagnostic => report.diagnostics().diagnostic_source_errors(),
        }
    }

    fn is_origin(self) -> bool {
        matches!(self, Self::Origin)
    }
}

fn traversal_from_report<'a, E, State>(
    report: &'a crate::report::Report<E, State>,
    options: CauseCollectOptions,
    strategy: ReportSourceTraversalStrategy,
) -> ReportSourceErrorTraversalImpl<'a>
where
    E: Error + 'static,
    State: crate::report::SeverityState,
{
    let source_errors = strategy.source_errors(report);
    let inner_source = if strategy.is_origin() {
        report.inner().source()
    } else {
        None
    };

    traversal_from_chain(source_errors, inner_source, options, strategy.is_origin())
}

/// Iterator over source errors in a report.
pub struct ReportSourceErrorIter<'a> {
    walk: ReportSourceErrorTraversalImpl<'a>,
}

impl<'a> ReportSourceErrorIter<'a> {
    pub(crate) fn new_origin<E, State>(
        report: &'a crate::report::Report<E, State>,
        options: CauseCollectOptions,
    ) -> Self
    where
        E: Error + 'static,
        State: crate::report::SeverityState,
    {
        Self {
            walk: traversal_from_report(report, options, ReportSourceTraversalStrategy::Origin),
        }
    }

    pub(crate) fn new_diagnostic<E, State>(
        report: &'a crate::report::Report<E, State>,
        options: CauseCollectOptions,
    ) -> Self
    where
        E: Error + 'static,
        State: crate::report::SeverityState,
    {
        Self {
            walk: traversal_from_report(report, options, ReportSourceTraversalStrategy::Diagnostic),
        }
    }

    /// Returns traversal state observed so far.
    pub fn state(&self) -> CauseTraversalState {
        self.walk.state()
    }
}

impl<'a> Iterator for ReportSourceErrorIter<'a> {
    type Item = SourceErrorEntry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.walk.next_entry()
    }
}

struct SourceErrorChainTraversal<'a> {
    walk: ChainWalker<'a>,
    hide_report_wrapper_types: bool,
}

impl<'a> SourceErrorChainTraversal<'a> {
    fn from_chain(chain: &'a SourceErrorChain, hide_report_wrapper_types: bool) -> Self {
        Self {
            walk: ChainWalker::from_roots(
                chain,
                &chain.roots,
                CauseCollectOptions {
                    max_depth: usize::MAX,
                    detect_cycle: true,
                },
            ),
            hide_report_wrapper_types,
        }
    }

    fn next_entry(&mut self) -> Option<SourceErrorEntry<'a>> {
        let hide_report_wrapper_types = self.hide_report_wrapper_types;
        self.walk
            .next_visit()
            .map(|visit| SourceErrorEntry::from_visit(visit, hide_report_wrapper_types))
    }
}

impl<'a> SourceErrorEntry<'a> {
    fn from_visit(visit: SourceErrorVisit<'a>, hide_report_wrapper_types: bool) -> Self {
        match visit {
            SourceErrorVisit::Error { error, depth } => Self {
                error,
                type_name: None,
                display_type_name: None,
                depth,
            },
            SourceErrorVisit::Item { item, depth } => {
                let raw_type_name = item.type_name.as_ref();
                let display_type_name = if hide_report_wrapper_types
                    && raw_type_name.is_some_and(is_report_wrapper_type)
                {
                    None
                } else {
                    raw_type_name
                };

                Self {
                    error: item.error.as_ref(),
                    type_name: raw_type_name,
                    display_type_name,
                    depth,
                }
            }
        }
    }
}

/// Iterator over flattened entries in a source-error chain.
pub struct SourceErrorChainEntries<'a> {
    walk: SourceErrorChainTraversal<'a>,
}

impl<'a> SourceErrorChainEntries<'a> {
    pub(crate) fn new(chain: &'a SourceErrorChain, hide_report_wrapper_types: bool) -> Self {
        Self {
            walk: SourceErrorChainTraversal::from_chain(chain, hide_report_wrapper_types),
        }
    }
}

impl<'a> Iterator for SourceErrorChainEntries<'a> {
    type Item = SourceErrorEntry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.walk.next_entry()
    }
}
