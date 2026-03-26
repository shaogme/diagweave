use super::{
    close_array, close_object, write_array_item_prefix, write_json_string, write_object_field,
    write_option_string,
};
use crate::report::{AttachmentValue, AttachmentVisit, Report};
use core::error::Error;
use core::fmt::{self, Display, Formatter, Write};

pub(super) fn write_context_array<E>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<E>,
) -> fmt::Result
where
    E: Error + Display + 'static,
{
    let mut first = true;
    f.write_char('[')?;
    report.visit_attachments(|item| {
        let AttachmentVisit::Context { key, value } = item else {
            return Ok(());
        };
        write_array_item_prefix(f, pretty, depth, &mut first)?;
        write_kv_obj(f, pretty, depth + 1, key.as_ref(), value)
    })?;
    close_array(f, pretty, depth, first)
}

pub(super) fn write_attachments_array<E>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<E>,
) -> fmt::Result
where
    E: Error + Display + 'static,
{
    let mut first = true;
    f.write_char('[')?;
    report.visit_attachments(|item| {
        match item {
            AttachmentVisit::Context { .. } => {}
            AttachmentVisit::Note { message } => {
                write_array_item_prefix(f, pretty, depth, &mut first)?;
                write_note_obj(f, pretty, depth + 1, message.as_ref())?;
            }
            AttachmentVisit::Payload {
                name,
                value,
                media_type,
            } => {
                write_array_item_prefix(f, pretty, depth, &mut first)?;
                write_payload_obj(
                    f,
                    pretty,
                    depth + 1,
                    name.as_ref(),
                    value,
                    media_type.map(|m| m.as_ref()),
                )?;
            }
        }
        Ok(())
    })?;
    close_array(f, pretty, depth, first)
}

fn write_note_obj(f: &mut Formatter<'_>, pretty: bool, depth: usize, message: &str) -> fmt::Result {
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "kind", |f| {
        write_json_string(f, "note")
    })?;
    write_object_field(f, pretty, depth, &mut first, "message", |f| {
        write_json_string(f, message)
    })?;
    close_object(f, pretty, depth, first)
}

fn write_payload_obj(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    name: &str,
    value: &AttachmentValue,
    media_type: Option<&str>,
) -> fmt::Result {
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "kind", |f| {
        write_json_string(f, "payload")
    })?;
    write_object_field(f, pretty, depth, &mut first, "name", |f| {
        write_json_string(f, name)
    })?;
    write_object_field(f, pretty, depth, &mut first, "value", |f| {
        write_attachment_value(f, pretty, depth + 1, value)
    })?;
    write_object_field(
        f,
        pretty,
        depth,
        &mut first,
        "media_type",
        |f| match media_type {
            Some(media_type) => write_json_string(f, media_type),
            None => f.write_str("null"),
        },
    )?;
    close_object(f, pretty, depth, first)
}

fn write_kv_obj(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    key: &str,
    value: &AttachmentValue,
) -> fmt::Result {
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "key", |f| {
        write_json_string(f, key)
    })?;
    write_object_field(f, pretty, depth, &mut first, "value", |f| {
        write_attachment_value(f, pretty, depth + 1, value)
    })?;
    close_object(f, pretty, depth, first)
}

pub(super) fn write_attachment_value(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    value: &AttachmentValue,
) -> fmt::Result {
    if let Some(result) = write_scalar_value(f, pretty, depth, value) {
        return result;
    }

    match value {
        AttachmentValue::Null
        | AttachmentValue::String(_)
        | AttachmentValue::Integer(_)
        | AttachmentValue::Unsigned(_)
        | AttachmentValue::Float(_)
        | AttachmentValue::Bool(_) => Err(fmt::Error),
        AttachmentValue::Array(values) => write_kind_and_value(f, pretty, depth, "array", |f| {
            let mut first = true;
            f.write_char('[')?;
            for item in values {
                write_array_item_prefix(f, pretty, depth + 1, &mut first)?;
                write_attachment_value(f, pretty, depth + 2, item)?;
            }
            close_array(f, pretty, depth + 1, first)
        }),
        AttachmentValue::Object(values) => write_kind_and_value(f, pretty, depth, "object", |f| {
            let mut first = true;
            f.write_char('{')?;
            for (key, item) in values {
                write_object_field(f, pretty, depth + 1, &mut first, key, |f| {
                    write_attachment_value(f, pretty, depth + 2, item)
                })?;
            }
            close_object(f, pretty, depth + 1, first)
        }),
        AttachmentValue::Bytes(bytes) => write_kind_and_value(f, pretty, depth, "bytes", |f| {
            let mut first = true;
            f.write_char('[')?;
            for byte in bytes {
                write_array_item_prefix(f, pretty, depth + 1, &mut first)?;
                write!(f, "{byte}")?;
            }
            close_array(f, pretty, depth + 1, first)
        }),
        AttachmentValue::Redacted { kind, reason } => {
            write_redacted_obj(f, pretty, depth, kind.as_deref(), reason.as_deref())
        }
    }
}

fn write_redacted_obj(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    kind: Option<&str>,
    reason: Option<&str>,
) -> fmt::Result {
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "kind", |f| {
        write_json_string(f, "redacted")
    })?;
    write_object_field(f, pretty, depth, &mut first, "value", |f| {
        let mut inner_first = true;
        f.write_char('{')?;
        write_object_field(f, pretty, depth + 1, &mut inner_first, "kind", |f| {
            write_option_string(f, kind)
        })?;
        write_object_field(f, pretty, depth + 1, &mut inner_first, "reason", |f| {
            write_option_string(f, reason)
        })?;
        close_object(f, pretty, depth + 1, inner_first)
    })?;
    close_object(f, pretty, depth, first)
}

fn write_scalar_value(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    value: &AttachmentValue,
) -> Option<fmt::Result> {
    match value {
        AttachmentValue::Null => Some(write_kind_only(f, pretty, depth, "null")),
        AttachmentValue::String(v) => Some(write_kind_and_value(f, pretty, depth, "string", |f| {
            write_json_string(f, v.as_ref())
        })),
        AttachmentValue::Integer(v) => {
            Some(write_kind_and_value(f, pretty, depth, "integer", |f| {
                write!(f, "{v}")
            }))
        }
        AttachmentValue::Unsigned(v) => {
            Some(write_kind_and_value(f, pretty, depth, "unsigned", |f| {
                write!(f, "{v}")
            }))
        }
        AttachmentValue::Float(v) => {
            if !v.is_finite() {
                Some(Err(fmt::Error))
            } else {
                Some(write_kind_and_value(f, pretty, depth, "float", |f| {
                    write!(f, "{v}")
                }))
            }
        }
        AttachmentValue::Bool(v) => Some(write_kind_and_value(f, pretty, depth, "bool", |f| {
            write!(f, "{v}")
        })),
        _ => None,
    }
}

fn write_kind_only(f: &mut Formatter<'_>, pretty: bool, depth: usize, kind: &str) -> fmt::Result {
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "kind", |f| {
        write_json_string(f, kind)
    })?;
    close_object(f, pretty, depth, first)
}

fn write_kind_and_value<F>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    kind: &str,
    mut write_value: F,
) -> fmt::Result
where
    F: FnMut(&mut Formatter<'_>) -> fmt::Result,
{
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "kind", |f| {
        write_json_string(f, kind)
    })?;
    write_object_field(f, pretty, depth, &mut first, "value", |f| write_value(f))?;
    close_object(f, pretty, depth, first)
}
