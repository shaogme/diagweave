#[path = "types/attachment.rs"]
pub mod attachment;
#[path = "types/config.rs"]
mod config;
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
pub use config::*;
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

    /// Sets the error code, replacing any existing value (mutable reference version).
    ///
    /// This method avoids cloning the entire metadata when modifying in place.
    pub fn set_error_code_mut(&mut self, error_code: impl Into<ErrorCode>) {
        self.error_code = Some(error_code.into());
    }

    /// Sets the error code only if not already set.
    pub fn with_error_code(mut self, error_code: impl Into<ErrorCode>) -> Self {
        if self.error_code.is_none() {
            self.error_code = Some(error_code.into());
        }
        self
    }

    /// Sets the error code only if not already set (mutable reference version).
    ///
    /// This method avoids cloning the entire metadata when modifying in place.
    pub fn with_error_code_mut(&mut self, error_code: impl Into<ErrorCode>) {
        if self.error_code.is_none() {
            self.error_code = Some(error_code.into());
        }
    }

    /// Sets the category, replacing any existing value.
    pub fn set_category(mut self, category: impl Into<StaticRefStr>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Sets the category, replacing any existing value (mutable reference version).
    ///
    /// This method avoids cloning the entire metadata when modifying in place.
    pub fn set_category_mut(&mut self, category: impl Into<StaticRefStr>) {
        self.category = Some(category.into());
    }

    /// Sets the category only if not already set.
    pub fn with_category(mut self, category: impl Into<StaticRefStr>) -> Self {
        if self.category.is_none() {
            self.category = Some(category.into());
        }
        self
    }

    /// Sets the category only if not already set (mutable reference version).
    ///
    /// This method avoids cloning the entire metadata when modifying in place.
    pub fn with_category_mut(&mut self, category: impl Into<StaticRefStr>) {
        if self.category.is_none() {
            self.category = Some(category.into());
        }
    }

    /// Sets the retryability flag, replacing any existing value.
    pub fn set_retryable(mut self, retryable: bool) -> Self {
        self.retryable = Some(retryable);
        self
    }

    /// Sets the retryability flag, replacing any existing value (mutable reference version).
    ///
    /// This method avoids cloning the entire metadata when modifying in place.
    pub fn set_retryable_mut(&mut self, retryable: bool) {
        self.retryable = Some(retryable);
    }

    /// Sets the retryability flag only if not already set.
    pub fn with_retryable(mut self, retryable: bool) -> Self {
        if self.retryable.is_none() {
            self.retryable = Some(retryable);
        }
        self
    }

    /// Sets the retryability flag only if not already set (mutable reference version).
    ///
    /// This method avoids cloning the entire metadata when modifying in place.
    pub fn with_retryable_mut(&mut self, retryable: bool) {
        if self.retryable.is_none() {
            self.retryable = Some(retryable);
        }
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

    /// Replaces the frames in the stack trace.
    pub fn set_frames(mut self, frames: Vec<StackFrame>) -> Self {
        self.frames = frames.into();
        self
    }

    /// Appends frames to the existing stack trace frames.
    pub fn with_frames(mut self, frames: Vec<StackFrame>) -> Self {
        let mut existing = self.frames.to_vec();
        existing.extend(frames);
        self.frames = existing.into();
        self
    }

    /// Sets the raw stack trace string, replacing any existing value.
    pub fn set_raw(mut self, raw: impl Into<StaticRefStr>) -> Self {
        self.raw = Some(raw.into());
        self
    }

    /// Sets the raw stack trace string only if not already set.
    pub fn with_raw(mut self, raw: impl Into<StaticRefStr>) -> Self {
        if self.raw.is_none() {
            self.raw = Some(raw.into());
        }
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
