use super::{
    close_array, close_object, write_array_item_prefix, write_json_display, write_json_string,
    write_object_field, write_option_string,
};
use crate::report::{
    AttachmentValue, AttachmentVisit, ContextValue, JsonContext, JsonContextEntry, Report,
    SeverityState, SystemContext,
};
use alloc::vec::Vec;
use core::error::Error;
use core::fmt::{self, Display, Formatter, Write};

pub(super) fn write_context_object<E, State>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<E, State>,
) -> fmt::Result
where
    E: Error + Display + 'static,
    State: SeverityState,
{
    let mut first = true;
    f.write_char('{')?;
    let context = build_json_context(report.context());
    for entry in context.entries {
        write_object_field(f, pretty, depth, &mut first, entry.key.as_ref(), |f| {
            write_context_value(f, pretty, depth + 1, &entry.value)
        })?;
    }
    close_object(f, pretty, depth, first)
}

pub(super) fn write_system_object<E, State>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<E, State>,
) -> fmt::Result
where
    E: Error + Display + 'static,
    State: SeverityState,
{
    let mut first = true;
    f.write_char('{')?;
    let system = build_json_system(report.system());
    for section in &system.sections {
        write_object_field(f, pretty, depth, &mut first, section.name, |f| {
            let mut section_first = true;
            f.write_char('{')?;
            for entry in &section.entries {
                write_object_field(
                    f,
                    pretty,
                    depth + 1,
                    &mut section_first,
                    entry.key.as_ref(),
                    |f| write_context_value(f, pretty, depth + 2, &entry.value),
                )?;
            }
            close_object(f, pretty, depth + 1, section_first)
        })?;
    }
    close_object(f, pretty, depth, first)
}

pub(super) fn write_context_value(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    value: &ContextValue,
) -> fmt::Result {
    match value {
        ContextValue::Null => write_kind_only(f, pretty, depth, "null"),
        ContextValue::String(v) => write_kind_and_value(f, pretty, depth, "string", |f| {
            write_json_string(f, v.as_ref())
        }),
        ContextValue::Integer(v) => {
            write_kind_and_value(f, pretty, depth, "integer", |f| write!(f, "{v}"))
        }
        ContextValue::Unsigned(v) => {
            write_kind_and_value(f, pretty, depth, "unsigned", |f| write!(f, "{v}"))
        }
        ContextValue::Float(v) => {
            if !v.is_finite() {
                Err(fmt::Error)
            } else {
                write_kind_and_value(f, pretty, depth, "float", |f| write!(f, "{v}"))
            }
        }
        ContextValue::Bool(v) => {
            write_kind_and_value(f, pretty, depth, "bool", |f| write!(f, "{v}"))
        }
        ContextValue::StringArray(values) => {
            write_context_array(f, pretty, depth, "string_array", values, |f, value| {
                write_json_string(f, value.as_ref())
            })
        }
        ContextValue::IntegerArray(values) => {
            write_context_array(f, pretty, depth, "integer_array", values, |f, value| {
                write!(f, "{value}")
            })
        }
        ContextValue::UnsignedArray(values) => {
            write_context_array(f, pretty, depth, "unsigned_array", values, |f, value| {
                write!(f, "{value}")
            })
        }
        ContextValue::FloatArray(values) => {
            if values.iter().any(|value| !value.is_finite()) {
                return Err(fmt::Error);
            }
            write_context_array(f, pretty, depth, "float_array", values, |f, value| {
                write!(f, "{value}")
            })
        }
        ContextValue::BoolArray(values) => {
            write_context_array(f, pretty, depth, "bool_array", values, |f, value| {
                write!(f, "{value}")
            })
        }
        ContextValue::Redacted { kind, reason } => {
            write_redacted_obj(f, pretty, depth, kind.as_deref(), reason.as_deref())
        }
    }
}

fn write_context_array<T, F>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    kind: &str,
    values: &[T],
    mut write_value: F,
) -> fmt::Result
where
    F: FnMut(&mut Formatter<'_>, &T) -> fmt::Result,
{
    write_kind_and_value(f, pretty, depth, kind, |f| {
        let mut first = true;
        f.write_char('[')?;
        for value in values {
            write_array_item_prefix(f, pretty, depth + 1, &mut first)?;
            write_value(f, value)?;
        }
        close_array(f, pretty, depth + 1, first)
    })
}

fn build_json_context(context: Option<&crate::report::ContextMap>) -> JsonContext
where
{
    let entries: Vec<JsonContextEntry> = context
        .map(|system| {
            system
                .sorted_entries()
                .into_iter()
                .map(|(key, value)| JsonContextEntry {
                    key: key.clone(),
                    value: value.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    JsonContext { entries }
}

struct JsonSystemSection<'a> {
    name: &'a str,
    entries: Vec<JsonContextEntry>,
}

struct JsonSystem<'a> {
    sections: Vec<JsonSystemSection<'a>>,
}

fn build_json_system(system: Option<&SystemContext>) -> JsonSystem<'_> {
    let mut sections = Vec::new();
    if let Some(system) = system {
        for (name, section) in system.sections() {
            sections.push(JsonSystemSection {
                name,
                entries: build_json_context(Some(section)).entries,
            });
        }
    }
    JsonSystem { sections }
}

pub(super) fn write_attachments_array<E, State>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<E, State>,
) -> fmt::Result
where
    E: Error + Display + 'static,
    State: SeverityState,
{
    let mut first = true;
    f.write_char('[')?;
    report.visit_attachments(|item| {
        match item {
            AttachmentVisit::Note { message } => {
                write_array_item_prefix(f, pretty, depth, &mut first)?;
                write_note_obj(f, pretty, depth + 1, message)?;
            }
            AttachmentVisit::Payload {
                name,
                value,
                media_type,
            } => {
                write_array_item_prefix(f, pretty, depth, &mut first)?;
                write_payload_obj(PayloadArgs {
                    f,
                    pretty,
                    depth: depth + 1,
                    name: name.as_ref(),
                    value,
                    media_type: media_type.map(|m| m.as_ref()),
                })?;
            }
        }
        Ok(())
    })?;
    close_array(f, pretty, depth, first)
}

fn write_note_obj(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    message: &(impl Display + ?Sized),
) -> fmt::Result {
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "kind", |f| {
        write_json_string(f, "note")
    })?;
    write_object_field(f, pretty, depth, &mut first, "message", |f| {
        write_json_display(f, message)
    })?;
    close_object(f, pretty, depth, first)
}

struct PayloadArgs<'a, 'b> {
    f: &'a mut Formatter<'b>,
    pretty: bool,
    depth: usize,
    name: &'a str,
    value: &'a AttachmentValue,
    media_type: Option<&'a str>,
}

fn write_payload_obj(args: PayloadArgs<'_, '_>) -> fmt::Result {
    let PayloadArgs {
        f,
        pretty,
        depth,
        name,
        value,
        media_type,
    } = args;
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
            for (key, item) in values.sorted_entries() {
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
