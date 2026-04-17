use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Display, Formatter};
use ref_str::StaticRefStr;

use super::{AttachmentValue, ErrorCode};
use crate::utils::FastMap;

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "json",
    serde(tag = "kind", content = "value", rename_all = "snake_case")
)]
pub enum ContextValue {
    String(StaticRefStr),
    Integer(i64),
    Unsigned(u64),
    Float(f64),
    Bool(bool),
    StringArray(Vec<StaticRefStr>),
    IntegerArray(Vec<i64>),
    UnsignedArray(Vec<u64>),
    FloatArray(Vec<f64>),
    BoolArray(Vec<bool>),
    Redacted {
        kind: Option<StaticRefStr>,
        reason: Option<StaticRefStr>,
    },
}

impl Display for ContextValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(value) => write!(f, "{value}"),
            Self::Integer(value) => write!(f, "{value}"),
            Self::Unsigned(value) => write!(f, "{value}"),
            Self::Float(value) => write!(f, "{value}"),
            Self::Bool(value) => write!(f, "{value}"),
            Self::StringArray(values) => fmt_array(f, values.iter()),
            Self::IntegerArray(values) => fmt_array(f, values.iter()),
            Self::UnsignedArray(values) => fmt_array(f, values.iter()),
            Self::FloatArray(values) => fmt_array(f, values.iter()),
            Self::BoolArray(values) => fmt_array(f, values.iter()),
            Self::Redacted { kind, reason } => match (kind, reason) {
                (Some(kind), Some(reason)) => write!(f, "<redacted:{kind}:{reason}>"),
                (Some(kind), None) => write!(f, "<redacted:{kind}>"),
                (None, Some(reason)) => write!(f, "<redacted:{reason}>"),
                (None, None) => write!(f, "<redacted>"),
            },
        }
    }
}

fn fmt_array<'a, T>(f: &mut Formatter<'_>, values: impl IntoIterator<Item = &'a T>) -> fmt::Result
where
    T: Display + 'a,
{
    write!(f, "[")?;
    for (idx, value) in values.into_iter().enumerate() {
        if idx > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{value}")?;
    }
    write!(f, "]")
}

impl From<String> for ContextValue {
    fn from(value: String) -> Self {
        Self::String(value.into())
    }
}

impl From<&'static str> for ContextValue {
    fn from(value: &'static str) -> Self {
        Self::String(value.into())
    }
}

impl From<StaticRefStr> for ContextValue {
    fn from(value: StaticRefStr) -> Self {
        Self::String(value)
    }
}

impl From<bool> for ContextValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i8> for ContextValue {
    fn from(value: i8) -> Self {
        Self::Integer(value as i64)
    }
}

impl From<i16> for ContextValue {
    fn from(value: i16) -> Self {
        Self::Integer(value as i64)
    }
}

impl From<i32> for ContextValue {
    fn from(value: i32) -> Self {
        Self::Integer(value as i64)
    }
}

impl From<i64> for ContextValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<u8> for ContextValue {
    fn from(value: u8) -> Self {
        Self::Unsigned(value as u64)
    }
}

impl From<u16> for ContextValue {
    fn from(value: u16) -> Self {
        Self::Unsigned(value as u64)
    }
}

impl From<u32> for ContextValue {
    fn from(value: u32) -> Self {
        Self::Unsigned(value as u64)
    }
}

impl From<u64> for ContextValue {
    fn from(value: u64) -> Self {
        Self::Unsigned(value)
    }
}

impl From<f32> for ContextValue {
    fn from(value: f32) -> Self {
        Self::Float(value as f64)
    }
}

impl From<f64> for ContextValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl From<Vec<String>> for ContextValue {
    fn from(value: Vec<String>) -> Self {
        Self::StringArray(value.into_iter().map(Into::into).collect())
    }
}

impl From<Vec<&'static str>> for ContextValue {
    fn from(value: Vec<&'static str>) -> Self {
        Self::StringArray(value.into_iter().map(Into::into).collect())
    }
}

impl From<Vec<StaticRefStr>> for ContextValue {
    fn from(value: Vec<StaticRefStr>) -> Self {
        Self::StringArray(value)
    }
}

impl From<Vec<bool>> for ContextValue {
    fn from(value: Vec<bool>) -> Self {
        Self::BoolArray(value)
    }
}

impl From<Vec<i8>> for ContextValue {
    fn from(value: Vec<i8>) -> Self {
        Self::IntegerArray(value.into_iter().map(i64::from).collect())
    }
}

impl From<Vec<i16>> for ContextValue {
    fn from(value: Vec<i16>) -> Self {
        Self::IntegerArray(value.into_iter().map(i64::from).collect())
    }
}

impl From<Vec<i32>> for ContextValue {
    fn from(value: Vec<i32>) -> Self {
        Self::IntegerArray(value.into_iter().map(i64::from).collect())
    }
}

impl From<Vec<i64>> for ContextValue {
    fn from(value: Vec<i64>) -> Self {
        Self::IntegerArray(value)
    }
}

impl From<Vec<u8>> for ContextValue {
    fn from(value: Vec<u8>) -> Self {
        Self::UnsignedArray(value.into_iter().map(u64::from).collect())
    }
}

impl From<Vec<u16>> for ContextValue {
    fn from(value: Vec<u16>) -> Self {
        Self::UnsignedArray(value.into_iter().map(u64::from).collect())
    }
}

impl From<Vec<u32>> for ContextValue {
    fn from(value: Vec<u32>) -> Self {
        Self::UnsignedArray(value.into_iter().map(u64::from).collect())
    }
}

impl From<Vec<u64>> for ContextValue {
    fn from(value: Vec<u64>) -> Self {
        Self::UnsignedArray(value)
    }
}

impl From<Vec<f32>> for ContextValue {
    fn from(value: Vec<f32>) -> Self {
        Self::FloatArray(value.into_iter().map(f64::from).collect())
    }
}

impl From<Vec<f64>> for ContextValue {
    fn from(value: Vec<f64>) -> Self {
        Self::FloatArray(value)
    }
}

impl From<ContextValue> for AttachmentValue {
    fn from(value: ContextValue) -> Self {
        match value {
            ContextValue::String(value) => Self::String(value),
            ContextValue::Integer(value) => Self::Integer(value),
            ContextValue::Unsigned(value) => Self::Unsigned(value),
            ContextValue::Float(value) => Self::Float(value),
            ContextValue::Bool(value) => Self::Bool(value),
            ContextValue::StringArray(values) => {
                Self::Array(values.into_iter().map(AttachmentValue::String).collect())
            }
            ContextValue::IntegerArray(values) => {
                Self::Array(values.into_iter().map(AttachmentValue::Integer).collect())
            }
            ContextValue::UnsignedArray(values) => {
                Self::Array(values.into_iter().map(AttachmentValue::Unsigned).collect())
            }
            ContextValue::FloatArray(values) => {
                Self::Array(values.into_iter().map(AttachmentValue::Float).collect())
            }
            ContextValue::BoolArray(values) => {
                Self::Array(values.into_iter().map(AttachmentValue::Bool).collect())
            }
            ContextValue::Redacted { kind, reason } => Self::Redacted { kind, reason },
        }
    }
}

impl From<&ContextValue> for AttachmentValue {
    fn from(value: &ContextValue) -> Self {
        Self::from(value.clone())
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct ContextMap(FastMap<StaticRefStr, ContextValue>);

impl ContextMap {
    /// Creates an empty context map.
    pub fn new() -> Self {
        Self(FastMap::new())
    }

    /// Returns `true` if the context map contains no entries.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the number of entries in the context map.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the context map contains a value for the specified key.
    pub fn contains_key<'a>(&self, key: impl Into<&'a str>) -> bool {
        self.0.contains_key(key.into())
    }

    /// Inserts a key-value pair into the context map.
    pub fn insert(&mut self, key: impl Into<StaticRefStr>, value: impl Into<ContextValue>) {
        self.0.insert(key.into(), value.into());
    }

    /// Returns an iterator over the key-value pairs in the context map.
    pub fn iter(&self) -> impl Iterator<Item = (&StaticRefStr, &ContextValue)> {
        self.0.iter()
    }

    /// Returns a vector of key-value pairs sorted by key.
    pub fn sorted_entries(&self) -> Vec<(&StaticRefStr, &ContextValue)> {
        self.0.sorted_entries()
    }
}

/// Default empty ContextMap singleton.
impl ContextMap {
    /// Returns a reference to the default empty ContextMap.
    #[cfg(feature = "std")]
    pub fn default_ref() -> &'static ContextMap {
        static DEFAULT: std::sync::LazyLock<ContextMap> = std::sync::LazyLock::new(ContextMap::new);
        &DEFAULT
    }

    /// Returns a reference to the default empty ContextMap.
    #[cfg(not(feature = "std"))]
    pub fn default_ref() -> &'static ContextMap {
        use alloc::boxed::Box;
        use core::ptr;
        static mut DEFAULT: *const ContextMap = ptr::null();
        // SAFETY: Single-threaded lazy init for no_std. Caller must ensure
        // no concurrent access during initialization.
        unsafe {
            if DEFAULT.is_null() {
                DEFAULT = Box::leak(Box::new(ContextMap::new()));
            }
            &*DEFAULT
        }
    }
}

impl<'a> IntoIterator for &'a ContextMap {
    type Item = (&'a StaticRefStr, &'a ContextValue);
    type IntoIter = <&'a FastMap<StaticRefStr, ContextValue> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        (&self.0).into_iter()
    }
}

/// A JSON-serialized context map, containing an ordered list of entries.
#[cfg(feature = "json")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JsonContext {
    /// The ordered list of context entries.
    pub entries: Vec<JsonContextEntry>,
}

/// A single entry in a JSON-serialized context map.
#[cfg(feature = "json")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JsonContextEntry {
    /// The context key.
    pub key: StaticRefStr,
    /// The context value.
    pub value: ContextValue,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct GlobalErrorMeta {
    pub error_code: Option<ErrorCode>,
    pub category: Option<StaticRefStr>,
    pub retryable: Option<bool>,
}

impl GlobalErrorMeta {
    /// Returns `true` if all metadata fields are `None`.
    pub fn is_empty(&self) -> bool {
        self.error_code.is_none() && self.category.is_none() && self.retryable.is_none()
    }
}
