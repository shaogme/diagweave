#[cfg(feature = "tracing")]
#[path = "trace/tracing.rs"]
mod tracing;

use core::error::Error;
use core::fmt::Display;

use crate::render::{DiagnosticIr, ReportRenderOptions};
use crate::report::{CauseStore, Report};

#[cfg(feature = "tracing")]
pub use tracing::TracingExporter;

/// Trait for exporting diagnostics to a tracing system.
pub trait TracingExporterTrait {
    /// Exports a `DiagnosticIr` to the tracing system.
    fn export_ir(&self, ir: &DiagnosticIr);
}

impl DiagnosticIr {
    /// Emits the diagnostic information using the default tracing exporter.
    #[cfg(feature = "tracing")]
    pub fn emit_tracing(&self) {
        TracingExporter.export_ir(self);
    }

    /// Emits the diagnostic information using a specific tracing exporter.
    pub fn emit_tracing_with(&self, exporter: &impl TracingExporterTrait) {
        exporter.export_ir(self);
    }
}

impl<E, C> Report<E, C>
where
    E: Error + Display + 'static,
    C: CauseStore,
{
    /// Emits the report using the default tracing exporter.
    #[cfg(feature = "tracing")]
    pub fn emit_tracing(&self, options: ReportRenderOptions) {
        let ir = self.to_diagnostic_ir(options);
        TracingExporter.export_ir(&ir);
    }

    /// Emits the report using a specific tracing exporter.
    pub fn emit_tracing_with(
        &self,
        exporter: &impl TracingExporterTrait,
        options: ReportRenderOptions,
    ) {
        let ir = self.to_diagnostic_ir(options);
        exporter.export_ir(&ir);
    }
}
