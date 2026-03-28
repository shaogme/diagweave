use super::*;
use crate::utils::fast_set_new;
use alloc::borrow::ToOwned;

pub(super) fn error_addr(error: &dyn Error) -> ErrorIdentity {
    ErrorIdentity::from_error(error)
}

pub(super) fn is_report_wrapper_type(type_name: &str) -> bool {
    let report_prefix = core::any::type_name::<crate::report::Report<()>>();
    let report_prefix = report_prefix
        .split_once('<')
        .map(|(prefix, _)| prefix)
        .unwrap_or(report_prefix);
    type_name.starts_with(report_prefix) && type_name[report_prefix.len()..].starts_with('<')
}

trait ArenaItemLike: Clone {
    fn from_borrowed_message(message: String) -> Self;
    fn source_roots(&self) -> &[SourceNodeId];
    fn source_roots_mut(&mut self) -> &mut Vec<SourceNodeId>;
    fn message_string(&self) -> String;
    fn type_name_str(&self) -> Option<&str>;
}

trait ArenaChainLike: Sized {
    type Item: ArenaItemLike;

    fn from_parts(
        nodes: Arc<[Self::Item]>,
        roots: Arc<[SourceNodeId]>,
        truncated: bool,
        cycle_detected: bool,
    ) -> Self;
    fn nodes(&self) -> &Arc<[Self::Item]>;
    fn roots(&self) -> &Arc<[SourceNodeId]>;
    fn truncated(&self) -> bool;
    fn cycle_detected(&self) -> bool;
}

impl ArenaItemLike for SourceErrorItem {
    fn from_borrowed_message(message: String) -> Self {
        Self {
            error: Arc::new(StringError(message)),
            type_name: None,
            source_roots: Vec::new(),
        }
    }

    fn source_roots(&self) -> &[SourceNodeId] {
        &self.source_roots
    }

    fn source_roots_mut(&mut self) -> &mut Vec<SourceNodeId> {
        &mut self.source_roots
    }

    fn message_string(&self) -> String {
        self.error.to_string()
    }

    fn type_name_str(&self) -> Option<&str> {
        self.type_name.as_deref()
    }
}

impl ArenaChainLike for SourceErrorChain {
    type Item = SourceErrorItem;

    fn from_parts(
        nodes: Arc<[Self::Item]>,
        roots: Arc<[SourceNodeId]>,
        truncated: bool,
        cycle_detected: bool,
    ) -> Self {
        Self {
            nodes,
            roots,
            truncated,
            cycle_detected,
        }
    }

    fn nodes(&self) -> &Arc<[Self::Item]> {
        &self.nodes
    }

    fn roots(&self) -> &Arc<[SourceNodeId]> {
        &self.roots
    }

    fn truncated(&self) -> bool {
        self.truncated
    }

    fn cycle_detected(&self) -> bool {
        self.cycle_detected
    }
}

fn collect_borrowed_items<I: ArenaItemLike>(
    next: Option<&dyn Error>,
    options: CauseCollectOptions,
) -> (Vec<I>, CauseTraversalState) {
    let Some(mut current) = next else {
        return (Vec::new(), CauseTraversalState::default());
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

        if options.detect_cycle {
            let addr = error_addr(current);
            if !seen.insert(addr) {
                state.cycle_detected = true;
                break;
            }
        }

        items.push(I::from_borrowed_message(current.to_string()));

        let Some(next) = current.source() else {
            break;
        };
        current = next;
        depth += 1;
    }

    (items, state)
}

fn chain_from_linear<C>(mut items: Vec<C::Item>, state: &CauseTraversalState) -> Option<Arc<C>>
where
    C: ArenaChainLike,
{
    if items.is_empty() {
        return None;
    }
    let len = items.len();
    for (idx, item) in items.iter_mut().enumerate() {
        if idx + 1 < len {
            *item.source_roots_mut() = vec![idx + 1];
        }
    }
    Some(Arc::new(C::from_parts(
        items.into(),
        vec![0].into(),
        state.truncated,
        state.cycle_detected,
    )))
}

fn shift_item<I: ArenaItemLike>(mut item: I, offset: usize) -> I {
    for id in item.source_roots_mut() {
        *id += offset;
    }
    item
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedNode {
    message: String,
    type_name: Option<String>,
    source_roots: Vec<Option<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedChain {
    roots: Vec<Option<usize>>,
    nodes: Vec<NormalizedNode>,
    truncated: bool,
    cycle_detected: bool,
}

fn normalize_chain_repr<C>(chain: &C) -> NormalizedChain
where
    C: ArenaChainLike,
{
    let nodes = chain.nodes();
    let mut order = Vec::with_capacity(nodes.len());
    let mut visited = vec![false; nodes.len()];
    let mut stack = Vec::new();

    for &root in chain.roots().iter().rev() {
        stack.push(root);
        while let Some(id) = stack.pop() {
            let Some(node) = nodes.get(id) else {
                continue;
            };
            if visited[id] {
                continue;
            }
            visited[id] = true;
            order.push(id);
            for &child in node.source_roots().iter().rev() {
                if child < nodes.len() && !visited[child] {
                    stack.push(child);
                }
            }
        }
    }

    for (id, seen) in visited.iter().enumerate() {
        if !seen {
            order.push(id);
        }
    }

    let remap = remap_order(&order, nodes.len());
    let map_id =
        |id: SourceNodeId| -> Option<usize> { if id < remap.len() { remap[id] } else { None } };

    let normalized_nodes = order
        .iter()
        .map(|&old_id| {
            let node = &nodes[old_id];
            NormalizedNode {
                message: node.message_string(),
                type_name: node.type_name_str().map(ToOwned::to_owned),
                source_roots: node.source_roots().iter().map(|&id| map_id(id)).collect(),
            }
        })
        .collect();

    let roots = chain.roots().iter().map(|&id| map_id(id)).collect();

    NormalizedChain {
        roots,
        nodes: normalized_nodes,
        truncated: chain.truncated(),
        cycle_detected: chain.cycle_detected(),
    }
}

fn remap_order(order: &[usize], len: usize) -> Vec<Option<usize>> {
    let mut remap = vec![None; len];
    for (new_id, &old_id) in order.iter().enumerate() {
        remap[old_id] = Some(new_id);
    }
    remap
}

fn chain_eq<C>(left: &C, right: &C) -> bool
where
    C: ArenaChainLike,
{
    normalize_chain_repr(left) == normalize_chain_repr(right)
}

pub(crate) fn append_source_chain(this: &mut SourceErrorChain, other: SourceErrorChain) {
    this.truncated |= other.truncated;
    this.cycle_detected |= other.cycle_detected;

    if this.roots.is_empty() {
        *this = other;
        return;
    }
    if other.roots.is_empty() {
        return;
    }

    let base = this.nodes.len();
    let mut nodes = Vec::with_capacity(this.nodes.len() + other.nodes.len());
    nodes.extend(this.nodes.iter().cloned());
    nodes.extend(other.nodes.iter().cloned().map(|n| shift_item(n, base)));

    let mut roots = Vec::with_capacity(this.roots.len() + other.roots.len());
    roots.extend(this.roots.iter().copied());
    roots.extend(other.roots.iter().map(|id| id + base));

    this.nodes = nodes.into();
    this.roots = roots.into();
}

pub(crate) fn limit_depth_source_chain(
    chain: &mut SourceErrorChain,
    options: CauseCollectOptions,
    depth: usize,
) -> bool {
    if depth >= options.max_depth {
        chain.roots = Vec::new().into();
        chain.truncated = true;
        return true;
    }

    let mut truncated = chain.truncated;
    let roots = chain.roots.clone();
    let nodes = Arc::make_mut(&mut chain.nodes);
    let mut stack: Vec<(SourceNodeId, usize)> =
        roots.iter().copied().map(|id| (id, depth)).collect();
    let mut visited = fast_set_new();

    while let Some((id, d)) = stack.pop() {
        if !visited.insert((id, d)) {
            continue;
        }
        let Some(node) = nodes.get_mut(id) else {
            continue;
        };
        if d + 1 >= options.max_depth {
            if !node.source_roots().is_empty() {
                node.source_roots_mut().clear();
                truncated = true;
            }
            continue;
        }
        for &child in node.source_roots().iter() {
            stack.push((child, d + 1));
        }
    }

    chain.truncated = truncated;
    truncated
}

impl Clone for SourceErrorChain {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
            roots: self.roots.clone(),
            truncated: self.truncated,
            cycle_detected: self.cycle_detected,
        }
    }
}

impl core::fmt::Debug for SourceErrorChain {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceErrorChain")
            .field("nodes_len", &self.nodes.len())
            .field("roots_len", &self.roots.len())
            .field("truncated", &self.truncated)
            .field("cycle_detected", &self.cycle_detected)
            .finish()
    }
}

impl PartialEq for SourceErrorChain {
    fn eq(&self, other: &Self) -> bool {
        chain_eq(self, other)
    }
}

impl Eq for SourceErrorChain {}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StringError(String);

impl Display for StringError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for StringError {}

impl SourceErrorChain {
    #[cfg(any(feature = "json", feature = "trace", feature = "otel"))]
    pub(crate) fn export_with_options(
        &self,
        hide_report_wrapper_types: bool,
    ) -> ExportedSourceErrorChain {
        let mut ids = Vec::new();
        let mut remap = vec![None; self.nodes.len()];
        let mut visited = fast_set_new();
        let mut stack: Vec<SourceNodeId> = self.roots.iter().copied().rev().collect();

        while let Some(id) = stack.pop() {
            if id >= self.nodes.len() || !visited.insert(id) {
                continue;
            }
            remap[id] = Some(ids.len());
            ids.push(id);
            for &child in self.nodes[id].source_roots.iter().rev() {
                stack.push(child);
            }
        }

        let roots = self
            .roots
            .iter()
            .filter_map(|&id| remap.get(id).copied().flatten())
            .collect();
        let nodes = ids
            .iter()
            .map(|&id| {
                let item = &self.nodes[id];
                ExportedSourceErrorNode {
                    message: item.error.to_string(),
                    type_name: item
                        .display_type_name(hide_report_wrapper_types)
                        .map(ToOwned::to_owned),
                    source_roots: item
                        .source_roots
                        .iter()
                        .filter_map(|&child| remap.get(child).copied().flatten())
                        .collect(),
                }
            })
            .collect();

        ExportedSourceErrorChain {
            roots,
            nodes,
            truncated: self.truncated,
            cycle_detected: self.cycle_detected,
        }
    }

    fn build_chain_root(
        root: SourceErrorItem,
        source: Option<&SourceErrorChain>,
        state: CauseTraversalState,
    ) -> Self {
        let mut nodes = vec![root];
        let mut truncated = state.truncated;
        let mut cycle_detected = state.cycle_detected;

        if let Some(source) = source {
            let offset = nodes.len();
            nodes[0].source_roots = source.roots.iter().map(|id| id + offset).collect();
            nodes.extend(source.nodes.iter().cloned().map(|n| shift_item(n, offset)));
            truncated |= source.truncated;
            cycle_detected |= source.cycle_detected;
        }

        Self {
            nodes: nodes.into(),
            roots: vec![0].into(),
            truncated,
            cycle_detected,
        }
    }

    pub(crate) fn from_source(error: &dyn Error, options: CauseCollectOptions) -> Self {
        Self::from_borrowed_error(error, options)
    }

    pub(crate) fn from_root_source<T>(
        error: T,
        source: Option<SourceErrorChain>,
        state: CauseTraversalState,
    ) -> Self
    where
        T: Error + Send + Sync + 'static,
    {
        Self::build_chain_root(SourceErrorItem::new(error), source.as_ref(), state)
    }

    pub(crate) fn from_error<T>(error: T) -> Self
    where
        T: Error + Send + Sync + 'static,
    {
        let (source, state) = Self::from_borrowed_srcs(
            error.source(),
            CauseCollectOptions {
                max_depth: usize::MAX,
                detect_cycle: true,
            },
        );
        Self::build_chain_root(SourceErrorItem::new(error), source.as_deref(), state)
    }

    /// Returns true when the chain has no roots.
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// Returns a flattened iterator over chain entries.
    pub fn iter_entries(&self) -> SourceErrorChainEntries<'_> {
        SourceErrorChainEntries::new(self, false)
    }

    pub(crate) fn iter_entries_origin(&self) -> SourceErrorChainEntries<'_> {
        SourceErrorChainEntries::new(self, true)
    }

    /// Returns an iterator over root-level source error items.
    pub fn iter(&self) -> SourceErrorItemIter<'_> {
        SourceErrorItemIter {
            chain: self,
            index: 0,
        }
    }

    pub(crate) fn first_error(&self) -> Option<&(dyn Error + 'static)> {
        let id = *self.roots.first()?;
        self.nodes
            .get(id)
            .map(|item| item.error.as_ref() as &(dyn Error + 'static))
    }

    pub(crate) fn roots_slice(&self) -> &[SourceNodeId] {
        &self.roots
    }

    pub(crate) fn node(&self, id: SourceNodeId) -> Option<&SourceErrorItem> {
        self.nodes.get(id)
    }

    pub(crate) fn state(&self) -> CauseTraversalState {
        CauseTraversalState {
            truncated: self.truncated,
            cycle_detected: self.cycle_detected,
        }
    }

    pub(crate) fn clear_cycle_flags(&mut self) {
        self.cycle_detected = false;
    }

    pub(super) fn from_borrowed_error(error: &dyn Error, options: CauseCollectOptions) -> Self {
        let (source, state) = Self::from_borrowed_srcs(error.source(), options);
        let root = SourceErrorItem {
            error: Arc::new(StringError(error.to_string())),
            type_name: None,
            source_roots: Vec::new(),
        };
        Self::build_chain_root(root, source.as_deref(), state)
    }

    pub(super) fn from_borrowed_srcs(
        next: Option<&dyn Error>,
        options: CauseCollectOptions,
    ) -> (Option<Arc<SourceErrorChain>>, CauseTraversalState) {
        let (items, state) = collect_borrowed_items::<SourceErrorItem>(next, options);
        (chain_from_linear::<SourceErrorChain>(items, &state), state)
    }
}

#[cfg(feature = "json")]
#[derive(serde::Serialize)]
struct DisplayCauseChainSerdeHelper {
    items: Vec<String>,
    truncated: bool,
    cycle_detected: bool,
}

#[cfg(feature = "json")]
impl serde::Serialize for DisplayCauseChain {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        DisplayCauseChainSerdeHelper {
            items: self.items.iter().map(ToString::to_string).collect(),
            truncated: self.truncated,
            cycle_detected: self.cycle_detected,
        }
        .serialize(serializer)
    }
}

#[cfg(feature = "json")]
#[derive(serde::Serialize)]
struct SourceArenaNodeSerde {
    message: String,
    #[serde(rename = "type")]
    type_name: Option<String>,
    source_roots: Vec<usize>,
}

#[cfg(feature = "json")]
#[derive(serde::Serialize)]
struct SourceArenaSerdeHelper {
    roots: Vec<usize>,
    nodes: Vec<SourceArenaNodeSerde>,
    truncated: bool,
    cycle_detected: bool,
}

#[cfg(feature = "json")]
impl serde::Serialize for SourceErrorChain {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let exported = self.export_with_options(false);
        SourceArenaSerdeHelper {
            roots: exported.roots,
            nodes: exported
                .nodes
                .into_iter()
                .map(|n| SourceArenaNodeSerde {
                    message: n.message,
                    type_name: n.type_name,
                    source_roots: n.source_roots,
                })
                .collect(),
            truncated: exported.truncated,
            cycle_detected: exported.cycle_detected,
        }
        .serialize(serializer)
    }
}
