use tracing::{Level, event};

use crate::render::DiagnosticIr;
use crate::report::{Severity, TraceEvent, TraceEventLevel};

use super::TracingExporterTrait;

/// Implementation of `TracingExporterTrait` that emits reports to the `tracing` system.
#[derive(Debug, Clone, Copy, Default)]
pub struct TracingExporter;

impl TracingExporterTrait for TracingExporter {
    fn export_ir(&self, ir: &DiagnosticIr) {
        let report_level = severity_to_level(ir.metadata.severity);
        emit_report_event(report_level, ir);

        if let Some(trace) = ir.trace {
            for (idx, trace_event) in trace.events.iter().enumerate() {
                let trace_level = trace_level_to_tracing(trace_event.level).unwrap_or(report_level);
                emit_trace_event(trace_level, idx, trace_event);
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
    ($level:expr, $ir:expr) => {
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
            context_count = $ir.context.len(),
            attachment_count = $ir.attachments.len(),
            stack_trace_present = $ir.metadata.stack_trace.is_some(),
            stack_trace_frame_count = $ir.metadata.stack_trace.as_ref().map(|x| x.frames.len()).unwrap_or(0),
            display_causes_present = $ir.metadata.display_causes.is_some(),
            display_causes_count = $ir.metadata.display_causes.as_ref().map(|x| x.items.len()).unwrap_or(0),
            display_causes_truncated = $ir.metadata.display_causes.as_ref().map(|x| x.truncated).unwrap_or(false),
            display_causes_cycle_detected = $ir.metadata.display_causes.as_ref().map(|x| x.cycle_detected).unwrap_or(false),
            source_errors_present = $ir.metadata.source_errors.is_some(),
            source_errors_count = $ir.metadata.source_errors.as_ref().map(|x| x.items.len()).unwrap_or(0),
            source_errors_truncated = $ir.metadata.source_errors.as_ref().map(|x| x.truncated).unwrap_or(false),
            source_errors_cycle_detected = $ir.metadata.source_errors.as_ref().map(|x| x.cycle_detected).unwrap_or(false),
            trace_event_count = $ir.trace.as_ref().map(|t| t.events.len()).unwrap_or(0),
            "diagweave report emitted"
        )
    };
}

fn emit_report_event(level: Level, ir: &DiagnosticIr) {
    match level {
        Level::TRACE => report_event!(Level::TRACE, ir),
        Level::DEBUG => report_event!(Level::DEBUG, ir),
        Level::INFO => report_event!(Level::INFO, ir),
        Level::WARN => report_event!(Level::WARN, ir),
        Level::ERROR => report_event!(Level::ERROR, ir),
    }
}

macro_rules! trace_event {
    ($level:expr, $idx:expr, $event:expr) => {
        event!(
            target: "diagweave::trace_event",
            $level,
            trace_event_index = $idx,
            trace_event_name = %$event.name,
            trace_event_level = ?$event.level,
            trace_event_timestamp_unix_nano = ?$event.timestamp_unix_nano,
            trace_event_attributes = ?$event.attributes,
            "diagweave trace event"
        )
    };
}

fn emit_trace_event(level: Level, idx: usize, event: &TraceEvent) {
    match level {
        Level::TRACE => trace_event!(Level::TRACE, idx, event),
        Level::DEBUG => trace_event!(Level::DEBUG, idx, event),
        Level::INFO => trace_event!(Level::INFO, idx, event),
        Level::WARN => trace_event!(Level::WARN, idx, event),
        Level::ERROR => trace_event!(Level::ERROR, idx, event),
    }
}
