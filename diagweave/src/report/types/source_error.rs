#[path = "source_error/traversal.rs"]
mod traversal;
#[path = "source_error/util.rs"]
mod util;
use super::*;
use crate::utils::FastSet;
pub use traversal::{ReportSourceErrorIter, SourceErrorChainEntries};
use util::is_report_wrapper_type;
pub(crate) use util::{append_source_chain, build_origin_source_chain, limit_depth_source_chain};

pub(crate) type SourceNodeId = usize;

/// Iterator over source errors with depth/cycle control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceErrorEntry {
    pub message: String,
    pub type_name: Option<String>,
    pub display_type_name: Option<String>,
    pub depth: usize,
}

/// Inner diagnostic bag containing all diagnostic data.
/// This is the actual storage for diagnostic information.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct DiagnosticBagInner {
    stack_trace: Option<StackTrace>,
    context: ContextMap,
    system: ContextMap,
    attachments: Vec<Attachment>,
    display_causes: Option<DisplayCauseChain>,
    origin_source_errors: Option<SourceErrorChain>,
    diagnostic_source_errors: Option<SourceErrorChain>,
}

impl DiagnosticBagInner {
    fn new() -> Self {
        Self::default()
    }
}

/// A lazily-allocated diagnostic bag that wraps an optional `DiagnosticBagInner`.
///
/// This design follows the same pattern as `ReportMetadata` - using `Option<Box<Inner>>`
/// for lazy allocation to minimize overhead when no diagnostic data is present.
///
/// # Example
///
/// ```rust
/// use diagweave::prelude::Report;
/// use diagweave::Error;
///
/// #[derive(Debug, Error)]
/// #[display("my error")]
/// struct MyError;
///
/// // DiagnosticBag starts as None, no allocation
/// let report = Report::new(MyError);
///
/// // Only allocates when you add diagnostic data
/// let report = report.attach_note("Additional context");
/// ```
#[derive(Debug, Default, PartialEq)]
pub struct DiagnosticBag {
    inner: Option<alloc::boxed::Box<DiagnosticBagInner>>,
}

impl DiagnosticBag {
    /// Creates a new empty `DiagnosticBag` with no allocation.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a reference to the inner diagnostic data, if allocated.
    pub(crate) fn inner(&self) -> Option<&DiagnosticBagInner> {
        self.inner.as_deref()
    }

    /// Ensures the inner storage is allocated, creating it if necessary.
    pub(crate) fn ensure_inner(&mut self) -> &mut DiagnosticBagInner {
        self.inner
            .get_or_insert_with(|| alloc::boxed::Box::new(DiagnosticBagInner::new()))
    }

    /// Returns the stack trace, if any.
    pub fn stack_trace(&self) -> Option<&StackTrace> {
        self.inner.as_ref()?.stack_trace.as_ref()
    }

    /// Returns the context map, or an empty reference if not allocated.
    pub fn context(&self) -> &ContextMap {
        self.inner
            .as_ref()
            .map(|i| &i.context)
            .unwrap_or(ContextMap::default_ref())
    }

    /// Returns the system context map, or an empty reference if not allocated.
    pub fn system(&self) -> &ContextMap {
        self.inner
            .as_ref()
            .map(|i| &i.system)
            .unwrap_or(ContextMap::default_ref())
    }

    /// Returns the attachments, or an empty slice if not allocated.
    pub fn attachments(&self) -> &[Attachment] {
        self.inner
            .as_ref()
            .map(|i| i.attachments.as_slice())
            .unwrap_or(&[])
    }

    /// Returns the display causes, if any.
    pub(crate) fn display_causes(&self) -> Option<&DisplayCauseChain> {
        self.inner.as_ref()?.display_causes.as_ref()
    }

    /// Returns the origin source errors, if any.
    pub(crate) fn origin_source_errors(&self) -> Option<&SourceErrorChain> {
        self.inner.as_ref()?.origin_source_errors.as_ref()
    }

    /// Returns the diagnostic source errors, if any.
    pub(crate) fn diagnostic_source_errors(&self) -> Option<&SourceErrorChain> {
        self.inner.as_ref()?.diagnostic_source_errors.as_ref()
    }

    /// Sets the stack trace.
    pub fn set_stack_trace(&mut self, stack_trace: StackTrace) {
        self.ensure_inner().stack_trace = Some(stack_trace);
    }

    /// Inserts a context key-value pair.
    pub fn insert_context(
        &mut self,
        key: impl Into<ref_str::StaticRefStr>,
        value: impl Into<ContextValue>,
    ) {
        self.ensure_inner().context.insert(key, value);
    }

    /// Inserts a system context key-value pair.
    pub fn insert_system(
        &mut self,
        key: impl Into<ref_str::StaticRefStr>,
        value: impl Into<ContextValue>,
    ) {
        self.ensure_inner().system.insert(key, value);
    }

    /// Adds an attachment.
    pub fn add_attachment(&mut self, attachment: Attachment) {
        self.ensure_inner().attachments.push(attachment);
    }

    /// Sets the display causes.
    pub(crate) fn set_display_causes(&mut self, causes: DisplayCauseChain) {
        self.ensure_inner().display_causes = Some(causes);
    }

    /// Sets the origin source errors.
    pub(crate) fn set_origin_source_errors(&mut self, errors: SourceErrorChain) {
        self.ensure_inner().origin_source_errors = Some(errors);
    }

    /// Sets the diagnostic source errors.
    pub(crate) fn set_diagnostic_source_errors(&mut self, errors: SourceErrorChain) {
        self.ensure_inner().diagnostic_source_errors = Some(errors);
    }

    /// Returns a mutable reference to the context map, allocating if necessary.
    pub(crate) fn context_mut(&mut self) -> &mut ContextMap {
        &mut self.ensure_inner().context
    }

    /// Returns a mutable reference to the system context map, allocating if necessary.
    pub(crate) fn system_mut(&mut self) -> &mut ContextMap {
        &mut self.ensure_inner().system
    }

    /// Returns a mutable reference to the stack trace, allocating if necessary.
    pub(crate) fn stack_trace_mut(&mut self) -> &mut Option<StackTrace> {
        &mut self.ensure_inner().stack_trace
    }

    /// Returns a mutable reference to the display causes, allocating if necessary.
    pub(crate) fn display_causes_mut(&mut self) -> &mut Option<DisplayCauseChain> {
        &mut self.ensure_inner().display_causes
    }

    /// Returns a mutable reference to the diagnostic source errors, allocating if necessary.
    pub(crate) fn diagnostic_source_errors_mut(&mut self) -> &mut Option<SourceErrorChain> {
        &mut self.ensure_inner().diagnostic_source_errors
    }

    /// Creates a DiagnosticBag from an existing inner bag.
    fn from_inner(inner: alloc::boxed::Box<DiagnosticBagInner>) -> Self {
        Self { inner: Some(inner) }
    }

    /// Creates a new bag with updated origin source errors, preserving all other data from this bag.
    /// If this bag is empty, creates a new bag with just the origin source errors.
    pub(crate) fn with_origin_source_chain(
        mut self,
        origin_source_errors: SourceErrorChain,
    ) -> Self {
        match self.inner.take() {
            Some(inner) => DiagnosticBag::from_inner(Box::new(DiagnosticBagInner {
                stack_trace: inner.stack_trace,
                context: inner.context,
                system: inner.system,
                attachments: inner.attachments,
                display_causes: inner.display_causes,
                origin_source_errors: Some(origin_source_errors),
                diagnostic_source_errors: inner.diagnostic_source_errors,
            })),
            None => {
                let mut new_bag = Self::new();
                new_bag.set_origin_source_errors(origin_source_errors);
                new_bag
            }
        }
    }
}

/// Global context information that can be injected into reports.
#[derive(Debug, Clone, Default)]
pub struct GlobalContext {
    #[cfg(feature = "trace")]
    pub trace: Option<GlobalTraceContext>,
    pub error: Option<GlobalErrorMeta>,
    pub system: ContextMap,
    pub context: ContextMap,
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
        let mut spill = FastSet::with_capacity(self.inline.len() * 2 + 1);
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
        let raw = error as *const dyn Error;
        // SAFETY: Splitting a `*const dyn Error` into data and vtable pointers preserves the
        // pointer bits; both pointers are only used for identity comparison, never dereferenced.
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

    pub(crate) fn display_type_name(&self, hide_report_wrapper_types: bool) -> Option<&str> {
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

/// Iterator over root-level source error items.
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
