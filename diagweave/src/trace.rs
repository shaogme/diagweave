#[cfg(feature = "tracing")]
#[path = "trace/tracing.rs"]
mod tracing;

use core::error::Error;
use core::fmt::Display;

use crate::render::DiagnosticIr;
use crate::report::Report;

#[cfg(feature = "tracing")]
pub use tracing::TracingExporter;

/// Trait for exporting diagnostics to a tracing system.
pub trait TracingExporterTrait {
    /// Exports a `DiagnosticIr` to the tracing system.
    fn export_ir(&self, ir: &DiagnosticIr<'_>);
}

impl DiagnosticIr<'_> {
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

impl<E> Report<E>
where
    E: Error + Display + 'static,
{
    /// Emits the report using the default tracing exporter.
    #[cfg(feature = "tracing")]
    pub fn emit_tracing(&self) {
        let ir = self.to_diagnostic_ir();
        TracingExporter.export_ir(&ir);
    }

    /// Emits the report using a specific tracing exporter.
    pub fn emit_tracing_with(&self, exporter: &impl TracingExporterTrait) {
        let ir = self.to_diagnostic_ir();
        exporter.export_ir(&ir);
    }
}
