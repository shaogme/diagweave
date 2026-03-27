#[path = "types/attachment.rs"]
pub mod attachment;
#[path = "types/error.rs"]
pub mod error;

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
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

/// The current stage of source error iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceErrorIterStage {
    /// Iterating through explicitly attached source errors.
    Attached,
    /// Iterating through the natural `source()` chain of the inner error.
    Inner,
    /// Completed iteration.
    Done,
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
pub struct ReportSourceErrorIter<'a> {
    pub(crate) source_errors: core::slice::Iter<'a, Box<dyn Error + 'static>>,
    pub(crate) root_source: Option<&'a (dyn Error + 'static)>,
    pub(crate) current: Option<&'a (dyn Error + 'static)>,
    pub(crate) stage: SourceErrorIterStage,
    pub(crate) depth: usize,
    pub(crate) options: CauseCollectOptions,
    pub(crate) seen: SeenErrorAddrs,
    pub(crate) state: CauseTraversalState,
}

impl<'a> ReportSourceErrorIter<'a> {
    /// Returns traversal state observed so far.
    pub fn state(&self) -> CauseTraversalState {
        self.state
    }
}

impl<'a> Iterator for ReportSourceErrorIter<'a> {
    type Item = &'a (dyn Error + 'static);

    fn next(&mut self) -> Option<Self::Item> {
        if self.state.truncated || self.state.cycle_detected {
            self.stage = SourceErrorIterStage::Done;
            return None;
        }

        loop {
            let err = match self.current.take() {
                Some(err) => err,
                None => match self.stage {
                    SourceErrorIterStage::Attached => {
                        if let Some(err) = self.source_errors.next() {
                            err.as_ref()
                        } else {
                            self.stage = SourceErrorIterStage::Inner;
                            continue;
                        }
                    }
                    SourceErrorIterStage::Inner => {
                        let Some(err) = self.root_source.take() else {
                            self.stage = SourceErrorIterStage::Done;
                            return None;
                        };
                        err
                    }
                    SourceErrorIterStage::Done => return None,
                },
            };

            if self.depth >= self.options.max_depth {
                self.state.truncated = true;
                self.stage = SourceErrorIterStage::Done;
                return None;
            }
            if self.options.detect_cycle {
                let ptr = (err as *const dyn Error) as *const ();
                let addr = ptr as usize;
                if !self.seen.insert(addr) {
                    self.state.cycle_detected = true;
                    self.stage = SourceErrorIterStage::Done;
                    return None;
                }
            }
            self.current = err.source();
            self.depth += 1;
            return Some(err);
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
#[derive(Default)]
pub struct SourceErrorChain {
    pub items: Vec<Box<dyn Error + 'static>>,
    pub truncated: bool,
    pub cycle_detected: bool,
}

impl Clone for SourceErrorChain {
    fn clone(&self) -> Self {
        Self {
            items: self
                .items
                .iter()
                .map(|item| Box::new(StringError(item.to_string())) as Box<dyn Error + 'static>)
                .collect(),
            truncated: self.truncated,
            cycle_detected: self.cycle_detected,
        }
    }
}

impl core::fmt::Debug for SourceErrorChain {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let items: Vec<String> = self.items.iter().map(ToString::to_string).collect();
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

impl Eq for SourceErrorChain {}

#[cfg(feature = "json")]
impl serde::Serialize for SourceErrorChain {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(serde::Serialize)]
        struct Item {
            message: String,
        }
        #[derive(serde::Serialize)]
        struct Helper {
            items: Vec<Item>,
            truncated: bool,
            cycle_detected: bool,
        }
        Helper {
            items: self
                .items
                .iter()
                .map(|v| Item {
                    message: v.to_string(),
                })
                .collect(),
            truncated: self.truncated,
            cycle_detected: self.cycle_detected,
        }
        .serialize(serializer)
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
