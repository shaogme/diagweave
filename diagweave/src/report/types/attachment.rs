use alloc::borrow::ToOwned;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Display, Formatter};

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
