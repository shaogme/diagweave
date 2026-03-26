use core::error::Error;
use core::fmt::{self, Display, Formatter, Write};

use crate::report::{
    AttachmentValue, CauseCollectOptions, ErrorCode, Report, StackTrace,
};

#[cfg(feature = "trace")]
use crate::report::{TraceContext, TraceEvent, TraceEventAttribute};

use super::{AttachmentPayloadRef, ReportRenderOptions, ReportRenderer, dispatch_attachments};

/// A renderer that outputs the diagnostic report in JSON format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Json {
    pub options: ReportRenderOptions,
}

pub const REPORT_JSON_SCHEMA_VERSION: &str = "v0.1.0";

pub const REPORT_JSON_SCHEMA_DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";

/// Returns the JSON schema for the diagnostic report.
pub fn report_json_schema() -> &'static str {
    include_str!("../../schemas/report-v0.1.0.schema.json")
}

impl Json {
    /// Creates a new JSON renderer with specific options.
    pub fn new(options: ReportRenderOptions) -> Self {
        Self { options }
    }
}

impl<E> ReportRenderer<E> for Json
where
    E: Error + Display + 'static,
{
    fn render(&self, report: &Report<E>, f: &mut Formatter<'_>) -> fmt::Result {
        render_json(report, self.options, f)
    }
}

fn render_json<E>(
    report: &Report<E>,
    options: ReportRenderOptions,
    f: &mut Formatter<'_>,
) -> fmt::Result
where
    E: Error + Display + 'static,
{
    let pretty = options.json_pretty;
    let mut first = true;
    let dispatched = dispatch_attachments(report.attachments());

    f.write_char('{')?;
    write_object_field(f, pretty, 0, &mut first, "schema_version", |f| {
        write_json_string(f, REPORT_JSON_SCHEMA_VERSION)
    })?;
    write_object_field(f, pretty, 0, &mut first, "error", |f| {
        write_error_object(f, pretty, 1, report.inner())
    })?;
    write_object_field(f, pretty, 0, &mut first, "metadata", |f| {
        write_metadata_object(f, pretty, 1, report, options)
    })?;
    #[cfg(feature = "trace")]
    if report.trace().is_some() {
        write_object_field(f, pretty, 0, &mut first, "trace", |f| {
            write_trace_object(f, pretty, 1, report)
        })?;
    }
    write_object_field(f, pretty, 0, &mut first, "context", |f| {
        write_context_array(f, pretty, 1, &dispatched.contexts)
    })?;
    write_object_field(f, pretty, 0, &mut first, "attachments", |f| {
        write_attachments_array(f, pretty, 1, &dispatched.notes, &dispatched.payloads)
    })?;

    if pretty && !first {
        f.write_char('\n')?;
        write_indent(f, 0)?;
    }
    f.write_char('}')
}

fn write_error_object<E>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    error: &E,
) -> fmt::Result
where
    E: Display,
{
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "message", |f| {
        write_json_string_from_display(f, error)
    })?;
    write_object_field(f, pretty, depth, &mut first, "type", |f| {
        write_json_string(f, core::any::type_name::<E>())
    })?;
    close_object(f, pretty, depth, first)
}

fn write_metadata_object<E>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<E>,
    options: ReportRenderOptions,
) -> fmt::Result
where
    E: Error + Display + 'static,
{
    let metadata = report.metadata();
    let mut first = true;
    f.write_char('{')?;
    write_object_field(
        f,
        pretty,
        depth,
        &mut first,
        "error_code",
        |f| match metadata.error_code.as_ref() {
            Some(code) => write_error_code(f, code),
            None => f.write_str("null"),
        },
    )?;
    write_object_field(
        f,
        pretty,
        depth,
        &mut first,
        "severity",
        |f| match metadata.severity {
            Some(severity) => write_json_string_from_display(f, &severity),
            None => f.write_str("null"),
        },
    )?;
    write_object_field(f, pretty, depth, &mut first, "category", |f| match metadata
        .category
        .as_ref()
    {
        Some(category) => write_json_string(f, category.as_ref()),
        None => f.write_str("null"),
    })?;
    write_object_field(
        f,
        pretty,
        depth,
        &mut first,
        "retryable",
        |f| match metadata.retryable {
            Some(retryable) => write!(f, "{retryable}"),
            None => f.write_str("null"),
        },
    )?;
    write_object_field(
        f,
        pretty,
        depth,
        &mut first,
        "stack_trace",
        |f| match metadata.stack_trace.as_ref() {
            Some(stack_trace) => write_stack_trace_object(f, pretty, depth + 1, stack_trace),
            None => f.write_str("null"),
        },
    )?;
    write_object_field(f, pretty, depth, &mut first, "display_causes", |f| {
        write_display_cause_chain_object(f, pretty, depth + 1, report, options)
    })?;
    write_object_field(f, pretty, depth, &mut first, "source_errors", |f| {
        write_source_error_chain_object(f, pretty, depth + 1, report, options)
    })?;
    close_object(f, pretty, depth, first)
}

#[cfg(feature = "trace")]
fn write_trace_object<E>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<E>,
) -> fmt::Result
where
    E: Error + Display + 'static,
{
    let Some(trace) = report.trace() else {
        return f.write_str("null");
    };

    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "context", |f| {
        write_trace_context_object(f, pretty, depth + 1, &trace.context)
    })?;
    write_object_field(f, pretty, depth, &mut first, "events", |f| {
        write_trace_events_array(f, pretty, depth + 1, &trace.events)
    })?;
    close_object(f, pretty, depth, first)
}

#[cfg(feature = "trace")]
fn write_trace_context_object(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    context: &TraceContext,
) -> fmt::Result {
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "trace_id", |f| {
        write_option_string(f, context.trace_id.as_deref())
    })?;
    write_object_field(f, pretty, depth, &mut first, "span_id", |f| {
        write_option_string(f, context.span_id.as_deref())
    })?;
    write_object_field(f, pretty, depth, &mut first, "parent_span_id", |f| {
        write_option_string(f, context.parent_span_id.as_deref())
    })?;
    write_object_field(f, pretty, depth, &mut first, "sampled", |f| {
        match context.sampled {
            Some(v) => write!(f, "{v}"),
            None => f.write_str("null"),
        }
    })?;
    write_object_field(f, pretty, depth, &mut first, "trace_state", |f| {
        write_option_string(f, context.trace_state.as_deref())
    })?;
    write_object_field(f, pretty, depth, &mut first, "flags", |f| {
        match context.flags {
            Some(v) => write!(f, "{v}"),
            None => f.write_str("null"),
        }
    })?;
    close_object(f, pretty, depth, first)
}

#[cfg(feature = "trace")]
fn write_trace_events_array(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    events: &[TraceEvent],
) -> fmt::Result {
    let mut first = true;
    f.write_char('[')?;
    for event in events {
        write_array_item_prefix(f, pretty, depth, &mut first)?;
        write_trace_event_object(f, pretty, depth + 1, event)?;
    }
    close_array(f, pretty, depth, first)
}

#[cfg(feature = "trace")]
fn write_trace_event_object(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    event: &TraceEvent,
) -> fmt::Result {
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "name", |f| {
        write_json_string(f, event.name.as_ref())
    })?;
    write_object_field(f, pretty, depth, &mut first, "level", |f| {
        match event.level {
            Some(level) => write_json_string_from_display(f, &level),
            None => f.write_str("null"),
        }
    })?;
    write_object_field(
        f,
        pretty,
        depth,
        &mut first,
        "timestamp_unix_nano",
        |f| match event.timestamp_unix_nano {
            Some(v) => write!(f, "{v}"),
            None => f.write_str("null"),
        },
    )?;
    write_object_field(f, pretty, depth, &mut first, "attributes", |f| {
        write_trace_attributes_array(f, pretty, depth + 1, &event.attributes)
    })?;
    close_object(f, pretty, depth, first)
}

#[cfg(feature = "trace")]
fn write_trace_attributes_array(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    attributes: &[TraceEventAttribute],
) -> fmt::Result {
    let mut first = true;
    f.write_char('[')?;
    for attr in attributes {
        write_array_item_prefix(f, pretty, depth, &mut first)?;
        write_object_with_key_value(f, pretty, depth + 1, &attr.key, &attr.value)?;
    }
    close_array(f, pretty, depth, first)
}

fn write_context_array(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    contexts: &[(&alloc::borrow::Cow<'static, str>, &AttachmentValue)],
) -> fmt::Result {
    let mut first = true;
    f.write_char('[')?;
    for (key, value) in contexts {
        write_array_item_prefix(f, pretty, depth, &mut first)?;
        write_object_with_key_value(f, pretty, depth + 1, key.as_ref(), value)?;
    }
    close_array(f, pretty, depth, first)
}

fn write_attachments_array(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    notes: &[&alloc::borrow::Cow<'static, str>],
    payloads: &[AttachmentPayloadRef<'_>],
) -> fmt::Result {
    let mut first = true;
    f.write_char('[')?;
    for message in notes {
        write_array_item_prefix(f, pretty, depth, &mut first)?;
        write_note_attachment_object(f, pretty, depth + 1, message.as_ref())?;
    }
    for payload in payloads {
        write_array_item_prefix(f, pretty, depth, &mut first)?;
        write_payload_attachment_object(
            f,
            pretty,
            depth + 1,
            payload.name.as_ref(),
            payload.value,
            payload.media_type.map(|m| m.as_ref()),
        )?;
    }
    close_array(f, pretty, depth, first)
}

fn write_note_attachment_object(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    message: &str,
) -> fmt::Result {
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

fn write_payload_attachment_object(
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

fn write_object_with_key_value(
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

fn write_display_cause_chain_object(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<impl Error + Display + 'static>,
    options: ReportRenderOptions,
) -> fmt::Result {
    let display_causes = report.display_causes();
    if display_causes.is_empty() {
        return f.write_str("null");
    }
    let item_count = core::cmp::min(display_causes.len(), options.max_source_depth);
    let truncated = display_causes.len() > options.max_source_depth;

    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "items", |f| {
        let mut array_first = true;
        f.write_char('[')?;
        for cause in display_causes.iter().take(item_count) {
            write_array_item_prefix(f, pretty, depth + 1, &mut array_first)?;
            write_json_string_from_display(f, cause.as_ref())?;
        }
        close_array(f, pretty, depth + 1, array_first)
    })?;
    write_object_field(f, pretty, depth, &mut first, "truncated", |f| {
        write!(f, "{truncated}")
    })?;
    write_object_field(f, pretty, depth, &mut first, "cycle_detected", |f| {
        f.write_str("false")
    })?;
    close_object(f, pretty, depth, first)
}

fn write_source_error_chain_object(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<impl Error + Display + 'static>,
    options: ReportRenderOptions,
) -> fmt::Result {
    if report.source_errors().is_empty() && report.inner().source().is_none() {
        return f.write_str("null");
    }

    let traversal_options = CauseCollectOptions {
        max_depth: options.max_source_depth,
        detect_cycle: options.detect_source_cycle,
    };
    let mut traversal = report.iter_source_errors_with(traversal_options);

    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "items", |f| {
        let mut array_first = true;
        f.write_char('[')?;
        for err in traversal.by_ref() {
            write_array_item_prefix(f, pretty, depth + 1, &mut array_first)?;
            write_source_error_object(f, pretty, depth + 2, err)?;
        }
        close_array(f, pretty, depth + 1, array_first)
    })?;
    let traversal_state = traversal.state();
    write_object_field(f, pretty, depth, &mut first, "truncated", |f| {
        write!(f, "{}", traversal_state.truncated)
    })?;
    write_object_field(f, pretty, depth, &mut first, "cycle_detected", |f| {
        write!(f, "{}", traversal_state.cycle_detected)
    })?;
    close_object(f, pretty, depth, first)
}

fn write_source_error_object(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    err: &dyn Error,
) -> fmt::Result {
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "message", |f| {
        write_json_string_from_display(f, err)
    })?;
    close_object(f, pretty, depth, first)
}

fn write_stack_trace_object(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    stack_trace: &StackTrace,
) -> fmt::Result {
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "format", |f| {
        let label = match stack_trace.format {
            crate::report::StackTraceFormat::Native => "native",
            crate::report::StackTraceFormat::Raw => "raw",
        };
        write_json_string(f, label)
    })?;
    write_object_field(f, pretty, depth, &mut first, "frames", |f| {
        let mut array_first = true;
        f.write_char('[')?;
        for frame in &stack_trace.frames {
            write_array_item_prefix(f, pretty, depth + 1, &mut array_first)?;
            write_stack_frame_object(f, pretty, depth + 2, frame)?;
        }
        close_array(f, pretty, depth + 1, array_first)
    })?;
    write_object_field(f, pretty, depth, &mut first, "raw", |f| {
        match stack_trace.raw.as_ref() {
            Some(raw) => write_json_string(f, raw),
            None => f.write_str("null"),
        }
    })?;
    close_object(f, pretty, depth, first)
}

fn write_stack_frame_object(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    frame: &crate::report::StackFrame,
) -> fmt::Result {
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "symbol", |f| {
        write_option_string(f, frame.symbol.as_ref().map(|s| s.as_str()))
    })?;
    write_object_field(f, pretty, depth, &mut first, "module_path", |f| {
        write_option_string(f, frame.module_path.as_ref().map(|s| s.as_str()))
    })?;
    write_object_field(f, pretty, depth, &mut first, "file", |f| {
        write_option_string(f, frame.file.as_ref().map(|s| s.as_str()))
    })?;
    write_object_field(f, pretty, depth, &mut first, "line", |f| match frame.line {
        Some(v) => write!(f, "{v}"),
        None => f.write_str("null"),
    })?;
    write_object_field(f, pretty, depth, &mut first, "column", |f| {
        match frame.column {
            Some(v) => write!(f, "{v}"),
            None => f.write_str("null"),
        }
    })?;
    close_object(f, pretty, depth, first)
}

fn write_attachment_value(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    value: &AttachmentValue,
) -> fmt::Result {
    if let Some(result) = write_scalar_attachment_value(f, pretty, depth, value) {
        return result;
    }

    match value {
        AttachmentValue::Null
        | AttachmentValue::String(_)
        | AttachmentValue::Integer(_)
        | AttachmentValue::Unsigned(_)
        | AttachmentValue::Float(_)
        | AttachmentValue::Bool(_) => unreachable!("handled in scalar fast path"),
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
            let mut first = true;
            f.write_char('{')?;
            write_object_field(f, pretty, depth, &mut first, "kind", |f| {
                write_json_string(f, "redacted")
            })?;
            write_object_field(f, pretty, depth, &mut first, "value", |f| {
                let mut inner_first = true;
                f.write_char('{')?;
                write_object_field(f, pretty, depth + 1, &mut inner_first, "kind", |f| {
                    write_option_string(f, kind.as_deref())
                })?;
                write_object_field(f, pretty, depth + 1, &mut inner_first, "reason", |f| {
                    write_option_string(f, reason.as_deref())
                })?;
                close_object(f, pretty, depth + 1, inner_first)
            })?;
            close_object(f, pretty, depth, first)
        }
    }
}

fn write_scalar_attachment_value(
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
        AttachmentValue::Integer(v) => Some(write_kind_and_value(f, pretty, depth, "integer", |f| {
            write!(f, "{v}")
        })),
        AttachmentValue::Unsigned(v) => Some(write_kind_and_value(f, pretty, depth, "unsigned", |f| {
            write!(f, "{v}")
        })),
        AttachmentValue::Float(v) => {
            if !v.is_finite() {
                Some(Err(fmt::Error))
            } else {
                Some(write_kind_and_value(f, pretty, depth, "float", |f| write!(f, "{v}")))
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

fn write_error_code(f: &mut Formatter<'_>, code: &ErrorCode) -> fmt::Result {
    match code {
        ErrorCode::Integer(v) => write!(f, "{v}"),
        ErrorCode::String(v) => write_json_string(f, v),
    }
}

fn write_option_string(f: &mut Formatter<'_>, value: Option<&str>) -> fmt::Result {
    match value {
        Some(v) => write_json_string(f, v),
        None => f.write_str("null"),
    }
}

fn write_json_string_from_display(
    f: &mut Formatter<'_>,
    value: &(impl Display + ?Sized),
) -> fmt::Result {
    f.write_char('"')?;
    {
        let mut escaper = JsonStringEscaper { out: f };
        write!(&mut escaper, "{value}")?;
    }
    f.write_char('"')
}

fn write_json_string(f: &mut Formatter<'_>, value: impl AsRef<str>) -> fmt::Result {
    f.write_char('"')?;
    {
        let mut escaper = JsonStringEscaper { out: f };
        escaper.write_str(value.as_ref())?;
    }
    f.write_char('"')
}

struct JsonStringEscaper<'a, 'b> {
    out: &'a mut Formatter<'b>,
}

impl Write for JsonStringEscaper<'_, '_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for ch in s.chars() {
            match ch {
                '"' => self.out.write_str("\\\"")?,
                '\\' => self.out.write_str("\\\\")?,
                '\n' => self.out.write_str("\\n")?,
                '\r' => self.out.write_str("\\r")?,
                '\t' => self.out.write_str("\\t")?,
                '\u{08}' => self.out.write_str("\\b")?,
                '\u{0C}' => self.out.write_str("\\f")?,
                c if c <= '\u{1F}' => write!(self.out, "\\u{:04X}", c as u32)?,
                c => self.out.write_char(c)?,
            }
        }
        Ok(())
    }
}

fn write_object_field<F>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    first: &mut bool,
    key: &str,
    mut write_value: F,
) -> fmt::Result
where
    F: FnMut(&mut Formatter<'_>) -> fmt::Result,
{
    if *first {
        *first = false;
    } else {
        f.write_char(',')?;
    }
    if pretty {
        f.write_char('\n')?;
        write_indent(f, depth + 1)?;
    }
    write_json_string(f, key)?;
    f.write_char(':')?;
    if pretty {
        f.write_char(' ')?;
    }
    write_value(f)
}

fn write_array_item_prefix(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    first: &mut bool,
) -> fmt::Result {
    if *first {
        *first = false;
    } else {
        f.write_char(',')?;
    }
    if pretty {
        f.write_char('\n')?;
        write_indent(f, depth + 1)?;
    }
    Ok(())
}

fn close_object(f: &mut Formatter<'_>, pretty: bool, depth: usize, empty: bool) -> fmt::Result {
    if pretty && !empty {
        f.write_char('\n')?;
        write_indent(f, depth)?;
    }
    f.write_char('}')
}

fn close_array(f: &mut Formatter<'_>, pretty: bool, depth: usize, empty: bool) -> fmt::Result {
    if pretty && !empty {
        f.write_char('\n')?;
        write_indent(f, depth)?;
    }
    f.write_char(']')
}

fn write_indent(f: &mut Formatter<'_>, depth: usize) -> fmt::Result {
    for _ in 0..depth {
        f.write_str("  ")?;
    }
    Ok(())
}
