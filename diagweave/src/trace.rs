#[cfg(feature = "tracing")]
#[path = "trace/tracing.rs"]
mod tracing;

use alloc::string::ToString;
use alloc::vec::Vec;
use core::error::Error;
use core::fmt::{Debug, Display, Formatter};
use ref_str::RefStr;

use crate::render::DiagnosticIr;
use crate::render_impl::{
    build_ctx_and_attachments, build_diag_src_errs_val, build_display_causes, build_error_value,
    build_origin_src_errs_val, build_stack_trace_value, build_trace_value,
};
use crate::report::{
    AttachmentValue, ErrorCode, HasSeverity, Report, Severity, SeverityState, TraceEvent,
    TraceEventLevel,
};

#[cfg(feature = "tracing")]
pub use tracing::TracingExporter;

fn error_code_value(value: &ErrorCode) -> AttachmentValue {
    match value {
        ErrorCode::Integer(v) => AttachmentValue::Integer(*v),
        ErrorCode::String(v) => AttachmentValue::String(v.clone()),
    }
}

/// A key-value pair for Tracing fields.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(bound(deserialize = "'de: 'a")))]
pub struct TracingField<'a> {
    pub key: RefStr<'a>,
    pub value: AttachmentValue,
}

impl<State> DiagnosticIr<'_, State>
where
    State: SeverityState,
{
    /// Converts the diagnostic IR to a vector of tracing fields.
    pub fn to_tracing_fields(&self) -> Vec<TracingField<'_>> {
        let mut fields = Vec::new();

        self.tracing_error(&mut fields);
        self.tracing_meta(&mut fields);
        self.tracing_stack_causes(&mut fields);
        #[cfg(feature = "trace")]
        self.tracing_trace(&mut fields);
        self.tracing_ctx_attrs(&mut fields);

        fields
    }

    fn tracing_error(&self, fields: &mut Vec<TracingField<'_>>) {
        fields.push(TracingField {
            key: "error".into(),
            value: build_error_value(&self.error),
        });
    }

    fn tracing_meta(&self, fields: &mut Vec<TracingField<'_>>) {
        if let Some(error_code) = self.metadata.error_code() {
            fields.push(TracingField {
                key: "metadata.error_code".into(),
                value: error_code_value(error_code),
            });
        }
        if let Some(severity) = self.metadata.severity() {
            fields.push(TracingField {
                key: "metadata.severity".into(),
                value: AttachmentValue::String(severity.to_string().into()),
            });
        }
        if let Some(category) = self.metadata.category() {
            fields.push(TracingField {
                key: "metadata.category".into(),
                value: AttachmentValue::String(category.to_string().into()),
            });
        }
        if let Some(retryable) = self.metadata.retryable() {
            fields.push(TracingField {
                key: "metadata.retryable".into(),
                value: AttachmentValue::Bool(retryable),
            });
        }
    }

    #[cfg(feature = "trace")]
    fn tracing_trace(&self, fields: &mut Vec<TracingField<'_>>) {
        if self.trace.is_empty() {
            return;
        }
        let trace_value = build_trace_value(self.trace, &self.error);
        fields.push(TracingField {
            key: "trace".into(),
            value: trace_value,
        });
    }

    fn tracing_stack_causes(&self, fields: &mut Vec<TracingField<'_>>) {
        if let Some(stack_trace) = self.metadata.stack_trace() {
            fields.push(TracingField {
                key: "diagnostic_bag.stack_trace".into(),
                value: build_stack_trace_value(stack_trace),
            });
        }
        if !self.display_causes.is_empty() {
            fields.push(TracingField {
                key: "diagnostic_bag.display_causes".into(),
                value: build_display_causes(self.display_causes, self.display_causes_state),
            });
        }
        if let Some(source_errors) = self.origin_source_errors.as_ref() {
            fields.push(TracingField {
                key: "diagnostic_bag.origin_source_errors".into(),
                value: build_origin_src_errs_val(source_errors),
            });
        }
        if let Some(source_errors) = self.diagnostic_source_errors.as_ref() {
            fields.push(TracingField {
                key: "diagnostic_bag.diagnostic_source_errors".into(),
                value: build_diag_src_errs_val(source_errors),
            });
        }
    }

    fn tracing_ctx_attrs(&self, fields: &mut Vec<TracingField<'_>>) {
        let (context_value, system_value, attachment_items): (
            AttachmentValue,
            AttachmentValue,
            Vec<AttachmentValue>,
        ) = build_ctx_and_attachments(self.context, self.system, self.attachments);

        if !self.context.is_empty() {
            fields.push(TracingField {
                key: "context".into(),
                value: context_value,
            });
        }
        if !self.system.is_empty() {
            fields.push(TracingField {
                key: "system".into(),
                value: system_value,
            });
        }
        if !attachment_items.is_empty() {
            fields.push(TracingField {
                key: "attachments".into(),
                value: AttachmentValue::Array(attachment_items),
            });
        }
    }
}

/// Resolved tracing level after severity fallback has been applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreparedTracingLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Display for PreparedTracingLevel {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let label = match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        };
        f.write_str(label)
    }
}

/// Counts emitted tracing records after a successful export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmitStats {
    pub report_events_emitted: usize,
    pub trace_events_emitted: usize,
}

impl EmitStats {
    /// Returns the total number of emitted tracing records.
    pub const fn total_events_emitted(self) -> usize {
        self.report_events_emitted + self.trace_events_emitted
    }
}

/// A fully validated tracing emission with all fallback levels resolved.
pub struct PreparedTracingEmission<'a> {
    ir: DiagnosticIr<'a, HasSeverity>,
    report_level: PreparedTracingLevel,
    trace_event_levels: Vec<PreparedTracingLevel>,
}

impl Debug for PreparedTracingEmission<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PreparedTracingEmission")
            .field("report_level", &self.report_level)
            .field("trace_event_levels", &self.trace_event_levels)
            .field("stats", &self.stats())
            .finish_non_exhaustive()
    }
}

impl<'a> PreparedTracingEmission<'a> {
    fn prepare(ir: DiagnosticIr<'a, HasSeverity>) -> Self {
        let report_level = severity_to_level(ir.metadata.required_severity());
        let trace_event_levels = ir
            .trace
            .events()
            .map(|events| {
                events
                    .iter()
                    .map(|trace_event| {
                        trace_level_to_prepared(trace_event.level).unwrap_or(report_level)
                    })
                    .collect()
            })
            .unwrap_or_default();

        Self {
            ir,
            report_level,
            trace_event_levels,
        }
    }

    /// Returns the frozen diagnostic IR captured during preparation.
    pub fn ir(&self) -> &DiagnosticIr<'a, HasSeverity> {
        &self.ir
    }

    /// Returns the resolved report-level tracing severity.
    pub fn report_level(&self) -> PreparedTracingLevel {
        self.report_level
    }

    /// Returns the resolved tracing level for a trace event by index.
    pub fn trace_event_level(&self, index: usize) -> Option<PreparedTracingLevel> {
        self.trace_event_levels.get(index).copied()
    }

    /// Returns the resolved tracing levels for all trace events.
    pub fn trace_event_levels(&self) -> &[PreparedTracingLevel] {
        self.trace_event_levels.as_slice()
    }

    /// Iterates over resolved trace events paired with their final tracing levels.
    pub fn trace_events(&self) -> impl Iterator<Item = PreparedTraceEvent<'_>> + '_ {
        let events = self.ir.trace.events().unwrap_or(&[]);
        events
            .iter()
            .enumerate()
            .zip(self.trace_event_levels.iter().copied())
            .map(|((index, event), level)| PreparedTraceEvent {
                index,
                event,
                level,
            })
    }

    /// Returns the number of tracing records this prepared emission will produce.
    pub fn stats(&self) -> EmitStats {
        EmitStats {
            report_events_emitted: 1,
            trace_events_emitted: self.trace_event_levels.len(),
        }
    }

    /// Emits the prepared tracing payload using the default tracing exporter.
    #[cfg(feature = "tracing")]
    pub fn emit(self) -> EmitStats {
        TracingExporter.export_prepared(self)
    }

    /// Emits the prepared tracing payload using a specific exporter.
    pub fn emit_with<TExporter>(self, exporter: &TExporter) -> EmitStats
    where
        TExporter: TracingExporterTrait,
    {
        exporter.export_prepared(self)
    }
}

/// A trace event paired with its resolved tracing level.
#[derive(Clone, Copy)]
pub struct PreparedTraceEvent<'a> {
    index: usize,
    event: &'a TraceEvent,
    level: PreparedTracingLevel,
}

impl<'a> PreparedTraceEvent<'a> {
    /// Returns the original event index within the report trace.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Returns the original trace event.
    pub fn event(&self) -> &'a TraceEvent {
        self.event
    }

    /// Returns the resolved tracing level used for emission.
    pub fn level(&self) -> PreparedTracingLevel {
        self.level
    }
}

/// Trait for exporting already-prepared tracing emissions.
pub trait TracingExporterTrait {
    /// Exports a prepared tracing emission.
    fn export_prepared(&self, emission: PreparedTracingEmission<'_>) -> EmitStats;
}

impl DiagnosticIr<'_, HasSeverity> {
    /// Prepares this diagnostic IR for tracing emission by resolving every final
    /// tracing level up front.
    pub fn prepare_tracing(&self) -> PreparedTracingEmission<'_> {
        PreparedTracingEmission::prepare(self.clone())
    }

    /// Convenience wrapper around `prepare_tracing().emit()`.
    #[cfg(feature = "tracing")]
    pub fn emit_tracing(&self) -> EmitStats {
        self.prepare_tracing().emit()
    }

    /// Convenience wrapper around `prepare_tracing().emit_with(exporter)`.
    pub fn emit_tracing_with<TExporter>(&self, exporter: &TExporter) -> EmitStats
    where
        TExporter: TracingExporterTrait,
    {
        self.prepare_tracing().emit_with(exporter)
    }
}

impl<E> Report<E, HasSeverity>
where
    E: Error,
{
    /// Prepares this report for tracing emission by freezing its diagnostic IR and
    /// resolving every final tracing level up front.
    pub fn prepare_tracing(&self) -> PreparedTracingEmission<'_> {
        PreparedTracingEmission::prepare(self.to_diagnostic_ir())
    }

    /// Convenience wrapper around `prepare_tracing().emit()`.
    #[cfg(feature = "tracing")]
    pub fn emit_tracing(&self) -> EmitStats {
        self.prepare_tracing().emit()
    }

    /// Convenience wrapper around `prepare_tracing().emit_with(exporter)`.
    pub fn emit_tracing_with<TExporter>(&self, exporter: &TExporter) -> EmitStats
    where
        TExporter: TracingExporterTrait,
    {
        self.prepare_tracing().emit_with(exporter)
    }
}

fn severity_to_level(level: Severity) -> PreparedTracingLevel {
    match level {
        Severity::Trace => PreparedTracingLevel::Trace,
        Severity::Debug => PreparedTracingLevel::Debug,
        Severity::Info => PreparedTracingLevel::Info,
        Severity::Warn => PreparedTracingLevel::Warn,
        Severity::Error | Severity::Fatal => PreparedTracingLevel::Error,
    }
}

fn trace_level_to_prepared(level: Option<TraceEventLevel>) -> Option<PreparedTracingLevel> {
    match level {
        Some(TraceEventLevel::Trace) => Some(PreparedTracingLevel::Trace),
        Some(TraceEventLevel::Debug) => Some(PreparedTracingLevel::Debug),
        Some(TraceEventLevel::Info) => Some(PreparedTracingLevel::Info),
        Some(TraceEventLevel::Warn) => Some(PreparedTracingLevel::Warn),
        Some(TraceEventLevel::Error) => Some(PreparedTracingLevel::Error),
        None => None,
    }
}
