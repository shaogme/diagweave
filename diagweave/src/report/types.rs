#[path = "types/attachment.rs"]
pub mod attachment;
#[path = "types/context.rs"]
pub mod context;
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
use super::trace::ReportTrace;

pub use attachment::*;
pub use context::*;
pub use error::*;
pub use source_error::*;

mod severity_state {
    pub trait Sealed {}
}

/// Typestate marker for reports whose severity has not been set.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MissingSeverity;

/// Typestate marker for reports whose severity is present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HasSeverity {
    severity: Severity,
}

impl HasSeverity {
    /// Creates a present severity typestate with the specified severity.
    pub const fn new(severity: Severity) -> Self {
        Self { severity }
    }

    /// Returns the guaranteed severity carried by this typestate.
    pub const fn severity(self) -> Severity {
        self.severity
    }
}

impl severity_state::Sealed for MissingSeverity {}
impl severity_state::Sealed for HasSeverity {}

/// Typestate contract for report severity metadata.
pub trait SeverityState: severity_state::Sealed + Clone + Copy + PartialEq + Eq {
    /// Returns the severity represented by the typestate, if any.
    fn severity(self) -> Option<Severity>;
}

impl SeverityState for MissingSeverity {
    fn severity(self) -> Option<Severity> {
        None
    }
}

impl SeverityState for HasSeverity {
    fn severity(self) -> Option<Severity> {
        Some(self.severity)
    }
}

#[cfg(feature = "json")]
impl serde::Serialize for MissingSeverity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_none()
    }
}

#[cfg(feature = "json")]
impl<'de> serde::Deserialize<'de> for MissingSeverity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        match Option::<Severity>::deserialize(deserializer)? {
            None => Ok(Self),
            Some(_) => Err(serde::de::Error::custom("expected null severity typestate")),
        }
    }
}

#[cfg(feature = "json")]
impl serde::Serialize for HasSeverity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.severity.serialize(serializer)
    }
}

#[cfg(feature = "json")]
impl<'de> serde::Deserialize<'de> for HasSeverity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Severity::deserialize(deserializer).map(Self::new)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
/// Report metadata carried alongside a diagnostic.
///
/// Contains optional error code, category, and retryable flag.
/// Severity is stored separately in the Report typestate.
pub struct ReportMetadata {
    error_code: Option<ErrorCode>,
    category: Option<StaticRefStr>,
    retryable: Option<bool>,
}

impl Default for ReportMetadata {
    fn default() -> Self {
        Self {
            error_code: None,
            category: None,
            retryable: None,
        }
    }
}

impl ReportMetadata {
    /// Returns a static reference to a default ReportMetadata.
    /// This is useful for cases where no cold data exists.
    pub(crate) fn default_ref() -> &'static Self {
        static DEFAULT: ReportMetadata = ReportMetadata {
            error_code: None,
            category: None,
            retryable: None,
        };
        &DEFAULT
    }

    /// Returns the error code, if present.
    pub fn error_code(&self) -> Option<&ErrorCode> {
        self.error_code.as_ref()
    }

    /// Returns the category, if present.
    pub fn category(&self) -> Option<&str> {
        self.category.as_deref()
    }

    /// Returns whether the metadata marks the diagnostic as retryable, if present.
    pub fn retryable(&self) -> Option<bool> {
        self.retryable
    }

    /// Sets the error code, replacing any existing value.
    pub fn set_error_code(mut self, error_code: impl Into<ErrorCode>) -> Self {
        self.error_code = Some(error_code.into());
        self
    }

    /// Sets the error code only if not already set.
    pub fn with_error_code(mut self, error_code: impl Into<ErrorCode>) -> Self {
        if self.error_code.is_none() {
            self.error_code = Some(error_code.into());
        }
        self
    }

    /// Sets the category, replacing any existing value.
    pub fn set_category(mut self, category: impl Into<StaticRefStr>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Sets the category only if not already set.
    pub fn with_category(mut self, category: impl Into<StaticRefStr>) -> Self {
        if self.category.is_none() {
            self.category = Some(category.into());
        }
        self
    }

    /// Sets the retryability flag, replacing any existing value.
    pub fn set_retryable(mut self, retryable: bool) -> Self {
        self.retryable = Some(retryable);
        self
    }

    /// Sets the retryability flag only if not already set.
    pub fn with_retryable(mut self, retryable: bool) -> Self {
        if self.retryable.is_none() {
            self.retryable = Some(retryable);
        }
        self
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
    Note {
        message: &'a (dyn Display + Send + Sync + 'static),
    },
    Payload {
        name: &'a StaticRefStr,
        value: &'a AttachmentValue,
        media_type: Option<&'a StaticRefStr>,
    },
}

/// Per-report configuration for error source chain accumulation and cause collection behavior.
///
/// This controls whether [`Report::map_err()`] automatically accumulates the source error chain
/// when transforming error types, as well as options for cause collection depth and cycle detection.
///
/// # Default Behavior
///
/// All default values depend on the build profile to provide better debugging experience
/// during development while avoiding performance overhead in production:
///
/// | Option | Debug Build | Release Build |
/// |--------|-------------|---------------|
/// | `accumulate_source_chain` | `true` | `false` |
/// | `detect_cycle` | `true` | `false` |
/// | `max_depth` | `16` | `16` |
///
/// - **Debug builds** (`debug_assertions` enabled): Full diagnostics with cycle detection
/// - **Release builds** (`debug_assertions` disabled): Optimized for performance
///
/// # Example
///
/// ```ignore
/// use diagweave::{Report, ReportOptions};
///
/// // Create a report with default options (profile-dependent)
/// let report = Report::new(my_error);
///
/// // Explicitly enable source chain accumulation
/// let report = report.set_accumulate_source_chain(true);
///
/// // Configure cause collection depth
/// let report = report.set_options(ReportOptions::default().with_max_depth(32));
///
/// // Disable cycle detection for performance-critical paths
/// let report = report.set_options(ReportOptions::default().with_cycle_detection(false));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportOptions {
    /// Whether `map_err()` should automatically accumulate source error chains.
    ///
    /// When `true`, calling `map_err()` will preserve and extend the origin
    /// source error chain, similar to the old `boundary()` behavior.
    ///
    /// When `false`, `map_err()` only transforms the error type without
    /// accumulating source chains.
    ///
    /// **Default**: `true` in debug builds, `false` in release builds.
    pub accumulate_source_chain: bool,

    /// Maximum depth of causes to collect during source error traversal.
    ///
    /// This limits how deep the error chain will be traversed when collecting
    /// source errors. A higher value provides more complete error context but
    /// may impact performance for very deep error chains.
    ///
    /// **Default**: `16` (same for all build profiles).
    pub max_depth: usize,

    /// Whether to detect cycles in the cause chain during traversal.
    ///
    /// When `true`, the error chain traversal will track visited errors and
    /// mark cycles when detected. This is useful for debugging but adds
    /// overhead from maintaining a visited set.
    ///
    /// When `false`, cycle detection is skipped for better performance.
    /// Use this in release builds when error chains are trusted to be acyclic.
    ///
    /// **Default**: `true` in debug builds, `false` in release builds.
    pub detect_cycle: bool,
}

impl ReportOptions {
    /// Creates new report options with the specified source chain accumulation setting.
    ///
    /// Uses profile-dependent defaults for `detect_cycle` and fixed default for `max_depth`.
    pub const fn new(accumulate_source_chain: bool) -> Self {
        Self {
            accumulate_source_chain,
            max_depth: 16,
            // Note: detect_cycle uses runtime default, not const-evaluated profile
            detect_cycle: true,
        }
    }

    /// Returns the default report options based on build profile.
    ///
    /// # Profile-Dependent Defaults
    ///
    /// | Option | Debug Build | Release Build |
    /// |--------|-------------|---------------|
    /// | `accumulate_source_chain` | `true` | `false` |
    /// | `detect_cycle` | `true` | `false` |
    /// | `max_depth` | `16` | `16` |
    ///
    /// Debug builds enable full diagnostics for better development experience.
    /// Release builds optimize for performance by disabling cycle detection.
    pub const fn default_for_profile() -> Self {
        Self {
            // In debug builds, enable source chain accumulation for better debugging
            // In release builds, disable for better performance
            #[cfg(debug_assertions)]
            accumulate_source_chain: true,
            #[cfg(not(debug_assertions))]
            accumulate_source_chain: false,
            max_depth: 16,
            // In debug builds, enable cycle detection for safety
            // In release builds, disable for performance
            #[cfg(debug_assertions)]
            detect_cycle: true,
            #[cfg(not(debug_assertions))]
            detect_cycle: false,
        }
    }

    /// Sets whether source chains should be accumulated during `map_err()`.
    pub const fn with_accumulate_source_chain(mut self, accumulate: bool) -> Self {
        self.accumulate_source_chain = accumulate;
        self
    }

    /// Sets the maximum depth for cause collection.
    pub const fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Enables or disables cycle detection during cause collection.
    pub const fn with_cycle_detection(mut self, detect_cycle: bool) -> Self {
        self.detect_cycle = detect_cycle;
        self
    }

    /// Returns a CauseCollectOptions view of this struct for internal use.
    pub(crate) fn as_cause_options(&self) -> CauseCollectOptions {
        CauseCollectOptions {
            max_depth: self.max_depth,
            detect_cycle: self.detect_cycle,
        }
    }
}

impl Default for ReportOptions {
    fn default() -> Self {
        Self::default_for_profile()
    }
}

impl ReportOptions {
    /// Returns a static reference to the default ReportOptions.
    /// This is useful for cases where no cold data exists.
    pub(crate) fn default_ref() -> &'static Self {
        static DEFAULT: ReportOptions = ReportOptions {
            accumulate_source_chain: false,
            max_depth: 16,
            detect_cycle: false,
        };
        &DEFAULT
    }
}

/// Options for collecting cause messages from an error report.
///
/// This is a lightweight view into ReportOptions for internal use.
/// It is used by the traversal iterators and internal chain-building functions.
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
            detect_cycle: cfg!(debug_assertions),
        }
    }
}
