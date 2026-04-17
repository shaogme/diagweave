use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryFrom;

use opentelemetry::logs::{AnyValue, LogRecord, Logger, LoggerProvider, Severity};
use opentelemetry::{Key, TraceFlags};

use crate::otel::{OtelEnvelope, OtelEvent, OtelSeverityNumber, OtelValue};

/// Number of records emitted through an OTEL SDK bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OtelSdkEmitStats {
    pub records_emitted: usize,
}

/// Bridge that writes diagweave OTEL envelopes into an OpenTelemetry logger provider.
#[derive(Debug, Clone)]
pub struct OtelSdkEmitter<'a, P> {
    provider: &'a P,
    logger_name: Cow<'static, str>,
}

impl<'a, P> OtelSdkEmitter<'a, P> {
    /// Creates a new bridge for the given logger provider.
    pub fn new(provider: &'a P, logger_name: impl Into<Cow<'static, str>>) -> Self {
        Self {
            provider,
            logger_name: logger_name.into(),
        }
    }
}

impl<'a, P> OtelSdkEmitter<'a, P>
where
    P: LoggerProvider,
{
    /// Emits a full OTEL envelope to the logger provider.
    pub fn emit_envelope(&self, envelope: &OtelEnvelope<'_>) -> OtelSdkEmitStats {
        self.emit_envelope_with(envelope, |_, _, _, _| {})
    }

    /// Emits a full OTEL envelope to the logger provider and lets the caller
    /// add extra SDK attributes per record.
    pub fn emit_envelope_with<F>(
        &self,
        envelope: &OtelEnvelope<'_>,
        mut decorate: F,
    ) -> OtelSdkEmitStats
    where
        F: FnMut(usize, usize, &OtelEvent<'_>, &mut <P::Logger as Logger>::LogRecord),
    {
        let logger = self.provider.logger(self.logger_name.clone());
        let record_count = envelope.records.len();

        for (index, event) in envelope.records.iter().enumerate() {
            let mut record = logger.create_log_record();
            copy_event_to_record(event, &mut record);
            decorate(index, record_count, event, &mut record);
            logger.emit(record);
        }

        OtelSdkEmitStats {
            records_emitted: record_count,
        }
    }
}

impl<'a> OtelEnvelope<'a> {
    /// Emits this OTEL envelope to the given logger provider.
    pub fn emit_to<P>(
        &self,
        provider: &P,
        logger_name: impl Into<Cow<'static, str>>,
    ) -> OtelSdkEmitStats
    where
        P: LoggerProvider,
    {
        OtelSdkEmitter::new(provider, logger_name).emit_envelope(self)
    }

    /// Emits this OTEL envelope to the given logger provider and lets the
    /// caller add extra SDK attributes per record.
    pub fn emit_to_with<P, F>(
        &self,
        provider: &P,
        logger_name: impl Into<Cow<'static, str>>,
        decorate: F,
    ) -> OtelSdkEmitStats
    where
        P: LoggerProvider,
        F: FnMut(usize, usize, &OtelEvent<'_>, &mut <P::Logger as Logger>::LogRecord),
    {
        OtelSdkEmitter::new(provider, logger_name).emit_envelope_with(self, decorate)
    }
}

fn copy_event_to_record<R>(event: &OtelEvent<'_>, record: &mut R)
where
    R: LogRecord,
{
    if let Some(timestamp) = event
        .timestamp_unix_nano
        .and_then(unix_nanos_to_system_time)
    {
        record.set_timestamp(timestamp);
    }
    if let Some(observed_timestamp) = event
        .observed_timestamp_unix_nano
        .and_then(unix_nanos_to_system_time)
    {
        record.set_observed_timestamp(observed_timestamp);
    }
    if let Some(severity_number) = event.severity_number {
        record.set_severity_number(otel_log_severity(severity_number));
    }
    if let Some(body) = event.body.as_ref() {
        record.set_body(otel_value_to_any_value(body));
    }
    if let (Some(trace_id), Some(span_id)) = (event.trace_id.as_ref(), event.span_id.as_ref()) {
        let trace_id = opentelemetry::TraceId::from_hex(trace_id.as_ref()).ok();
        let span_id = opentelemetry::SpanId::from_hex(span_id.as_ref()).ok();
        if let (Some(trace_id), Some(span_id)) = (trace_id, span_id) {
            let trace_flags = event.trace_flags.map(TraceFlags::new);
            record.set_trace_context(trace_id, span_id, trace_flags);
        }
    }
    for attr in &event.attributes {
        record.add_attribute(
            attr.key.as_ref().to_owned(),
            otel_value_to_any_value(&attr.value),
        );
    }
}

fn otel_log_severity(number: OtelSeverityNumber) -> Severity {
    match number.as_u8() {
        1 => Severity::Trace,
        5 => Severity::Debug,
        9 => Severity::Info,
        13 => Severity::Warn,
        17 => Severity::Error,
        21 => Severity::Fatal,
        _ => Severity::Info,
    }
}

fn otel_value_to_any_value(value: &OtelValue<'_>) -> AnyValue {
    match value {
        OtelValue::String(v) => AnyValue::from(v.to_string()),
        OtelValue::Int(v) => AnyValue::from(*v),
        OtelValue::U64(v) => match i64::try_from(*v) {
            Ok(v) => AnyValue::from(v),
            Err(_) => AnyValue::from(v.to_string()),
        },
        OtelValue::Double(v) => AnyValue::from(*v),
        OtelValue::Bool(v) => AnyValue::from(*v),
        OtelValue::Bytes(v) => AnyValue::Bytes(Box::new(v.clone())),
        OtelValue::Array(values) => AnyValue::ListAny(Box::new(
            values.iter().map(otel_value_to_any_value).collect(),
        )),
        OtelValue::KvList(values) => {
            AnyValue::Map(Box::new(HashMap::from_iter(values.iter().map(|attr| {
                (
                    Key::new(attr.key.as_ref().to_owned()),
                    otel_value_to_any_value(&attr.value),
                )
            }))))
        }
    }
}

fn unix_nanos_to_system_time(unix_nanos: u64) -> Option<std::time::SystemTime> {
    std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_nanos(unix_nanos))
}
