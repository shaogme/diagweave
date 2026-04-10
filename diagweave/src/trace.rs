#[cfg(feature = "tracing")]
#[path = "trace/tracing.rs"]
mod tracing;

use alloc::vec::Vec;
use core::error::Error;
use core::fmt::{Debug, Display, Formatter};

use crate::render::DiagnosticIr;
use crate::report::{HasSeverity, Report, Severity, TraceEvent, TraceEventLevel};

#[cfg(feature = "tracing")]
pub use tracing::TracingExporter;

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
        let report_level = severity_to_prepared_level(ir.metadata.required_severity());
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
    E: Error + Display + 'static,
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

fn severity_to_prepared_level(level: Severity) -> PreparedTracingLevel {
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
