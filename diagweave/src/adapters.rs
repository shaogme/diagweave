#[cfg(feature = "otel")]
#[path = "adapters/otel.rs"]
mod otel;
#[cfg(feature = "trace")]
#[path = "adapters/trace.rs"]
mod trace;

#[cfg(feature = "otel")]
pub use otel::{
    OtelAttribute, OtelEnvelope, OtelEvent, OtelValue, REPORT_OTEL_SCHEMA_DRAFT,
    REPORT_OTEL_SCHEMA_VERSION, report_otel_schema,
};
#[cfg(feature = "trace")]
pub use trace::TracingField;
