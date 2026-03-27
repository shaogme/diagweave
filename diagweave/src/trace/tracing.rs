use tracing::{Level, event};

use alloc::vec::Vec;

use crate::render_impl::{
    DiagnosticIr, build_context_and_attachments, build_display_causes_value,
    build_source_errors_value, build_stack_trace_value,
};
use crate::report::{AttachmentValue, Severity, TraceEvent, TraceEventLevel};

use super::TracingExporterTrait;

/// Implementation of `TracingExporterTrait` that emits reports to the `tracing` system.
#[derive(Debug, Clone, Copy, Default)]
pub struct TracingExporter;

impl TracingExporterTrait for TracingExporter {
    fn export_ir(&self, ir: &DiagnosticIr) {
        let report_level = severity_to_level(ir.metadata.severity);
        let (context_items, attachment_items) = build_context_and_attachments(ir.attachments);
        let stack_trace_value = ir.metadata.stack_trace.map(build_stack_trace_value);
        let display_causes_value = build_display_causes_value(ir.display_causes);
        let source_errors_value = build_source_errors_value(ir.source_errors);

        emit_report_event(
            report_level,
            ir,
            &context_items,
            &attachment_items,
            stack_trace_value.as_ref(),
            &display_causes_value,
            &source_errors_value,
        );

        if let Some(trace) = ir.trace {
            for (idx, trace_event) in trace.events.iter().enumerate() {
                let trace_level = trace_level_to_tracing(trace_event.level).unwrap_or(report_level);
                emit_trace_event(
                    trace_level,
                    idx,
                    trace_event,
                    &context_items,
                    &attachment_items,
                    stack_trace_value.as_ref(),
                    &display_causes_value,
                    &source_errors_value,
                );
            }
        }
    }
}

fn severity_to_level(severity: Option<Severity>) -> Level {
    match severity {
        Some(Severity::Debug) => Level::DEBUG,
        Some(Severity::Info) => Level::INFO,
        Some(Severity::Warn) => Level::WARN,
        Some(Severity::Error) | Some(Severity::Fatal) => Level::ERROR,
        None => Level::ERROR,
    }
}

fn trace_level_to_tracing(level: Option<TraceEventLevel>) -> Option<Level> {
    match level {
        Some(TraceEventLevel::Trace) => Some(Level::TRACE),
        Some(TraceEventLevel::Debug) => Some(Level::DEBUG),
        Some(TraceEventLevel::Info) => Some(Level::INFO),
        Some(TraceEventLevel::Warn) => Some(Level::WARN),
        Some(TraceEventLevel::Error) => Some(Level::ERROR),
        None => None,
    }
}

macro_rules! report_event {
    ($level:expr, $ir:expr, $context:expr, $attachments:expr, $stack:expr, $display:expr, $sources:expr) => {
        event!(
            target: "diagweave::report",
            $level,
            error_message = %$ir.error.message,
            error_type = %$ir.error.r#type,
            error_code = ?$ir.metadata.error_code,
            severity = ?$ir.metadata.severity,
            category = ?$ir.metadata.category,
            retryable = ?$ir.metadata.retryable,
            trace_id = ?$ir.trace.as_ref().and_then(|t| t.context.trace_id.as_ref()),
            span_id = ?$ir.trace.as_ref().and_then(|t| t.context.span_id.as_ref()),
            parent_span_id = ?$ir.trace.as_ref().and_then(|t| t.context.parent_span_id.as_ref()),
            sampled = ?$ir.trace.as_ref().and_then(|t| t.context.sampled),
            trace_state = ?$ir.trace.as_ref().and_then(|t| t.context.trace_state.as_ref()),
            trace_flags = ?$ir.trace.as_ref().and_then(|t| t.context.flags),
            context_count = $ir.context_count,
            attachment_count = $ir.attachment_count,
            stack_trace_present = $ir.metadata.stack_trace.is_some(),
            stack_trace_frame_count = $ir.metadata.stack_trace.as_ref().map(|x| x.frames.len()).unwrap_or(0),
            trace_event_count = $ir.trace.as_ref().map(|t| t.events.len()).unwrap_or(0),
            report_context = ?$context,
            report_attachments = ?$attachments,
            report_stack_trace = ?$stack,
            report_display_causes = ?$display,
            report_source_errors = ?$sources,
            "diagweave report emitted"
        )
    };
}

fn emit_report_event(
    level: Level,
    ir: &DiagnosticIr,
    context: &Vec<AttachmentValue>,
    attachments: &Vec<AttachmentValue>,
    stack_trace: Option<&AttachmentValue>,
    display_causes: &AttachmentValue,
    source_errors: &AttachmentValue,
) {
    match level {
        Level::TRACE => report_event!(
            Level::TRACE,
            ir,
            context,
            attachments,
            stack_trace,
            display_causes,
            source_errors
        ),
        Level::DEBUG => report_event!(
            Level::DEBUG,
            ir,
            context,
            attachments,
            stack_trace,
            display_causes,
            source_errors
        ),
        Level::INFO => report_event!(
            Level::INFO,
            ir,
            context,
            attachments,
            stack_trace,
            display_causes,
            source_errors
        ),
        Level::WARN => report_event!(
            Level::WARN,
            ir,
            context,
            attachments,
            stack_trace,
            display_causes,
            source_errors
        ),
        Level::ERROR => report_event!(
            Level::ERROR,
            ir,
            context,
            attachments,
            stack_trace,
            display_causes,
            source_errors
        ),
    }
}

macro_rules! trace_event {
    ($level:expr, $idx:expr, $event:expr, $context:expr, $attachments:expr, $stack:expr, $display:expr, $sources:expr) => {
        event!(
            target: "diagweave::trace_event",
            $level,
            trace_event_index = $idx,
            trace_event_name = %$event.name,
            trace_event_level = ?$event.level,
            trace_event_timestamp_unix_nano = ?$event.timestamp_unix_nano,
            trace_event_attributes = ?$event.attributes,
            report_context = ?$context,
            report_attachments = ?$attachments,
            report_stack_trace = ?$stack,
            report_display_causes = ?$display,
            report_source_errors = ?$sources,
            "diagweave trace event"
        )
    };
}

fn emit_trace_event(
    level: Level,
    idx: usize,
    event: &TraceEvent,
    context: &Vec<AttachmentValue>,
    attachments: &Vec<AttachmentValue>,
    stack_trace: Option<&AttachmentValue>,
    display_causes: &AttachmentValue,
    source_errors: &AttachmentValue,
) {
    match level {
        Level::TRACE => trace_event!(
            Level::TRACE,
            idx,
            event,
            context,
            attachments,
            stack_trace,
            display_causes,
            source_errors
        ),
        Level::DEBUG => trace_event!(
            Level::DEBUG,
            idx,
            event,
            context,
            attachments,
            stack_trace,
            display_causes,
            source_errors
        ),
        Level::INFO => trace_event!(
            Level::INFO,
            idx,
            event,
            context,
            attachments,
            stack_trace,
            display_causes,
            source_errors
        ),
        Level::WARN => trace_event!(
            Level::WARN,
            idx,
            event,
            context,
            attachments,
            stack_trace,
            display_causes,
            source_errors
        ),
        Level::ERROR => trace_event!(
            Level::ERROR,
            idx,
            event,
            context,
            attachments,
            stack_trace,
            display_causes,
            source_errors
        ),
    }
}
