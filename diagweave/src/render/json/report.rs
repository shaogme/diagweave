use core::error::Error;
use core::fmt::{self, Display, Formatter, Write};

use crate::report::{CauseCollectOptions, Report, StackTrace};

#[cfg(feature = "trace")]
use crate::report::{TraceContext, TraceEvent, TraceEventAttribute};

#[cfg(feature = "trace")]
use super::attachment;
use super::{
    ReportRenderOptions, close_array, close_object, write_array_item_prefix, write_error_code,
    write_json_display, write_json_string, write_object_field, write_option_string,
};

pub(super) fn write_error_object<E>(
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
        write_json_display(f, error)
    })?;
    write_object_field(f, pretty, depth, &mut first, "type", |f| {
        write_json_string(f, core::any::type_name::<E>())
    })?;
    close_object(f, pretty, depth, first)
}

pub(super) fn write_metadata_object<E>(
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
    write_meta_gov_fields(f, pretty, depth, &mut first, metadata)?;
    write_meta_diag_fields(f, pretty, depth, &mut first, report, options)?;
    close_object(f, pretty, depth, first)
}

fn write_meta_gov_fields(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    first: &mut bool,
    metadata: &crate::report::ReportMetadata,
) -> fmt::Result {
    write_object_field(f, pretty, depth, first, "error_code", |f| {
        match metadata.error_code.as_ref() {
            Some(code) => write_error_code(f, code),
            None => f.write_str("null"),
        }
    })?;
    write_object_field(f, pretty, depth, first, "severity", |f| {
        match metadata.severity {
            Some(severity) => write_json_display(f, &severity),
            None => f.write_str("null"),
        }
    })?;
    write_object_field(f, pretty, depth, first, "category", |f| {
        match metadata.category.as_ref() {
            Some(category) => write_json_string(f, category.as_ref()),
            None => f.write_str("null"),
        }
    })?;
    write_object_field(f, pretty, depth, first, "retryable", |f| {
        match metadata.retryable {
            Some(retryable) => write!(f, "{retryable}"),
            None => f.write_str("null"),
        }
    })?;
    Ok(())
}

fn write_meta_diag_fields<E>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    first: &mut bool,
    report: &Report<E>,
    options: ReportRenderOptions,
) -> fmt::Result
where
    E: Error + Display + 'static,
{
    let metadata = report.metadata();
    write_object_field(f, pretty, depth, first, "stack_trace", |f| {
        match metadata.stack_trace.as_ref() {
            Some(stack_trace) => write_stack_trace_object(f, pretty, depth + 1, stack_trace),
            None => f.write_str("null"),
        }
    })?;
    write_object_field(f, pretty, depth, first, "display_causes", |f| {
        write_display_causes(f, pretty, depth + 1, report, options)
    })?;
    write_object_field(f, pretty, depth, first, "source_errors", |f| {
        write_source_errors(f, pretty, depth + 1, report, options)
    })?;
    Ok(())
}

#[cfg(feature = "trace")]
pub(super) fn write_trace_object<E>(
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
        write_trace_ctx(f, pretty, depth + 1, &trace.context)
    })?;
    write_object_field(f, pretty, depth, &mut first, "events", |f| {
        write_trace_events_array(f, pretty, depth + 1, &trace.events)
    })?;
    close_object(f, pretty, depth, first)
}

#[cfg(feature = "trace")]
fn write_trace_ctx(
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
            Some(level) => write_json_display(f, &level),
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
        write_trace_attrs(f, pretty, depth + 1, &event.attributes)
    })?;
    close_object(f, pretty, depth, first)
}

#[cfg(feature = "trace")]
fn write_trace_attrs(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    attributes: &[TraceEventAttribute],
) -> fmt::Result {
    let mut first = true;
    f.write_char('[')?;
    for attr in attributes {
        write_array_item_prefix(f, pretty, depth, &mut first)?;
        write_kv_obj(f, pretty, depth + 1, &attr.key, &attr.value)?;
    }
    close_array(f, pretty, depth, first)
}

#[cfg(feature = "trace")]
fn write_kv_obj(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    key: &str,
    value: &crate::report::AttachmentValue,
) -> fmt::Result {
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "key", |f| {
        write_json_string(f, key)
    })?;
    write_object_field(f, pretty, depth, &mut first, "value", |f| {
        attachment::write_attachment_value(f, pretty, depth + 1, value)
    })?;
    close_object(f, pretty, depth, first)
}

fn write_display_causes(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<impl Error + 'static>,
    options: ReportRenderOptions,
) -> fmt::Result {
    let display_causes = report.display_causes();
    if display_causes.is_empty() {
        return f.write_str("null");
    }
    let traversal_options = CauseCollectOptions {
        max_depth: options.max_source_depth,
        detect_cycle: options.detect_source_cycle,
    };
    let mut traversal_state = crate::report::CauseTraversalState::default();

    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "items", |f| {
        let mut array_first = true;
        f.write_char('[')?;
        traversal_state = report.visit_causes_ext(traversal_options, |cause| {
            write_array_item_prefix(f, pretty, depth + 1, &mut array_first)?;
            write_json_display(f, cause)
        })?;
        close_array(f, pretty, depth + 1, array_first)
    })?;
    write_object_field(f, pretty, depth, &mut first, "truncated", |f| {
        write!(f, "{}", traversal_state.truncated)
    })?;
    write_object_field(f, pretty, depth, &mut first, "cycle_detected", |f| {
        write!(f, "{}", traversal_state.cycle_detected)
    })?;
    close_object(f, pretty, depth, first)
}

fn write_source_errors(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<impl Error + 'static>,
    options: ReportRenderOptions,
) -> fmt::Result {
    if report.source_errors().is_empty() && report.inner().source().is_none() {
        return f.write_str("null");
    }

    let traversal_options = CauseCollectOptions {
        max_depth: options.max_source_depth,
        detect_cycle: options.detect_source_cycle,
    };
    let mut traversal_state = crate::report::CauseTraversalState::default();

    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "items", |f| {
        let mut array_first = true;
        f.write_char('[')?;
        traversal_state = report.visit_sources_ext(traversal_options, |err| {
            write_array_item_prefix(f, pretty, depth + 1, &mut array_first)?;
            write_source_error_object(f, pretty, depth + 2, err)
        })?;
        close_array(f, pretty, depth + 1, array_first)
    })?;
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
        write_json_display(f, err)
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
        write_option_string(f, frame.symbol.as_deref())
    })?;
    write_object_field(f, pretty, depth, &mut first, "module_path", |f| {
        write_option_string(f, frame.module_path.as_deref())
    })?;
    write_object_field(f, pretty, depth, &mut first, "file", |f| {
        write_option_string(f, frame.file.as_deref())
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
