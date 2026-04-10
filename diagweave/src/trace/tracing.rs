use tracing::{Level, event};

use crate::render_impl::{
    DiagnosticIr, build_ctx_and_attachments, build_diag_src_errs_val, build_display_causes,
    build_origin_src_errs_val, build_stack_trace_value,
};
use crate::report::{AttachmentValue, HasSeverity, ReportTrace, TraceEvent};

use super::{EmitStats, PreparedTracingEmission, PreparedTracingLevel, TracingExporterTrait};

/// Implementation of `TracingExporterTrait` that emits reports to the `tracing` system.
#[derive(Debug, Clone, Copy, Default)]
pub struct TracingExporter;

impl TracingExporterTrait for TracingExporter {
    fn export_prepared(&self, emission: PreparedTracingEmission<'_>) -> EmitStats {
        let stats = emission.stats();
        let report_level = prepared_level_to_tracing(emission.report_level());
        let ir = emission.ir();
        let (context_value, system_value, attachment_items) =
            build_ctx_and_attachments(ir.context, ir.system, ir.attachments);
        let stack_trace_value = ir.metadata.stack_trace().map(build_stack_trace_value);
        let display_causes_value = build_display_causes(ir.display_causes, ir.display_causes_state);
        let origin_source_errors_value = ir
            .origin_source_errors
            .as_ref()
            .map(build_origin_src_errs_val);
        let diagnostic_source_errors_value = ir
            .diagnostic_source_errors
            .as_ref()
            .map(build_diag_src_errs_val);

        emit_report_event(
            report_level,
            ReportEventFields {
                ir,
                context: &context_value,
                system: &system_value,
                attachments: attachment_items.as_slice(),
                stack_trace: stack_trace_value.as_ref(),
                display_causes: &display_causes_value,
                origin_source_errors: origin_source_errors_value.as_ref(),
                diagnostic_source_errors: diagnostic_source_errors_value.as_ref(),
            },
        );

        if !ir.trace.is_empty() {
            for prepared_event in emission.trace_events() {
                emit_trace_event(
                    prepared_level_to_tracing(prepared_event.level()),
                    prepared_event.index(),
                    prepared_event.event(),
                    &ir.trace,
                );
            }
        }

        stats
    }
}

fn prepared_level_to_tracing(level: PreparedTracingLevel) -> Level {
    match level {
        PreparedTracingLevel::Trace => Level::TRACE,
        PreparedTracingLevel::Debug => Level::DEBUG,
        PreparedTracingLevel::Info => Level::INFO,
        PreparedTracingLevel::Warn => Level::WARN,
        PreparedTracingLevel::Error => Level::ERROR,
    }
}

macro_rules! report_event {
    ($level:expr, $ir:expr, $context:expr, $system:expr, $attachments:expr, $stack:expr, $display:expr, $origin_sources:expr, $diagnostic_sources:expr) => {
        event!(
            target: "diagweave::report",
            $level,
            error_message = %$ir.error.message,
            error_type = %$ir.error.r#type,
            error_code = ?$ir.metadata.error_code(),
            error_severity = ?$ir.metadata.severity(),
            error_required_severity = ?$ir.metadata.required_severity(),
            error_category = ?$ir.metadata.category(),
            error_retryable = ?$ir.metadata.retryable(),
            trace_id = ?$ir.trace.context().and_then(|t| t.trace_id.as_ref()),
            span_id = ?$ir.trace.context().and_then(|t| t.span_id.as_ref()),
            parent_span_id = ?$ir.trace.context().and_then(|t| t.parent_span_id.as_ref()),
            trace_sampled = ?$ir.trace.context().and_then(|t| t.sampled),
            trace_state = ?$ir.trace.context().and_then(|t| t.trace_state.as_ref()),
            trace_flags = ?$ir.trace.context().and_then(|t| t.flags),
            report_context_count = $ir.context_count,
            report_system_count = $ir.system_count,
            report_attachment_count = $ir.attachment_count,
            trace_event_count = $ir.trace.events().map(|e| e.len()).unwrap_or(0),
            report_context = ?$context,
            report_system = ?$system,
            report_attachments = ?$attachments,
            report_stack_trace = ?$stack,
            report_display_causes = ?$display,
            report_origin_source_errors = ?$origin_sources,
            report_diagnostic_source_errors = ?$diagnostic_sources,
            "diagweave report emitted"
        )
    };
}

struct ReportEventFields<'a, 'ir> {
    ir: &'a DiagnosticIr<'ir, HasSeverity>,
    context: &'a AttachmentValue,
    system: &'a AttachmentValue,
    attachments: &'a [AttachmentValue],
    stack_trace: Option<&'a AttachmentValue>,
    display_causes: &'a AttachmentValue,
    origin_source_errors: Option<&'a AttachmentValue>,
    diagnostic_source_errors: Option<&'a AttachmentValue>,
}

fn emit_report_event(level: Level, fields: ReportEventFields<'_, '_>) {
    macro_rules! emit_with_level {
        ($fixed:expr) => {
            report_event!(
                $fixed,
                fields.ir,
                fields.context,
                fields.system,
                fields.attachments,
                fields.stack_trace,
                fields.display_causes,
                fields.origin_source_errors,
                fields.diagnostic_source_errors
            )
        };
    }
    match level {
        Level::TRACE => emit_with_level!(Level::TRACE),
        Level::DEBUG => emit_with_level!(Level::DEBUG),
        Level::INFO => emit_with_level!(Level::INFO),
        Level::WARN => emit_with_level!(Level::WARN),
        Level::ERROR => emit_with_level!(Level::ERROR),
    }
}

macro_rules! trace_event {
    ($level:expr, $idx:expr, $event:expr, $trace:expr) => {
        event!(
            target: "diagweave::trace_event",
            $level,
            trace_event_index = $idx,
            trace_event_name = %$event.name,
            trace_event_level = ?$event.level,
            trace_event_timestamp_unix_nano = ?$event.timestamp_unix_nano,
            trace_event_attributes = ?$event.attributes,
            trace_id = ?$trace.context().and_then(|t| t.trace_id.as_ref()),
            span_id = ?$trace.context().and_then(|t| t.span_id.as_ref()),
            parent_span_id = ?$trace.context().and_then(|t| t.parent_span_id.as_ref()),
            trace_sampled = ?$trace.context().and_then(|t| t.sampled),
            trace_state = ?$trace.context().and_then(|t| t.trace_state.as_ref()),
            trace_flags = ?$trace.context().and_then(|t| t.flags),
            "diagweave trace event"
        )
    };
}

fn emit_trace_event(level: Level, idx: usize, event: &TraceEvent, trace: &ReportTrace) {
    match level {
        Level::TRACE => trace_event!(Level::TRACE, idx, event, trace),
        Level::DEBUG => trace_event!(Level::DEBUG, idx, event, trace),
        Level::INFO => trace_event!(Level::INFO, idx, event, trace),
        Level::WARN => trace_event!(Level::WARN, idx, event, trace),
        Level::ERROR => trace_event!(Level::ERROR, idx, event, trace),
    }
}
