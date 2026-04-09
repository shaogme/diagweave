#[path = "source_error/traversal.rs"]
mod traversal;
#[path = "source_error/util.rs"]
mod util;
use super::*;
use crate::utils::FastSet;
pub use traversal::{ReportSourceErrorIter, SourceErrorChainEntries};
use util::is_report_wrapper_type;
pub(crate) use util::{append_source_chain, limit_depth_source_chain};

pub(crate) type SourceNodeId = usize;

/// Iterator over source errors with depth/cycle control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceErrorEntry {
    pub message: String,
    pub type_name: Option<String>,
    pub display_type_name: Option<String>,
    pub depth: usize,
}

#[derive(Debug, Default)]
pub(crate) struct DiagnosticBag {
    #[cfg(feature = "trace")]
    pub(crate) trace: Option<ReportTrace>,
    pub(crate) stack_trace: Option<StackTrace>,
    pub(crate) context: ContextMap,
    pub(crate) system: SystemContext,
    pub(crate) attachments: Vec<Attachment>,
    pub(crate) display_causes: Option<DisplayCauseChain>,
    pub(crate) origin_source_errors: Option<SourceErrorChain>,
    pub(crate) diagnostic_source_errors: Option<SourceErrorChain>,
}

impl Clone for DiagnosticBag {
    fn clone(&self) -> Self {
        Self {
            #[cfg(feature = "trace")]
            trace: self.trace.clone(),
            stack_trace: self.stack_trace.clone(),
            context: self.context.clone(),
            system: self.system.clone(),
            attachments: self.attachments.clone(),
            display_causes: self.display_causes.clone(),
            origin_source_errors: self.origin_source_errors.clone(),
            diagnostic_source_errors: self.diagnostic_source_errors.clone(),
        }
    }
}

impl PartialEq for DiagnosticBag {
    fn eq(&self, other: &Self) -> bool {
        #[cfg(feature = "trace")]
        if self.trace != other.trace {
            return false;
        }
        self.stack_trace == other.stack_trace
            && self.context == other.context
            && self.system == other.system
            && self.attachments == other.attachments
            && self.display_causes == other.display_causes
            && self.origin_source_errors == other.origin_source_errors
            && self.diagnostic_source_errors == other.diagnostic_source_errors
    }
}

/// Cold data storage for Report - contains metadata, diagnostic bag, and options.
/// This struct is used to reduce Report's size by combining
/// metadata, DiagnosticBag, and ReportOptions into a single boxed structure.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ColdData {
    pub(crate) metadata: ReportMetadata,
    pub(crate) bag: DiagnosticBag,
    pub(crate) options: ReportOptions,
}

impl ColdData {
    /// Creates a new ColdData with the given metadata, options, and empty diagnostic bag.
    pub(crate) fn new(metadata: ReportMetadata, options: ReportOptions) -> Self {
        Self {
            metadata,
            bag: DiagnosticBag::default(),
            options,
        }
    }
}

impl Default for ColdData {
    fn default() -> Self {
        Self {
            metadata: ReportMetadata::default(),
            bag: DiagnosticBag::default(),
            options: ReportOptions::default_for_profile(),
        }
    }
}

/// Global context information that can be injected into reports.
#[derive(Debug, Clone, Default)]
pub struct GlobalContext {
    #[cfg(feature = "trace")]
    pub trace: Option<GlobalTraceContext>,
    pub error: Option<GlobalErrorMeta>,
    pub system: SystemContext,
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
