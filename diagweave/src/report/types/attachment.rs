use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::fmt::{self, Display, Formatter};
use ref_str::StaticRefStr;

use crate::utils::FastMap;

#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "json",
    serde(tag = "kind", content = "value", rename_all = "snake_case")
)]
/// Represents a value that can be attached to a diagnostic report payload.
pub enum AttachmentValue {
    #[default]
    Null,
    String(StaticRefStr),
    Integer(i64),
    Unsigned(u64),
    Float(f64),
    Bool(bool),
    Array(Vec<AttachmentValue>),
    Object(FastMap<StaticRefStr, AttachmentValue>),
    Bytes(Vec<u8>),
    Redacted {
        kind: Option<StaticRefStr>,
        reason: Option<StaticRefStr>,
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
                for (idx, (key, value)) in values.sorted_entries().into_iter().enumerate() {
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
        Self::String(value.into())
    }
}

impl From<&'static str> for AttachmentValue {
    fn from(value: &'static str) -> Self {
        Self::String(value.into())
    }
}

impl From<StaticRefStr> for AttachmentValue {
    fn from(value: StaticRefStr) -> Self {
        Self::String(value)
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
        Self::Array(value.into_iter().map(|s| Self::String(s.into())).collect())
    }
}

impl From<Vec<&'static str>> for AttachmentValue {
    fn from(value: Vec<&'static str>) -> Self {
        Self::Array(value.into_iter().map(|s| Self::String(s.into())).collect())
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

impl<V, K: Into<StaticRefStr>> From<FastMap<K, V>> for AttachmentValue
where
    V: Into<AttachmentValue>,
{
    fn from(value: FastMap<K, V>) -> Self {
        Self::Object(value.into_iter().map(|(k, v)| (k.into(), v.into())).collect())
    }
}

impl<V, K: Into<StaticRefStr>> From<BTreeMap<K, V>> for AttachmentValue
where
    V: Into<AttachmentValue>,
{
    fn from(value: BTreeMap<K, V>) -> Self {
        Self::Object(value.into_iter().map(|(k, v)| (k.into(), v.into())).collect())
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
            serde_json::Value::String(s) => Self::String(s.into()),
            serde_json::Value::Array(arr) => {
                Self::Array(arr.into_iter().map(AttachmentValue::from).collect())
            }
            serde_json::Value::Object(obj) => {
                let mut map = FastMap::with_capacity(obj.len());
                for (k, v) in obj {
                    map.insert(k.into(), AttachmentValue::from(v));
                }
                Self::Object(map)
            }
        }
    }
}

/// Represents an attachment to a diagnostic report, such as context, notes, or payloads.
pub enum Attachment {
    Note {
        message: Box<dyn Display + Send + Sync + 'static>,
    },
    Payload {
        name: StaticRefStr,
        value: AttachmentValue,
        media_type: Option<StaticRefStr>,
    },
}

impl Clone for Attachment {
    fn clone(&self) -> Self {
        match self {
            Self::Note { message } => Self::Note {
                message: Box::new(message.to_string()),
            },
            Self::Payload {
                name,
                value,
                media_type,
            } => Self::Payload {
                name: name.clone(),
                value: value.clone(),
                media_type: media_type.clone(),
            },
        }
    }
}

impl PartialEq for Attachment {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Note { message: l }, Self::Note { message: r }) => {
                l.to_string() == r.to_string()
            }
            (
                Self::Payload {
                    name: l_name,
                    value: l_value,
                    media_type: l_media_type,
                },
                Self::Payload {
                    name: r_name,
                    value: r_value,
                    media_type: r_media_type,
                },
            ) => l_name == r_name && l_value == r_value && l_media_type == r_media_type,
            _ => false,
        }
    }
}

impl core::fmt::Debug for Attachment {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Note { message } => f
                .debug_struct("Note")
                .field("message", &message.to_string())
                .finish(),
            Self::Payload {
                name,
                value,
                media_type,
            } => f
                .debug_struct("Payload")
                .field("name", name)
                .field("value", value)
                .field("media_type", media_type)
                .finish(),
        }
    }
}

#[cfg(feature = "json")]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AttachmentSerde {
    Note {
        message: String,
    },
    Payload {
        name: StaticRefStr,
        value: AttachmentValue,
        media_type: Option<StaticRefStr>,
    },
}

#[cfg(feature = "json")]
impl serde::Serialize for Attachment {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let helper = match self {
            Self::Note { message } => AttachmentSerde::Note {
                message: message.to_string(),
            },
            Self::Payload {
                name,
                value,
                media_type,
            } => AttachmentSerde::Payload {
                name: name.clone(),
                value: value.clone(),
                media_type: media_type.clone(),
            },
        };
        helper.serialize(serializer)
    }
}

#[cfg(feature = "json")]
impl<'de> serde::Deserialize<'de> for Attachment {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let helper = AttachmentSerde::deserialize(deserializer)?;
        Ok(match helper {
            AttachmentSerde::Note { message } => Self::Note {
                message: Box::new(message),
            },
            AttachmentSerde::Payload {
                name,
                value,
                media_type,
            } => Self::Payload {
                name,
                value,
                media_type,
            },
        })
    }
}

impl Attachment {
    /// Creates a new note attachment with a message.
    pub fn note(message: impl Display + Send + Sync + 'static) -> Self {
        Self::Note {
            message: Box::new(message),
        }
    }

    /// Creates a new payload attachment with a name, value, and optional media type.
    pub fn payload(
        name: impl Into<StaticRefStr>,
        value: impl Into<AttachmentValue>,
        media_type: Option<impl Into<StaticRefStr>>,
    ) -> Self {
        Self::Payload {
            name: name.into(),
            value: value.into(),
            media_type: media_type.map(|m| m.into()),
        }
    }

    /// Attempts to interpret the attachment as a note message.
    pub fn as_note(&self) -> Option<String> {
        match self {
            Self::Note { message } => Some(message.to_string()),
            Self::Payload { .. } => None,
        }
    }

    /// Returns the note as `Display` for zero-allocation access.
    pub fn as_note_display(&self) -> Option<&(dyn Display + Send + Sync + 'static)> {
        match self {
            Self::Note { message } => Some(message.as_ref()),
            Self::Payload { .. } => None,
        }
    }

    /// Attempts to interpret the attachment as a payload.
    pub fn as_payload(&self) -> Option<(&str, &AttachmentValue, Option<&str>)> {
        match self {
            Self::Payload {
                name,
                value,
                media_type,
            } => Some((
                name.as_str(),
                value,
                media_type.as_ref().map(|v| v.as_str()),
            )),
            Self::Note { .. } => None,
        }
    }
}
