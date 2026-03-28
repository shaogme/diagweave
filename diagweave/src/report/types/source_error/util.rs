use super::*;

pub(super) fn error_addr(error: &dyn Error) -> usize {
    let ptr = (error as *const dyn Error) as *const ();
    ptr as usize
}

pub(super) fn walk_source_chains<F>(chain: &SourceErrorChain, depth: usize, visit: &mut F)
where
    F: FnMut(&SourceErrorChain, usize),
{
    let mut stack = vec![(chain, depth)];
    while let Some((current, current_depth)) = stack.pop() {
        visit(current, current_depth);
        for item in current.items.iter().rev() {
            if let Some(source) = item.source.as_ref() {
                stack.push((source.as_ref(), current_depth + 1));
            }
        }
    }
}

pub(super) fn is_report_wrapper_type(type_name: &str) -> bool {
    let report_prefix = core::any::type_name::<crate::report::Report<()>>();
    let report_prefix = report_prefix
        .split_once('<')
        .map(|(prefix, _)| prefix)
        .unwrap_or(report_prefix);
    type_name.starts_with(report_prefix) && type_name[report_prefix.len()..].starts_with('<')
}

fn collect_borrowed_items(
    next: Option<&dyn Error>,
    options: CauseCollectOptions,
) -> (Vec<SourceErrorItem>, CauseTraversalState) {
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

        let addr = error_addr(current);
        if options.detect_cycle && seen.contains(addr) {
            state.cycle_detected = true;
            break;
        }
        if options.detect_cycle {
            let _ = seen.insert(addr);
        }

        items.push(SourceErrorItem {
            error: Arc::new(StringError(current.to_string())),
            type_name: None,
            source: None,
        });

        let Some(next) = current.source() else {
            break;
        };
        current = next;
        depth += 1;
    }

    (items, state)
}

fn chain_from_reversed_items(
    items: Vec<SourceErrorItem>,
    state: &CauseTraversalState,
) -> Option<Arc<SourceErrorChain>> {
    if items.is_empty() {
        return Some(Arc::new(SourceErrorChain {
            items: Vec::new().into(),
            truncated: state.truncated,
            cycle_detected: state.cycle_detected,
        }));
    }

    let mut chain: Option<Arc<SourceErrorChain>> = None;
    for mut item in items.into_iter().rev() {
        item.source = chain;
        chain = Some(Arc::new(SourceErrorChain {
            items: vec![item].into(),
            truncated: false,
            cycle_detected: false,
        }));
    }
    if let Some(chain) = chain.as_mut() {
        let chain = Arc::make_mut(chain);
        chain.truncated = state.truncated;
        chain.cycle_detected = state.cycle_detected;
    }
    chain
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

fn source_error_eq_items(
    items: &[SourceErrorItem],
) -> Vec<(String, Option<String>, Option<&SourceErrorChain>)> {
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

impl Clone for SourceErrorChain {
    fn clone(&self) -> Self {
        Self {
            items: self.items.clone(),
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct StringError(String);

impl Display for StringError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for StringError {}

impl SourceErrorChain {
    pub(crate) fn from_error<T>(error: T) -> Self
    where
        T: Error + Send + Sync + 'static,
    {
        let (item, cycle_detected) = SourceErrorItem::from_error(
            error,
            CauseCollectOptions {
                max_depth: usize::MAX,
                detect_cycle: true,
            },
        );
        Self {
            items: vec![item].into(),
            truncated: false,
            cycle_detected,
        }
    }

    pub(crate) fn from_source(error: &dyn Error, options: CauseCollectOptions) -> Self {
        Self::from_borrowed_error(error, options)
    }

    pub(crate) fn append(&mut self, other: SourceErrorChain) {
        let state = other.state();
        self.truncated |= state.truncated;
        self.cycle_detected |= state.cycle_detected;
        if self.items.is_empty() {
            self.items = other.items;
            return;
        }
        if other.items.is_empty() {
            return;
        }
        let mut items = Vec::with_capacity(self.items.len() + other.items.len());
        items.extend(self.items.iter().cloned());
        items.extend(other.items.iter().cloned());
        self.items = items.into();
    }

    /// Returns `true` when the chain has no items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Returns an iterator over flattened source-error entries.
    pub fn iter_entries(&self) -> SourceErrorChainEntries<'_> {
        SourceErrorChainEntries::new(self)
    }

    /// Returns a direct iterator over top-level source-error items.
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
        let items = self
            .items
            .iter()
            .map(|item| {
                let mut item = item.clone();
                if let Some(source) = item.source.as_ref() {
                    let mut source = (**source).clone();
                    source.clear_cycle_flags();
                    item.source = Some(Arc::new(source));
                }
                item
            })
            .collect::<Vec<_>>();
        self.items = items.into();
    }

    pub(crate) fn limit_depth(&mut self, options: CauseCollectOptions, depth: usize) -> bool {
        let mut truncated = self.truncated;
        if depth >= options.max_depth {
            self.items = Vec::new().into();
            self.truncated = true;
            return true;
        }

        let items = self
            .items
            .iter()
            .map(|item| {
                let mut item = item.clone();
                if let Some(source) = item.source.as_ref() {
                    let mut source = (**source).clone();
                    truncated |= source.limit_depth(options, depth + 1);
                    item.source = Some(Arc::new(source));
                }
                item
            })
            .collect::<Vec<_>>();
        self.items = items.into();
        self.truncated = truncated;
        truncated
    }

    pub(super) fn from_borrowed_error(error: &dyn Error, options: CauseCollectOptions) -> Self {
        let (source, state) = Self::from_borrowed_srcs(error.source(), options);
        Self {
            items: vec![SourceErrorItem {
                error: Arc::new(StringError(error.to_string())),
                type_name: None,
                source,
            }]
            .into(),
            truncated: state.truncated,
            cycle_detected: state.cycle_detected,
        }
    }

    pub(super) fn from_borrowed_srcs(
        next: Option<&dyn Error>,
        options: CauseCollectOptions,
    ) -> (Option<Arc<SourceErrorChain>>, CauseTraversalState) {
        let (items, state) = collect_borrowed_items(next, options);
        (chain_from_reversed_items(items, &state), state)
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
