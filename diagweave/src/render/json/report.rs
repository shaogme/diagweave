use core::error::Error;
use core::fmt::{self, Display, Formatter, Write};

use crate::report::{CauseCollectOptions, Report, SeverityState, StackTrace};

#[cfg(feature = "trace")]
use super::attachment;
#[cfg(feature = "trace")]
use super::trace::{
    TraceAttributeValue, TraceContextValue, TraceEventValue, TraceSectionValue,
    build_trace_section_value,
};
use super::{
    ReportRenderOptions, close_array, close_object, filtered_frames, write_array_item_prefix,
    write_error_code, write_json_display, write_json_string, write_object_field,
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

pub(super) fn write_metadata_object<E, State>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<E, State>,
) -> fmt::Result
where
    E: Error,
    State: SeverityState,
{
    let mut first = true;
    f.write_char('{')?;
    write_meta_gov_fields(f, pretty, depth, &mut first, report)?;
    close_object(f, pretty, depth, first)
}

pub(super) fn write_diag_bag<E, State>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<E, State>,
    options: ReportRenderOptions,
) -> fmt::Result
where
    E: Error,
    State: SeverityState,
{
    let mut first = true;
    f.write_char('{')?;
    write_diag_stack(f, pretty, depth, report, options, &mut first)?;
    write_diag_display_causes(f, pretty, depth, report, options, &mut first)?;
    write_diag_origin_sources(f, pretty, depth, report, options, &mut first)?;
    write_diag_extra_sources(f, pretty, depth, report, options, &mut first)?;
    close_object(f, pretty, depth, first)
}

fn write_diag_stack<E, State>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<E, State>,
    options: ReportRenderOptions,
    first: &mut bool,
) -> fmt::Result
where
    E: Error,
    State: SeverityState,
{
    if !options.show_stack_trace_section
        || (!options.show_empty_sections && report.stack_trace().is_none())
    {
        return Ok(());
    }
    let Some(stack_trace) = report.stack_trace() else {
        return Ok(());
    };
    write_object_field(f, pretty, depth, first, "stack_trace", |f| {
        write_stack_trace_object(f, pretty, depth + 1, stack_trace, options)
    })
}

fn write_diag_display_causes<E, State>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<E, State>,
    options: ReportRenderOptions,
    first: &mut bool,
) -> fmt::Result
where
    E: Error,
    State: SeverityState,
{
    if !options.show_cause_chains_section
        || (!options.show_empty_sections && !has_display_causes(report))
    {
        return Ok(());
    }
    let Some(display_causes) = report.display_causes_chain() else {
        return Ok(());
    };
    write_object_field(f, pretty, depth, first, "display_causes", |f| {
        write_display_causes(f, pretty, depth + 1, report, display_causes, options)
    })
}

fn write_diag_origin_sources<E, State>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<E, State>,
    options: ReportRenderOptions,
    first: &mut bool,
) -> fmt::Result
where
    E: Error,
    State: SeverityState,
{
    if !options.show_cause_chains_section
        || (!options.show_empty_sections && !has_origin_source_errors(report))
    {
        return Ok(());
    }
    let traversal_options = CauseCollectOptions {
        max_depth: options.max_source_depth,
        detect_cycle: options.detect_source_cycle,
    };
    let Some(source_errors) = report.origin_src_err_view(traversal_options) else {
        return Ok(());
    };
    write_object_field(f, pretty, depth, first, "origin_source_errors", |f| {
        write_source_errors_chain(f, pretty, depth + 1, &source_errors, true)
    })
}

fn write_diag_extra_sources<E, State>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<E, State>,
    options: ReportRenderOptions,
    first: &mut bool,
) -> fmt::Result
where
    E: Error,
    State: SeverityState,
{
    if !options.show_cause_chains_section
        || (!options.show_empty_sections && !has_diag_source_errors(report))
    {
        return Ok(());
    }
    let traversal_options = CauseCollectOptions {
        max_depth: options.max_source_depth,
        detect_cycle: options.detect_source_cycle,
    };
    let Some(source_errors) = report.diag_src_err_view(traversal_options) else {
        return Ok(());
    };
    write_object_field(f, pretty, depth, first, "diagnostic_source_errors", |f| {
        write_source_errors_chain(f, pretty, depth + 1, &source_errors, false)
    })
}

fn has_display_causes<E, State>(report: &Report<E, State>) -> bool
where
    E: Error,
    State: SeverityState,
{
    report.display_causes_chain().is_some()
}

fn has_origin_source_errors<E, State>(report: &Report<E, State>) -> bool
where
    E: Error,
    State: SeverityState,
{
    report.origin_src_err_chain().is_some() || report.inner().source().is_some()
}

fn has_diag_source_errors<E, State>(report: &Report<E, State>) -> bool
where
    E: Error,
    State: SeverityState,
{
    report.diag_src_err_chain().is_some()
}

fn write_meta_gov_fields<E, State>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    first: &mut bool,
    report: &Report<E, State>,
) -> fmt::Result
where
    E: Error,
    State: SeverityState,
{
    let metadata = report.metadata();
    if let Some(code) = metadata.error_code() {
        write_object_field(f, pretty, depth, first, "error_code", |f| {
            write_error_code(f, code)
        })?;
    }
    if let Some(severity) = report.severity() {
        write_object_field(f, pretty, depth, first, "severity", |f| {
            write_json_display(f, &severity)
        })?;
    }
    if let Some(category) = metadata.category() {
        write_object_field(f, pretty, depth, first, "category", |f| {
            write_json_string(f, category)
        })?;
    }
    if let Some(retryable) = metadata.retryable() {
        write_object_field(f, pretty, depth, first, "retryable", |f| {
            write!(f, "{retryable}")
        })?;
    }
    Ok(())
}

fn write_display_causes<State>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<impl Error, State>,
    display_causes: &crate::report::DisplayCauseChain,
    options: ReportRenderOptions,
) -> fmt::Result
where
    State: SeverityState,
{
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

fn write_source_error_object(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    message: &str,
    type_name: Option<&str>,
    source_roots: &[usize],
) -> fmt::Result {
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "message", |f| {
        write_json_string(f, message)
    })?;
    if let Some(type_name) = type_name {
        write_object_field(f, pretty, depth, &mut first, "type", |f| {
            write_json_string(f, type_name)
        })?;
    }
    write_object_field(f, pretty, depth, &mut first, "source_roots", |f| {
        let mut array_first = true;
        f.write_char('[')?;
        for &source_id in source_roots {
            write_array_item_prefix(f, pretty, depth + 1, &mut array_first)?;
            write!(f, "{source_id}")?;
        }
        close_array(f, pretty, depth + 1, array_first)
    })?;
    close_object(f, pretty, depth, first)
}

fn write_source_errors_chain(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    source_errors: &crate::report::SourceErrorChain,
    hide_report_wrapper_types: bool,
) -> fmt::Result {
    let exported = source_errors.export_with_options(hide_report_wrapper_types);

    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "roots", |f| {
        let mut array_first = true;
        f.write_char('[')?;
        for &node_id in exported.roots.as_slice() {
            write_array_item_prefix(f, pretty, depth + 1, &mut array_first)?;
            write!(f, "{node_id}")?;
        }
        close_array(f, pretty, depth + 1, array_first)
    })?;
    write_object_field(f, pretty, depth, &mut first, "nodes", |f| {
        let mut array_first = true;
        f.write_char('[')?;
        for node in exported.nodes.iter() {
            write_array_item_prefix(f, pretty, depth + 1, &mut array_first)?;
            write_source_error_object(
                f,
                pretty,
                depth + 2,
                &node.message,
                node.type_name.as_deref(),
                node.source_roots.as_slice(),
            )?;
        }
        close_array(f, pretty, depth + 1, array_first)
    })?;
    write_object_field(f, pretty, depth, &mut first, "truncated", |f| {
        write!(f, "{}", exported.truncated)
    })?;
    write_object_field(f, pretty, depth, &mut first, "cycle_detected", |f| {
        write!(f, "{}", exported.cycle_detected)
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
    if let Some(trace_id) = context.trace_id.as_deref() {
        write_object_field(f, pretty, depth, &mut first, "trace_id", |f| {
            write_json_string(f, trace_id)
        })?;
    }
    if let Some(span_id) = context.span_id.as_deref() {
        write_object_field(f, pretty, depth, &mut first, "span_id", |f| {
            write_json_string(f, span_id)
        })?;
    }
    if let Some(parent_span_id) = context.parent_span_id.as_deref() {
        write_object_field(f, pretty, depth, &mut first, "parent_span_id", |f| {
            write_json_string(f, parent_span_id)
        })?;
    }
    if let Some(v) = context.sampled {
        write_object_field(f, pretty, depth, &mut first, "sampled", |f| {
            write!(f, "{v}")
        })?;
    }
    if let Some(trace_state) = context.trace_state.as_deref() {
        write_object_field(f, pretty, depth, &mut first, "trace_state", |f| {
            write_json_string(f, trace_state)
        })?;
    }
    if let Some(v) = context.flags {
        write_object_field(f, pretty, depth, &mut first, "flags", |f| write!(f, "{v}"))?;
    }
    close_object(f, pretty, depth, first)
}

#[cfg(feature = "trace")]
pub(super) fn write_trace_object<E, State>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    report: &Report<E, State>,
    options: ReportRenderOptions,
) -> fmt::Result
where
    E: Error,
    State: SeverityState,
{
    let trace = report.trace();
    if trace.is_empty() {
        // When trace is absent, write empty object to ensure valid JSON
        f.write_str("{}")?;
        return Ok(());
    }
    write_trace_section_value(f, pretty, depth, &build_trace_section_value(trace), options)
}

#[cfg(feature = "trace")]
fn write_trace_section_value(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    value: &TraceSectionValue,
    options: ReportRenderOptions,
) -> fmt::Result {
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "context", |f| {
        write_trace_context_value(f, pretty, depth + 1, &value.context)
    })?;
    write_object_field(f, pretty, depth, &mut first, "events", |f| {
        write_trace_events_array(f, pretty, depth + 1, &value.events, options)
    })?;
    close_object(f, pretty, depth, first)
}

#[cfg(feature = "trace")]
fn write_trace_events_array(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    events: &[TraceEventValue],
    options: ReportRenderOptions,
) -> fmt::Result {
    let mut first = true;
    f.write_char('[')?;
    for event in events {
        write_array_item_prefix(f, pretty, depth, &mut first)?;
        write_trace_event_value(f, pretty, depth + 1, event, options)?;
    }
    close_array(f, pretty, depth, first)
}

#[cfg(feature = "trace")]
fn write_trace_event_value(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    event: &TraceEventValue,
    options: ReportRenderOptions,
) -> fmt::Result {
    let mut first = true;
    f.write_char('{')?;
    write_object_field(f, pretty, depth, &mut first, "name", |f| {
        write_json_string(f, event.name.as_ref())
    })?;
    if options.show_trace_event_details {
        if let Some(level) = event.level.as_deref() {
            write_object_field(f, pretty, depth, &mut first, "level", |f| {
                write_json_string(f, level)
            })?;
        }
        if let Some(v) = event.timestamp_unix_nano {
            write_object_field(f, pretty, depth, &mut first, "timestamp_unix_nano", |f| {
                write!(f, "{v}")
            })?;
        }
        if !event.attributes.is_empty() {
            write_object_field(f, pretty, depth, &mut first, "attributes", |f| {
                write_trace_attr_array(f, pretty, depth + 1, &event.attributes)
            })?;
        }
    }
    close_object(f, pretty, depth, first)
}

#[cfg(feature = "trace")]
fn write_trace_attr_array(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    attrs: &[TraceAttributeValue],
) -> fmt::Result {
    let mut first = true;
    f.write_char('[')?;
    for attr in attrs {
        write_array_item_prefix(f, pretty, depth, &mut first)?;
        write_trace_attr_value(f, pretty, depth + 1, attr)?;
    }
    close_array(f, pretty, depth, first)
}

#[cfg(feature = "trace")]
fn write_trace_attr_value(
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
    options: ReportRenderOptions,
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
    match stack_trace.format {
        crate::report::StackTraceFormat::Native => {
            // Native format: output frames, no raw
            write_object_field(f, pretty, depth, &mut first, "frames", |f| {
                let mut array_first = true;
                f.write_char('[')?;
                for (_, frame) in filtered_frames(&stack_trace.frames, &options.stack_trace_filter)
                    .take(options.stack_trace_max_lines)
                {
                    write_array_item_prefix(f, pretty, depth + 1, &mut array_first)?;
                    write_stack_frame_object(f, pretty, depth + 2, frame)?;
                }
                close_array(f, pretty, depth + 1, array_first)
            })?;
        }
        crate::report::StackTraceFormat::Raw => {
            // Raw format: output raw, no frames
            if let Some(raw) = stack_trace.raw.as_ref() {
                write_object_field(f, pretty, depth, &mut first, "raw", |f| {
                    write_json_string(f, raw)
                })?;
            }
        }
    }
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
    if let Some(symbol) = frame.symbol.as_deref() {
        write_object_field(f, pretty, depth, &mut first, "symbol", |f| {
            write_json_string(f, symbol)
        })?;
    }
    if let Some(module_path) = frame.module_path.as_deref() {
        write_object_field(f, pretty, depth, &mut first, "module_path", |f| {
            write_json_string(f, module_path)
        })?;
    }
    if let Some(file) = frame.file.as_deref() {
        write_object_field(f, pretty, depth, &mut first, "file", |f| {
            write_json_string(f, file)
        })?;
    }
    if let Some(v) = frame.line {
        write_object_field(f, pretty, depth, &mut first, "line", |f| write!(f, "{v}"))?;
    }
    if let Some(v) = frame.column {
        write_object_field(f, pretty, depth, &mut first, "column", |f| write!(f, "{v}"))?;
    }
    close_object(f, pretty, depth, first)
}
