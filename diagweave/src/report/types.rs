#[path = "types/attachment.rs"]
pub mod attachment;
#[path = "types/error.rs"]
pub mod error;
#[path = "types/source_error.rs"]
mod source_error;

use alloc::string::String;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::any;
use core::error::Error;
use core::fmt::{self, Display, Formatter};
use ref_str::StaticRefStr;

#[cfg(feature = "trace")]
use super::trace::{ParentSpanId, ReportTrace, SpanId, TraceId};

pub use attachment::*;
pub use error::*;
pub use source_error::*;

mod observability_state {
    pub trait Sealed {}
}

/// Typestate marker for reports whose observability level has not been set.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MissingObservability;

/// Typestate marker for reports whose observability level is present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HasObservability {
    level: ObservabilityLevel,
}

impl HasObservability {
    /// Creates a present observability typestate with the specified level.
    pub const fn new(level: ObservabilityLevel) -> Self {
        Self { level }
    }

    /// Returns the guaranteed observability level carried by this typestate.
    pub const fn level(self) -> ObservabilityLevel {
        self.level
    }
}

impl observability_state::Sealed for MissingObservability {}
impl observability_state::Sealed for HasObservability {}

/// Typestate contract for report observability metadata.
pub trait ObservabilityState: observability_state::Sealed + Clone + Copy + PartialEq + Eq {
    /// Returns the observability level represented by the typestate, if any.
    fn observability_level(self) -> Option<ObservabilityLevel>;
}

impl ObservabilityState for MissingObservability {
    fn observability_level(self) -> Option<ObservabilityLevel> {
        None
    }
}

impl ObservabilityState for HasObservability {
    fn observability_level(self) -> Option<ObservabilityLevel> {
        Some(self.level)
    }
}

#[cfg(feature = "json")]
impl serde::Serialize for MissingObservability {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_none()
    }
}

#[cfg(feature = "json")]
impl<'de> serde::Deserialize<'de> for MissingObservability {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        match Option::<ObservabilityLevel>::deserialize(deserializer)? {
            None => Ok(Self),
            Some(_) => Err(serde::de::Error::custom(
                "expected null observability typestate",
            )),
        }
    }
}

#[cfg(feature = "json")]
impl serde::Serialize for HasObservability {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.level.serialize(serializer)
    }
}

#[cfg(feature = "json")]
impl<'de> serde::Deserialize<'de> for HasObservability {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        ObservabilityLevel::deserialize(deserializer).map(Self::new)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
/// Report metadata carried alongside a diagnostic.
///
/// When the `json` feature is enabled, deserialization is intentionally exposed
/// only for [`ReportMetadata<HasObservability>`]. The default
/// [`ReportMetadata`] alias keeps the stricter missing-observability typestate
/// and does not implement `Deserialize`.
pub struct ReportMetadata<State = MissingObservability> {
    error_code: Option<ErrorCode>,
    severity: Option<Severity>,
    observability_level: State,
    category: Option<StaticRefStr>,
    retryable: Option<bool>,
}

impl Default for ReportMetadata<MissingObservability> {
    fn default() -> Self {
        Self {
            error_code: None,
            severity: None,
            observability_level: MissingObservability,
            category: None,
            retryable: None,
        }
    }
}

impl<State> ReportMetadata<State>
where
    State: ObservabilityState,
{
    /// Returns the error code, if present.
    pub fn error_code(&self) -> Option<&ErrorCode> {
        self.error_code.as_ref()
    }

    /// Returns the severity, if present.
    pub fn severity(&self) -> Option<Severity> {
        self.severity
    }

    /// Returns the observability level, if present.
    pub fn observability_level(&self) -> Option<ObservabilityLevel> {
        self.observability_level.observability_level()
    }

    /// Returns whether the metadata carries an observability level.
    pub fn has_observability_level(&self) -> bool {
        self.observability_level().is_some()
    }

    /// Returns the category, if present.
    pub fn category(&self) -> Option<&str> {
        self.category.as_deref()
    }

    /// Returns whether the metadata marks the diagnostic as retryable, if present.
    pub fn retryable(&self) -> Option<bool> {
        self.retryable
    }

    pub(crate) fn observability_state(&self) -> State {
        self.observability_level
    }

    pub(crate) fn map_observability<NewState>(
        self,
        observability_level: NewState,
    ) -> ReportMetadata<NewState>
    where
        NewState: ObservabilityState,
    {
        ReportMetadata {
            error_code: self.error_code,
            severity: self.severity,
            observability_level,
            category: self.category,
            retryable: self.retryable,
        }
    }

    /// Returns metadata with the specified error code.
    pub fn with_error_code(mut self, error_code: impl Into<ErrorCode>) -> Self {
        self.error_code = Some(error_code.into());
        self
    }

    /// Returns metadata with the specified severity.
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = Some(severity);
        self
    }

    /// Replaces the metadata typestate with a concrete observability level.
    pub fn with_observability_level(
        self,
        level: ObservabilityLevel,
    ) -> ReportMetadata<HasObservability> {
        self.map_observability(HasObservability::new(level))
    }

    /// Returns metadata with the specified category.
    pub fn with_category(mut self, category: impl Into<StaticRefStr>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Returns metadata with the specified retryability flag.
    pub fn with_retryable(mut self, retryable: bool) -> Self {
        self.retryable = Some(retryable);
        self
    }
}

impl ReportMetadata<HasObservability> {
    /// Returns the guaranteed observability level.
    pub const fn required_observability_level(&self) -> ObservabilityLevel {
        self.observability_level.level()
    }
}

#[cfg(feature = "json")]
impl<'de> serde::Deserialize<'de> for ReportMetadata<HasObservability> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct ReportMetadataWire {
            error_code: Option<ErrorCode>,
            severity: Option<Severity>,
            observability_level: ObservabilityLevel,
            category: Option<StaticRefStr>,
            retryable: Option<bool>,
        }

        let wire = ReportMetadataWire::deserialize(deserializer)?;
        Ok(ReportMetadata {
            error_code: wire.error_code,
            severity: wire.severity,
            observability_level: HasObservability::new(wire.observability_level),
            category: wire.category,
            retryable: wire.retryable,
        })
    }
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
    pub symbol: Option<StaticRefStr>,
    pub module_path: Option<StaticRefStr>,
    pub file: Option<StaticRefStr>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct StackTrace {
    pub format: StackTraceFormat,
    pub frames: Arc<[StackFrame]>,
    pub raw: Option<StaticRefStr>,
}

impl Default for StackTrace {
    fn default() -> Self {
        Self {
            format: StackTraceFormat::Native,
            frames: Vec::new().into(),
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
        self.frames = frames.into();
        self
    }

    /// Sets the raw stack trace string.
    pub fn with_raw(mut self, raw: impl Into<StaticRefStr>) -> Self {
        self.raw = Some(raw.into());
        self
    }

    /// Captures the current stack trace as a raw string (requires `std` feature).
    #[cfg(feature = "std")]
    pub fn capture_raw() -> Self {
        let backtrace = std::backtrace::Backtrace::force_capture();
        Self {
            format: StackTraceFormat::Raw,
            frames: Vec::new().into(),
            raw: Some(backtrace.to_string().into()),
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

impl CauseTraversalState {
    /// Merges traversal flags from another state.
    pub fn merge_from(&mut self, other: Self) {
        self.truncated |= other.truncated;
        self.cycle_detected |= other.cycle_detected;
    }
}

/// A streamed attachment item for visitor-based traversal.
pub enum AttachmentVisit<'a> {
    Context {
        key: &'a StaticRefStr,
        value: &'a AttachmentValue,
    },
    Note {
        message: &'a (dyn Display + Send + Sync + 'static),
    },
    Payload {
        name: &'a StaticRefStr,
        value: &'a AttachmentValue,
        media_type: Option<&'a StaticRefStr>,
    },
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
