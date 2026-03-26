use alloc::borrow::ToOwned;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::convert::TryFrom;
use core::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(rename_all = "snake_case"))]
pub enum Severity {
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl Display for Severity {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
            Self::Fatal => "fatal",
        };
        write!(f, "{label}")
    }
}

/// An error code that can be either an integer or a string.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(untagged))]
pub enum ErrorCode {
    /// An integer error code.
    Integer(i64),
    /// A string error code.
    String(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCodeIntError {
    InvalidIntegerString,
    OutOfRange,
}

impl Display for ErrorCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Integer(v) => write!(f, "{v}"),
            Self::String(v) => write!(f, "{v}"),
        }
    }
}

impl From<ErrorCode> for String {
    fn from(value: ErrorCode) -> Self {
        match value {
            ErrorCode::Integer(v) => v.to_string(),
            ErrorCode::String(v) => v,
        }
    }
}

impl From<&ErrorCode> for String {
    fn from(value: &ErrorCode) -> Self {
        value.to_string()
    }
}

macro_rules! impl_error_code_from_integer_try_into_i64 {
    ($($ty:ty),* $(,)?) => {
        $(
            impl From<$ty> for ErrorCode {
                fn from(v: $ty) -> Self {
                    match i64::try_from(v) {
                        Ok(value) => Self::Integer(value),
                        Err(_) => Self::String(v.to_string()),
                    }
                }
            }
        )*
    };
}

impl_error_code_from_integer_try_into_i64!(
    i8, i16, i32, i64, isize,
    u8, u16, u32, u64, usize,
    i128, u128,
);

macro_rules! impl_try_from_error_code_for_signed_int {
    ($($ty:ty),* $(,)?) => {
        $(
            impl TryFrom<ErrorCode> for $ty {
                type Error = ErrorCodeIntError;

                fn try_from(value: ErrorCode) -> Result<Self, Self::Error> {
                    match value {
                        ErrorCode::Integer(v) => <$ty>::try_from(v).map_err(|_| ErrorCodeIntError::OutOfRange),
                        ErrorCode::String(v) => {
                            let parsed = v
                                .parse::<i128>()
                                .map_err(|_| ErrorCodeIntError::InvalidIntegerString)?;
                            <$ty>::try_from(parsed).map_err(|_| ErrorCodeIntError::OutOfRange)
                        }
                    }
                }
            }

            impl TryFrom<&ErrorCode> for $ty {
                type Error = ErrorCodeIntError;

                fn try_from(value: &ErrorCode) -> Result<Self, Self::Error> {
                    match value {
                        ErrorCode::Integer(v) => <$ty>::try_from(*v).map_err(|_| ErrorCodeIntError::OutOfRange),
                        ErrorCode::String(v) => {
                            let parsed = v
                                .parse::<i128>()
                                .map_err(|_| ErrorCodeIntError::InvalidIntegerString)?;
                            <$ty>::try_from(parsed).map_err(|_| ErrorCodeIntError::OutOfRange)
                        }
                    }
                }
            }
        )*
    };
}

macro_rules! impl_try_from_error_code_for_unsigned_int {
    ($($ty:ty),* $(,)?) => {
        $(
            impl TryFrom<ErrorCode> for $ty {
                type Error = ErrorCodeIntError;

                fn try_from(value: ErrorCode) -> Result<Self, Self::Error> {
                    match value {
                        ErrorCode::Integer(v) => <$ty>::try_from(v).map_err(|_| ErrorCodeIntError::OutOfRange),
                        ErrorCode::String(v) => {
                            let parsed = v
                                .parse::<u128>()
                                .map_err(|_| ErrorCodeIntError::InvalidIntegerString)?;
                            <$ty>::try_from(parsed).map_err(|_| ErrorCodeIntError::OutOfRange)
                        }
                    }
                }
            }

            impl TryFrom<&ErrorCode> for $ty {
                type Error = ErrorCodeIntError;

                fn try_from(value: &ErrorCode) -> Result<Self, Self::Error> {
                    match value {
                        ErrorCode::Integer(v) => <$ty>::try_from(*v).map_err(|_| ErrorCodeIntError::OutOfRange),
                        ErrorCode::String(v) => {
                            let parsed = v
                                .parse::<u128>()
                                .map_err(|_| ErrorCodeIntError::InvalidIntegerString)?;
                            <$ty>::try_from(parsed).map_err(|_| ErrorCodeIntError::OutOfRange)
                        }
                    }
                }
            }
        )*
    };
}

impl_try_from_error_code_for_signed_int!(i8, i16, i32, i64, isize, i128);
impl_try_from_error_code_for_unsigned_int!(u8, u16, u32, u64, usize, u128);

impl From<String> for ErrorCode {
    fn from(v: String) -> Self {
        Self::String(v)
    }
}

impl From<&str> for ErrorCode {
    fn from(v: &str) -> Self {
        Self::String(v.to_owned())
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct ReportMetadata {
    pub error_code: Option<ErrorCode>,
    pub severity: Option<Severity>,
    pub category: Option<String>,
    pub retryable: Option<bool>,
    pub stack_trace: Option<StackTrace>,
    pub display_causes: Option<DisplayCauseChain>,
    pub source_errors: Option<SourceErrorChain>,
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

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct DisplayCauseChain {
    pub items: Vec<String>,
    pub truncated: bool,
    pub cycle_detected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct SourceError {
    pub message: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct SourceErrorChain {
    pub items: Vec<SourceError>,
    pub truncated: bool,
    pub cycle_detected: bool,
}

#[cfg(feature = "trace")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(rename_all = "snake_case"))]
pub enum TraceEventLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[cfg(feature = "trace")]
impl Display for TraceEventLevel {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        };
        write!(f, "{label}")
    }
}

#[cfg(feature = "trace")]
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct TraceEventAttribute {
    pub key: String,
    pub value: AttachmentValue,
}

#[cfg(feature = "trace")]
impl Default for TraceEventAttribute {
    fn default() -> Self {
        Self {
            key: String::new(),
            value: AttachmentValue::Null,
        }
    }
}

#[cfg(feature = "trace")]
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct TraceEvent {
    pub name: String,
    pub level: Option<TraceEventLevel>,
    pub timestamp_unix_nano: Option<u64>,
    pub attributes: Vec<TraceEventAttribute>,
}

#[cfg(feature = "trace")]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct TraceContext {
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub parent_span_id: Option<String>,
    pub sampled: Option<bool>,
    pub trace_state: Option<String>,
    pub flags: Option<u32>,
}

#[cfg(feature = "trace")]
impl TraceContext {
    /// Returns true if the trace context is empty (no IDs or flags).
    pub fn is_empty(&self) -> bool {
        self.trace_id.is_none()
            && self.span_id.is_none()
            && self.parent_span_id.is_none()
            && self.sampled.is_none()
            && self.trace_state.is_none()
            && self.flags.is_none()
    }
}

#[cfg(feature = "trace")]
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct ReportTrace {
    pub context: TraceContext,
    pub events: Vec<TraceEvent>,
}

#[cfg(feature = "trace")]
impl ReportTrace {
    /// Returns true if the report trace is empty (no context and no events).
    pub fn is_empty(&self) -> bool {
        self.context.is_empty() && self.events.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "json",
    serde(tag = "kind", content = "value", rename_all = "snake_case")
)]
pub enum AttachmentValue {
    #[default]
    Null,
    String(String),
    Integer(i64),
    Unsigned(u64),
    Float(f64),
    Bool(bool),
    Array(Vec<AttachmentValue>),
    Object(BTreeMap<String, AttachmentValue>),
    Bytes(Vec<u8>),
    Redacted {
        kind: Option<String>,
        reason: Option<String>,
    },
}

impl From<&str> for Severity {
    fn from(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "debug" => Self::Debug,
            "info" => Self::Info,
            "warn" | "warning" => Self::Warn,
            "error" => Self::Error,
            "fatal" | "critical" => Self::Fatal,
            _ => Self::Error,
        }
    }
}

impl From<String> for Severity {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl Display for AttachmentValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::String(value) => write!(f, "{value}"),
            Self::Integer(value) => write!(f, "{value}"),
            Self::Unsigned(value) => write!(f, "{value}"),
            Self::Float(value) => write!(f, "{value}"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::Array(values) => {
                write!(f, "[")?;
                for (idx, value) in values.iter().enumerate() {
                    if idx > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{value}")?;
                }
                write!(f, "]")
            }
            Self::Object(values) => {
                write!(f, "{{")?;
                for (idx, (key, value)) in values.iter().enumerate() {
                    if idx > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{key}: {value}")?;
                }
                write!(f, "}}")
            }
            Self::Bytes(bytes) => write!(f, "<{} bytes>", bytes.len()),
            Self::Redacted { kind, reason } => match (kind, reason) {
                (Some(kind), Some(reason)) => write!(f, "<redacted:{kind}:{reason}>"),
                (Some(kind), None) => write!(f, "<redacted:{kind}>"),
                (None, Some(reason)) => write!(f, "<redacted:{reason}>"),
                (None, None) => write!(f, "<redacted>"),
            },
        }
    }
}

impl From<String> for AttachmentValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for AttachmentValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<bool> for AttachmentValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i8> for AttachmentValue {
    fn from(value: i8) -> Self {
        Self::Integer(value as i64)
    }
}

impl From<i16> for AttachmentValue {
    fn from(value: i16) -> Self {
        Self::Integer(value as i64)
    }
}

impl From<i32> for AttachmentValue {
    fn from(value: i32) -> Self {
        Self::Integer(value as i64)
    }
}

impl From<i64> for AttachmentValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<u8> for AttachmentValue {
    fn from(value: u8) -> Self {
        Self::Unsigned(value as u64)
    }
}

impl From<u16> for AttachmentValue {
    fn from(value: u16) -> Self {
        Self::Unsigned(value as u64)
    }
}

impl From<u32> for AttachmentValue {
    fn from(value: u32) -> Self {
        Self::Unsigned(value as u64)
    }
}

impl From<u64> for AttachmentValue {
    fn from(value: u64) -> Self {
        Self::Unsigned(value)
    }
}

impl From<f32> for AttachmentValue {
    fn from(value: f32) -> Self {
        Self::Float(value as f64)
    }
}

impl From<f64> for AttachmentValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl From<Vec<String>> for AttachmentValue {
    fn from(value: Vec<String>) -> Self {
        Self::Array(value.into_iter().map(Self::String).collect())
    }
}

impl From<Vec<&str>> for AttachmentValue {
    fn from(value: Vec<&str>) -> Self {
        Self::Array(
            value
                .into_iter()
                .map(|s| Self::String(s.to_owned()))
                .collect(),
        )
    }
}

impl From<Vec<u8>> for AttachmentValue {
    fn from(value: Vec<u8>) -> Self {
        Self::Bytes(value)
    }
}

impl<T> From<Option<T>> for AttachmentValue
where
    T: Into<AttachmentValue>,
{
    fn from(value: Option<T>) -> Self {
        match value {
            Some(v) => v.into(),
            None => Self::Null,
        }
    }
}

impl<V> From<BTreeMap<String, V>> for AttachmentValue
where
    V: Into<AttachmentValue>,
{
    fn from(value: BTreeMap<String, V>) -> Self {
        Self::Object(value.into_iter().map(|(k, v)| (k, v.into())).collect())
    }
}

#[cfg(feature = "json")]
impl From<serde_json::Value> for AttachmentValue {
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(b) => Self::Bool(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Self::Integer(i)
                } else if let Some(u) = n.as_u64() {
                    Self::Unsigned(u)
                } else {
                    Self::Float(n.as_f64().unwrap_or(0.0))
                }
            }
            serde_json::Value::String(s) => Self::String(s),
            serde_json::Value::Array(arr) => {
                Self::Array(arr.into_iter().map(AttachmentValue::from).collect())
            }
            serde_json::Value::Object(obj) => {
                let mut map = BTreeMap::new();
                for (k, v) in obj {
                    map.insert(k, AttachmentValue::from(v));
                }
                Self::Object(map)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(tag = "kind", rename_all = "snake_case"))]
pub enum Attachment {
    Context {
        key: String,
        value: AttachmentValue,
    },
    Note {
        message: String,
    },
    Payload {
        name: String,
        value: AttachmentValue,
        media_type: Option<String>,
    },
}

impl Attachment {
    /// Creates a new context attachment with a key and value.
    pub fn context(key: impl Into<String>, value: impl Into<AttachmentValue>) -> Self {
        Self::Context {
            key: key.into(),
            value: value.into(),
        }
    }

    /// Creates a new note attachment with a message.
    pub fn note(message: impl Into<String>) -> Self {
        Self::Note {
            message: message.into(),
        }
    }

    /// Creates a new payload attachment with a name, value, and optional media type.
    pub fn payload(
        name: impl Into<String>,
        value: impl Into<AttachmentValue>,
        media_type: Option<String>,
    ) -> Self {
        Self::Payload {
            name: name.into(),
            value: value.into(),
            media_type,
        }
    }

    /// Attempts to interpret the attachment as a context entry.
    pub fn as_context(&self) -> Option<(&str, &AttachmentValue)> {
        match self {
            Self::Context { key, value } => Some((key.as_str(), value)),
            Self::Note { .. } | Self::Payload { .. } => None,
        }
    }

    /// Attempts to interpret the attachment as a note message.
    pub fn as_note(&self) -> Option<&str> {
        match self {
            Self::Note { message } => Some(message.as_str()),
            Self::Context { .. } | Self::Payload { .. } => None,
        }
    }

    /// Attempts to interpret the attachment as a payload.
    pub fn as_payload(&self) -> Option<(&str, &AttachmentValue, Option<&str>)> {
        match self {
            Self::Payload {
                name,
                value,
                media_type,
            } => Some((name.as_str(), value, media_type.as_deref())),
            Self::Context { .. } | Self::Note { .. } => None,
        }
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

/// A collection of cause messages and metadata.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CauseCollection {
    /// The collected cause message strings.
    pub messages: Vec<String>,
    /// Whether the collection was truncated due to depth limits.
    pub truncated: bool,
    /// Whether a circular reference was detected.
    pub cycle_detected: bool,
}

impl CauseCollection {
    /// Returns true if no messages were collected.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Returns the number of collected messages.
    pub fn len(&self) -> usize {
        self.messages.len()
    }
}
