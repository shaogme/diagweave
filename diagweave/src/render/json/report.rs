use core::error::Error;
use core::fmt::{self, Display, Formatter, Write};

use crate::report::{CauseCollectOptions, Report, StackTrace};

#[cfg(feature = "trace")]
use super::attachment;
#[cfg(feature = "trace")]
use super::trace::{
    TraceAttributeValue, TraceContextValue, TraceEventValue, TraceSectionValue,
    build_trace_section_value,
};
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
) -> fmt::Result
where
    E: Error + Display + 'static,
{
    let metadata = report.metadata();
    let mut first = true;
    f.write_char('{')?;
    write_meta_gov_fields(f, pretty, depth, &mut first, metadata)?;
    close_object(f, pretty, depth, first)
}

pub(super) fn write_diag_bag<E>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<E>,
    options: ReportRenderOptions,
) -> fmt::Result
where
    E: Error + Display + 'static,
{
    let mut first = true;
    f.write_char('{')?;
    if options.show_stack_trace_section
        && (options.show_empty_sections || report.stack_trace().is_some())
    {
        write_object_field(
            f,
            pretty,
            depth,
            &mut first,
            "stack_trace",
            |f| match report.stack_trace() {
                Some(stack_trace) => write_stack_trace_object(f, pretty, depth + 1, stack_trace),
                None => f.write_str("null"),
            },
        )?;
    }
    if options.show_cause_chains_section
        && (options.show_empty_sections || has_display_causes(report))
    {
        write_object_field(f, pretty, depth, &mut first, "display_causes", |f| {
            write_display_causes(f, pretty, depth + 1, report, options)
        })?;
    }
    if options.show_cause_chains_section
        && (options.show_empty_sections || has_source_errors(report))
    {
        write_object_field(f, pretty, depth, &mut first, "source_errors", |f| {
            write_source_errors(f, pretty, depth + 1, report, options)
        })?;
    }
    close_object(f, pretty, depth, first)
}

fn has_display_causes<E>(report: &Report<E>) -> bool
where
    E: Error + Display + 'static,
{
    report.display_causes_chain().is_some()
}

fn has_source_errors<E>(report: &Report<E>) -> bool
where
    E: Error + Display + 'static,
{
    report.source_errors_chain().is_some() || report.inner().source().is_some()
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

fn write_display_causes(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<impl Error + 'static>,
    options: ReportRenderOptions,
) -> fmt::Result {
    let Some(display_causes) = report.display_causes_chain() else {
        return f.write_str("null");
    };

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
        write!(
            f,
            "{}",
            display_causes.truncated || traversal_state.truncated
        )
    })?;
    write_object_field(f, pretty, depth, &mut first, "cycle_detected", |f| {
        write!(
            f,
            "{}",
            display_causes.cycle_detected || traversal_state.cycle_detected
        )
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
    let traversal_options = CauseCollectOptions {
        max_depth: options.max_source_depth,
        detect_cycle: options.detect_source_cycle,
    };
    let Some(source_errors) = report.source_errors_snapshot(traversal_options) else {
        return f.write_str("null");
    };
    write_source_errors_chain(f, pretty, depth, &source_errors)
}

fn write_source_error_object(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    item: &crate::report::SourceErrorItem,
) -> fmt::Result {
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "message", |f| {
        write_json_display(f, item.error.as_ref())
    })?;
    write_object_field(f, pretty, depth, &mut first, "type", |f| {
        match item.display_type_name() {
            Some(type_name) => write_json_string(f, type_name),
            None => f.write_str("null"),
        }
    })?;
    if let Some(source) = item.source.as_ref() {
        write_object_field(f, pretty, depth, &mut first, "source", |f| {
            write_source_errors_chain(f, pretty, depth + 1, source)
        })?;
    }
    close_object(f, pretty, depth, first)
}

fn write_source_errors_chain(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    source_errors: &crate::report::SourceErrorChain,
) -> fmt::Result {
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "items", |f| {
        let mut array_first = true;
        f.write_char('[')?;
        for item in &source_errors.items {
            write_array_item_prefix(f, pretty, depth + 1, &mut array_first)?;
            write_source_error_object(f, pretty, depth + 2, item)?;
        }
        close_array(f, pretty, depth + 1, array_first)
    })?;
    write_object_field(f, pretty, depth, &mut first, "truncated", |f| {
        write!(f, "{}", source_errors.truncated)
    })?;
    write_object_field(f, pretty, depth, &mut first, "cycle_detected", |f| {
        write!(f, "{}", source_errors.cycle_detected)
    })?;
    close_object(f, pretty, depth, first)
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
    write_trace_section_value(f, pretty, depth, &build_trace_section_value(trace))
}

#[cfg(feature = "trace")]
fn write_trace_section_value(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    value: &TraceSectionValue,
) -> fmt::Result {
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "context", |f| {
        write_trace_context_value(f, pretty, depth + 1, &value.context)
    })?;
    write_object_field(f, pretty, depth, &mut first, "events", |f| {
        write_trace_events_array(f, pretty, depth + 1, &value.events)
    })?;
    close_object(f, pretty, depth, first)
}

#[cfg(feature = "trace")]
fn write_trace_context_value(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    context: &TraceContextValue,
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
    events: &[TraceEventValue],
) -> fmt::Result {
    let mut first = true;
    f.write_char('[')?;
    for event in events {
        write_array_item_prefix(f, pretty, depth, &mut first)?;
        write_trace_event_value(f, pretty, depth + 1, event)?;
    }
    close_array(f, pretty, depth, first)
}

#[cfg(feature = "trace")]
fn write_trace_event_value(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    event: &TraceEventValue,
) -> fmt::Result {
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "name", |f| {
        write_json_string(f, event.name.as_ref())
    })?;
    write_object_field(f, pretty, depth, &mut first, "level", |f| {
        write_option_string(f, event.level.as_deref())
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
        write_trace_attributes(f, pretty, depth + 1, &event.attributes)
    })?;
    close_object(f, pretty, depth, first)
}

#[cfg(feature = "trace")]
fn write_trace_attributes(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    attributes: &[TraceAttributeValue],
) -> fmt::Result {
    let mut first = true;
    f.write_char('[')?;
    for attr in attributes {
        write_array_item_prefix(f, pretty, depth, &mut first)?;
        write_trace_attribute_value(f, pretty, depth + 1, attr)?;
    }
    close_array(f, pretty, depth, first)
}

#[cfg(feature = "trace")]
fn write_trace_attribute_value(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    attr: &TraceAttributeValue,
) -> fmt::Result {
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "key", |f| {
        write_json_string(f, attr.key.as_ref())
    })?;
    write_object_field(f, pretty, depth, &mut first, "value", |f| {
        attachment::write_attachment_value(f, pretty, depth + 1, &attr.value)
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
