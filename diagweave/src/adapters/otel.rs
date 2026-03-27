use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;

use crate::render_impl::{
    DiagnosticIr, build_display_causes_value, build_error_value, build_source_errors_value,
    build_stack_trace_value,
};
use crate::report::{Attachment, AttachmentValue, ErrorCode};

fn error_code_otel_value(value: &ErrorCode) -> OtelValue {
    match value {
        ErrorCode::Integer(v) => OtelValue::Int(*v),
        ErrorCode::String(v) => OtelValue::String(v.clone()),
    }
}

/// An attribute for OpenTelemetry.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct OtelAttribute {
    pub key: Cow<'static, str>,
    pub value: OtelValue,
}

/// An OpenTelemetry log/event record shaped like the OTLP log data model.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct OtelEvent {
    pub name: Cow<'static, str>,
    pub body: Option<OtelValue>,
    pub timestamp_unix_nano: Option<u64>,
    pub observed_timestamp_unix_nano: Option<u64>,
    pub severity_text: Option<Cow<'static, str>>,
    pub severity_number: Option<u8>,
    pub trace_id: Option<Cow<'static, str>>,
    pub span_id: Option<Cow<'static, str>>,
    pub trace_flags: Option<u8>,
    pub attributes: Vec<OtelAttribute>,
}

/// OTLP-friendly OpenTelemetry value representation.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub enum OtelValue {
    Null,
    String(Cow<'static, str>),
    Int(i64),
    U64(u64),
    Double(f64),
    Bool(bool),
    Bytes(Vec<u8>),
    Array(Vec<OtelValue>),
    KvList(Vec<OtelAttribute>),
}

impl OtelValue {
    /// Returns a compact string representation for debugging and examples.
    pub fn as_string(&self) -> Cow<'static, str> {
        match self {
            Self::Null => Cow::Borrowed("null"),
            Self::String(v) => v.clone(),
            Self::Int(v) => Cow::Owned(v.to_string()),
            Self::U64(v) => Cow::Owned(v.to_string()),
            Self::Double(v) => Cow::Owned(v.to_string()),
            Self::Bool(v) => Cow::Owned(v.to_string()),
            Self::Bytes(v) => Cow::Owned(format!("<{} bytes>", v.len())),
            Self::Array(v) => Cow::Owned(format!("{v:?}")),
            Self::KvList(v) => Cow::Owned(format!("{v:?}")),
        }
    }
}

impl core::fmt::Display for OtelValue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_string().as_ref())
    }
}

impl From<&AttachmentValue> for OtelValue {
    fn from(value: &AttachmentValue) -> Self {
        match value {
            AttachmentValue::Null => Self::Null,
            AttachmentValue::String(v) => Self::String(v.clone()),
            AttachmentValue::Integer(v) => Self::Int(*v),
            AttachmentValue::Unsigned(v) => Self::U64(*v),
            AttachmentValue::Float(v) => Self::Double(*v),
            AttachmentValue::Bool(v) => Self::Bool(*v),
            AttachmentValue::Array(values) => {
                Self::Array(values.iter().map(OtelValue::from).collect())
            }
            AttachmentValue::Object(values) => {
                let attrs = values
                    .iter()
                    .map(|(k, v)| OtelAttribute {
                        key: k.clone().into(),
                        value: OtelValue::from(v),
                    })
                    .collect();
                Self::KvList(attrs)
            }
            AttachmentValue::Bytes(v) => Self::Bytes(v.clone()),
            AttachmentValue::Redacted { kind, reason } => {
                let mut fields = BTreeMap::new();
                fields.insert(
                    "kind".to_string(),
                    kind.as_ref()
                        .map(|v| AttachmentValue::String(v.clone()))
                        .unwrap_or(AttachmentValue::Null),
                );
                fields.insert(
                    "reason".to_string(),
                    reason
                        .as_ref()
                        .map(|v| AttachmentValue::String(v.clone()))
                        .unwrap_or(AttachmentValue::Null),
                );
                let attrs = fields
                    .iter()
                    .map(|(k, v)| OtelAttribute {
                        key: k.clone().into(),
                        value: OtelValue::from(v),
                    })
                    .collect();
                Self::KvList(attrs)
            }
        }
    }
}

/// A batch of OpenTelemetry log/event records.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct OtelEnvelope {
    pub records: Vec<OtelEvent>,
}

pub const REPORT_OTEL_SCHEMA_VERSION: &str = "v0.1.0";

pub const REPORT_OTEL_SCHEMA_DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";

/// Returns the OTEL schema for the diagnostic envelope.
pub fn report_otel_schema() -> &'static str {
    include_str!("../../schemas/report-otel-v0.1.0.schema.json")
}

impl DiagnosticIr<'_> {
    /// Converts the diagnostic IR to OpenTelemetry log/event records.
    pub fn to_otel_envelope(&self) -> OtelEnvelope {
        let mut records = Vec::new();
        records.push(self.otel_report_record());

        #[cfg(feature = "trace")]
        self.otel_trace_ev(&mut records);

        OtelEnvelope { records }
    }

    fn otel_report_record(&self) -> OtelEvent {
        let severity = self
            .metadata
            .severity
            .unwrap_or(crate::report::Severity::Error);
        let mut attributes = Vec::new();

        attributes.push(OtelAttribute {
            key: "exception.type".into(),
            value: OtelValue::String(self.error.r#type.clone().into_owned().into()),
        });
        attributes.push(OtelAttribute {
            key: "exception.message".into(),
            value: OtelValue::String(self.error.message.to_string_owned().into()),
        });
        if let Some(stack_trace) = &self.metadata.stack_trace {
            attributes.push(OtelAttribute {
                key: "exception.stacktrace".into(),
                value: OtelValue::from(&build_stack_trace_value(stack_trace)),
            });
        }
        if let Some(error_code) = &self.metadata.error_code {
            attributes.push(Self::otel_error_code_attr("error.code", error_code));
        }
        if let Some(category) = self.metadata.category {
            attributes.push(OtelAttribute {
                key: "error.category".into(),
                value: OtelValue::String((*category).clone()),
            });
        }
        if let Some(retryable) = self.metadata.retryable {
            attributes.push(OtelAttribute {
                key: "error.retryable".into(),
                value: OtelValue::Bool(retryable),
            });
        }

        #[cfg(feature = "trace")]
        self.otel_trace_correlation(&mut attributes);
        self.otel_diagnostic_bag(&mut attributes);
        self.otel_attachment_attributes(&mut attributes);

        #[cfg(feature = "trace")]
        let trace_id = self
            .trace
            .and_then(|trace| trace.context.trace_id.as_ref().map(|v| v.as_cow()));
        #[cfg(not(feature = "trace"))]
        let trace_id: Option<Cow<'static, str>> = None;

        #[cfg(feature = "trace")]
        let span_id = self
            .trace
            .and_then(|trace| trace.context.span_id.as_ref().map(|v| v.as_cow()));
        #[cfg(not(feature = "trace"))]
        let span_id: Option<Cow<'static, str>> = None;

        #[cfg(feature = "trace")]
        let trace_flags = self.trace.and_then(|trace| trace.context.flags);
        #[cfg(not(feature = "trace"))]
        let trace_flags: Option<u8> = None;

        OtelEvent {
            name: "exception".into(),
            body: Some(OtelValue::from(&build_error_value(&self.error))),
            timestamp_unix_nano: None,
            observed_timestamp_unix_nano: None,
            severity_text: Some(Cow::from(severity)),
            severity_number: Some(severity_to_otel_number(severity)),
            trace_id,
            span_id,
            trace_flags,
            attributes,
        }
    }

    fn otel_error_code_attr(key: &'static str, value: &ErrorCode) -> OtelAttribute {
        OtelAttribute {
            key: key.into(),
            value: error_code_otel_value(value),
        }
    }

    #[cfg(feature = "trace")]
    fn otel_trace_correlation(&self, attributes: &mut Vec<OtelAttribute>) {
        let Some(trace) = self.trace else {
            return;
        };
        if let Some(parent_span_id) = trace.context.parent_span_id.as_ref() {
            attributes.push(OtelAttribute {
                key: "trace.parent_span_id".into(),
                value: OtelValue::String(parent_span_id.as_cow()),
            });
        }
        if let Some(trace_state) = trace.context.trace_state.as_ref() {
            attributes.push(OtelAttribute {
                key: "trace.state".into(),
                value: OtelValue::String(trace_state.clone()),
            });
        }
    }

    fn otel_diagnostic_bag(&self, attributes: &mut Vec<OtelAttribute>) {
        if !self.display_causes.is_empty() {
            attributes.push(OtelAttribute {
                key: "diagnostic_bag.display_causes".into(),
                value: OtelValue::from(&build_display_causes_value(
                    self.display_causes,
                    self.display_causes_state,
                )),
            });
        }
        if let Some(source_errors) = self.source_errors.as_ref() {
            attributes.push(OtelAttribute {
                key: "diagnostic_bag.source_errors".into(),
                value: OtelValue::from(&build_source_errors_value(source_errors)),
            });
        }
    }

    fn otel_attachment_attributes(&self, attributes: &mut Vec<OtelAttribute>) {
        for attachment in self.attachments {
            match attachment {
                Attachment::Context { key, value } => {
                    attributes.push(OtelAttribute {
                        key: key.clone(),
                        value: OtelValue::from(value),
                    });
                }
                Attachment::Note { message } => {
                    attributes.push(OtelAttribute {
                        key: "attachment.note".into(),
                        value: OtelValue::String(message.to_string().into()),
                    });
                }
                Attachment::Payload {
                    name,
                    value,
                    media_type,
                } => {
                    attributes.push(OtelAttribute {
                        key: format!("attachment.payload.{name}").into(),
                        value: OtelValue::from(value),
                    });
                    if let Some(media_type) = media_type {
                        attributes.push(OtelAttribute {
                            key: format!("attachment.payload.{name}.media_type").into(),
                            value: OtelValue::String(media_type.clone()),
                        });
                    }
                }
            }
        }
    }

    #[cfg(feature = "trace")]
    fn otel_trace_ev(&self, records: &mut Vec<OtelEvent>) {
        let trace = match self.trace {
            Some(t) => t,
            None => return,
        };
        let fallback_severity = self
            .metadata
            .severity
            .unwrap_or(crate::report::Severity::Error);
        for trace_event in trace.events.iter() {
            let (severity_text, severity_number) = match trace_event.level {
                Some(level) => severity_for_trace_level(level),
                None => (
                    Cow::from(fallback_severity),
                    severity_to_otel_number(fallback_severity),
                ),
            };
            let mut attributes = trace_event
                .attributes
                .iter()
                .map(|attr| OtelAttribute {
                    key: attr.key.clone(),
                    value: OtelValue::from(&attr.value),
                })
                .collect::<Vec<_>>();
            if let Some(parent_span_id) = trace.context.parent_span_id.as_ref() {
                attributes.push(OtelAttribute {
                    key: "trace.parent_span_id".into(),
                    value: OtelValue::String(parent_span_id.as_cow()),
                });
            }
            if let Some(trace_state) = trace.context.trace_state.as_ref() {
                attributes.push(OtelAttribute {
                    key: "trace.state".into(),
                    value: OtelValue::String(trace_state.clone()),
                });
            }
            records.push(OtelEvent {
                name: trace_event.name.clone(),
                body: None,
                timestamp_unix_nano: trace_event.timestamp_unix_nano,
                observed_timestamp_unix_nano: None,
                severity_text: Some(severity_text),
                severity_number: Some(severity_number),
                trace_id: trace.context.trace_id.as_ref().map(|v| v.as_cow()),
                span_id: trace.context.span_id.as_ref().map(|v| v.as_cow()),
                trace_flags: trace.context.flags,
                attributes,
            });
        }
    }
}

fn severity_to_otel_number(severity: crate::report::Severity) -> u8 {
    match severity {
        crate::report::Severity::Debug => 5,
        crate::report::Severity::Info => 9,
        crate::report::Severity::Warn => 13,
        crate::report::Severity::Error => 17,
        crate::report::Severity::Fatal => 21,
    }
}

#[cfg(feature = "trace")]
fn severity_for_trace_level(level: crate::report::TraceEventLevel) -> (Cow<'static, str>, u8) {
    match level {
        crate::report::TraceEventLevel::Trace => ("trace".into(), 1),
        crate::report::TraceEventLevel::Debug => ("debug".into(), 5),
        crate::report::TraceEventLevel::Info => ("info".into(), 9),
        crate::report::TraceEventLevel::Warn => ("warn".into(), 13),
        crate::report::TraceEventLevel::Error => ("error".into(), 17),
    }
}
