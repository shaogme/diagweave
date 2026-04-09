use alloc::borrow::Cow;
use alloc::format;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use ref_str::RefStr;

use crate::render_impl::{
    DiagnosticIr, build_diag_src_errs_val, build_display_causes, build_error_value,
    build_origin_src_errs_val, build_stack_trace_value,
};
use crate::report::{Attachment, AttachmentValue, ContextValue, ErrorCode, HasSeverity};

fn error_code_otel_value(value: &ErrorCode) -> OtelValue<'_> {
    match value {
        ErrorCode::Integer(v) => OtelValue::Int(*v),
        ErrorCode::String(v) => OtelValue::String(v.clone().into()),
    }
}

/// An attribute for OpenTelemetry.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(bound(deserialize = "'de: 'a")))]
pub struct OtelAttribute<'a> {
    pub key: RefStr<'a>,
    pub value: OtelValue<'a>,
}

/// An OpenTelemetry log/event record shaped like the OTLP log data model.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(bound(deserialize = "'de: 'a")))]
pub struct OtelEvent<'a> {
    pub name: RefStr<'a>,
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub body: Option<OtelValue<'a>>,
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub timestamp_unix_nano: Option<u64>,
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub observed_timestamp_unix_nano: Option<u64>,
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub severity_text: Option<ref_str::StaticRefStr>,
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub severity_number: Option<u8>,
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub trace_id: Option<RefStr<'a>>,
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub span_id: Option<RefStr<'a>>,
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub trace_flags: Option<u8>,
    pub attributes: Vec<OtelAttribute<'a>>,
}

/// OTLP-friendly OpenTelemetry value representation.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(bound(deserialize = "'de: 'a")))]
pub enum OtelValue<'a> {
    Null,
    String(RefStr<'a>),
    Int(i64),
    U64(u64),
    Double(f64),
    Bool(bool),
    Bytes(Vec<u8>),
    Array(Vec<OtelValue<'a>>),
    KvList(Vec<OtelAttribute<'a>>),
}

impl OtelValue<'_> {
    /// Returns a cow representation for debugging and examples.
    pub fn as_cow(&self) -> Cow<'_, str> {
        match self {
            Self::Null => Cow::Borrowed("null"),
            Self::String(v) => v.as_ref().into(),
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

impl core::fmt::Display for OtelValue<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_cow().as_ref())
    }
}

impl<'a> From<&'a AttachmentValue> for OtelValue<'a> {
    fn from(value: &'a AttachmentValue) -> Self {
        match value {
            AttachmentValue::Null => Self::Null,
            AttachmentValue::String(v) => Self::String(v.clone().into()),
            AttachmentValue::Integer(v) => Self::Int(*v),
            AttachmentValue::Unsigned(v) => Self::U64(*v),
            AttachmentValue::Float(v) => Self::Double(*v),
            AttachmentValue::Bool(v) => Self::Bool(*v),
            AttachmentValue::Array(values) => {
                Self::Array(values.iter().map(OtelValue::from).collect())
            }
            AttachmentValue::Object(values) => {
                let attrs = values
                    .sorted_entries()
                    .into_iter()
                    .map(|(k, v)| OtelAttribute {
                        key: k.clone().into(),
                        value: OtelValue::from(v),
                    })
                    .collect();
                Self::KvList(attrs)
            }
            AttachmentValue::Bytes(v) => Self::Bytes(v.clone()),
            AttachmentValue::Redacted { kind, reason } => Self::KvList(vec![
                OtelAttribute {
                    key: "kind".into(),
                    value: kind
                        .as_ref()
                        .map(|v| OtelValue::String(v.clone().into()))
                        .unwrap_or(OtelValue::Null),
                },
                OtelAttribute {
                    key: "reason".into(),
                    value: reason
                        .as_ref()
                        .map(|v| OtelValue::String(v.clone().into()))
                        .unwrap_or(OtelValue::Null),
                },
            ]),
        }
    }
}

impl<'a> From<&'a ContextValue> for OtelValue<'a> {
    fn from(value: &'a ContextValue) -> Self {
        match value {
            ContextValue::String(v) => Self::String(v.clone().into()),
            ContextValue::Integer(v) => Self::Int(*v),
            ContextValue::Unsigned(v) => Self::U64(*v),
            ContextValue::Float(v) => Self::Double(*v),
            ContextValue::Bool(v) => Self::Bool(*v),
            ContextValue::StringArray(values) => Self::Array(
                values
                    .iter()
                    .map(|value| OtelValue::String(value.clone().into()))
                    .collect(),
            ),
            ContextValue::IntegerArray(values) => {
                Self::Array(values.iter().copied().map(OtelValue::Int).collect())
            }
            ContextValue::UnsignedArray(values) => {
                Self::Array(values.iter().copied().map(OtelValue::U64).collect())
            }
            ContextValue::FloatArray(values) => {
                Self::Array(values.iter().copied().map(OtelValue::Double).collect())
            }
            ContextValue::BoolArray(values) => {
                Self::Array(values.iter().copied().map(OtelValue::Bool).collect())
            }
            ContextValue::Redacted { kind, reason } => Self::KvList(vec![
                OtelAttribute {
                    key: "kind".into(),
                    value: kind
                        .as_ref()
                        .map(|v| OtelValue::String(v.clone().into()))
                        .unwrap_or(OtelValue::Null),
                },
                OtelAttribute {
                    key: "reason".into(),
                    value: reason
                        .as_ref()
                        .map(|v| OtelValue::String(v.clone().into()))
                        .unwrap_or(OtelValue::Null),
                },
            ]),
        }
    }
}

/// A batch of OpenTelemetry log/event records.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(bound(deserialize = "'de: 'a")))]
pub struct OtelEnvelope<'a> {
    pub records: Vec<OtelEvent<'a>>,
}

pub const REPORT_OTEL_SCHEMA_VERSION: &str = "v0.1.0";

pub const REPORT_OTEL_SCHEMA_DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";

/// Returns the OTEL schema for the diagnostic envelope.
pub fn report_otel_schema() -> &'static str {
    include_str!("../../schemas/report-otel-v0.1.0.schema.json")
}

impl<'a> DiagnosticIr<'a, HasSeverity> {
    /// Converts the diagnostic IR to OpenTelemetry log/event records.
    ///
    /// This API is only available once the diagnostic IR carries an explicit
    /// severity in its typestate.
    pub fn to_otel_envelope(&'a self) -> OtelEnvelope<'a> {
        let mut records = Vec::new();
        records.push(self.otel_report_ev());

        #[cfg(feature = "trace")]
        self.otel_trace_ev(&mut records);

        OtelEnvelope { records }
    }

    fn otel_report_ev(&'a self) -> OtelEvent<'a> {
        let mut attributes = self.otel_base_attrs();
        let report_level = self.metadata.required_severity();

        #[cfg(feature = "trace")]
        self.otel_trace_corr(&mut attributes);
        self.otel_diagnostic_bag(&mut attributes);
        self.otel_attach_attrs(&mut attributes);

        let (trace_id, span_id, trace_flags) = self.otel_trace_ids();

        OtelEvent {
            name: "exception".into(),
            body: Some(otel_value_from_owned(build_error_value(&self.error))),
            timestamp_unix_nano: None,
            observed_timestamp_unix_nano: None,
            severity_text: Some(severity_ref(report_level)),
            severity_number: Some(severity_to_otel_number(report_level)),
            trace_id,
            span_id,
            trace_flags,
            attributes,
        }
    }

    fn otel_error_code_attr(key: &'static str, value: &'a ErrorCode) -> OtelAttribute<'a> {
        OtelAttribute {
            key: key.into(),
            value: error_code_otel_value(value),
        }
    }

    fn otel_base_attrs(&'a self) -> Vec<OtelAttribute<'a>> {
        let mut attributes = Vec::new();
        attributes.push(OtelAttribute {
            key: "exception.type".into(),
            value: OtelValue::String(self.error.r#type.clone()),
        });
        attributes.push(OtelAttribute {
            key: "exception.message".into(),
            value: OtelValue::String(self.error.message.to_string_owned().into()),
        });
        if let Some(stack_trace) = self.metadata.stack_trace() {
            attributes.push(OtelAttribute {
                key: "exception.stacktrace".into(),
                value: otel_value_from_owned(build_stack_trace_value(stack_trace)),
            });
        }
        if let Some(error_code) = self.metadata.error_code() {
            attributes.push(Self::otel_error_code_attr("error.code", error_code));
        }
        if let Some(category) = self.metadata.category() {
            attributes.push(OtelAttribute {
                key: "error.category".into(),
                value: OtelValue::String(category.into()),
            });
        }
        if let Some(retryable) = self.metadata.retryable() {
            attributes.push(OtelAttribute {
                key: "error.retryable".into(),
                value: OtelValue::Bool(retryable),
            });
        }
        attributes
    }

    fn otel_trace_ids(&'a self) -> (Option<RefStr<'a>>, Option<RefStr<'a>>, Option<u8>) {
        #[cfg(feature = "trace")]
        {
            let trace_id = self
                .trace
                .and_then(|trace| trace.context.trace_id.as_ref().map(|v| v.as_ref().into()));
            let span_id = self
                .trace
                .and_then(|trace| trace.context.span_id.as_ref().map(|v| v.as_ref().into()));
            let trace_flags = self
                .trace
                .and_then(|trace| trace.context.flags.map(|flags| flags.bits()));
            (trace_id, span_id, trace_flags)
        }
        #[cfg(not(feature = "trace"))]
        {
            (None, None, None)
        }
    }

    #[cfg(feature = "trace")]
    fn otel_trace_corr(&'a self, attributes: &mut Vec<OtelAttribute<'a>>) {
        let Some(trace) = self.trace else {
            return;
        };
        if let Some(parent_span_id) = trace.context.parent_span_id.as_ref() {
            attributes.push(OtelAttribute {
                key: "trace.parent_span_id".into(),
                value: OtelValue::String(parent_span_id.as_ref().into()),
            });
        }
        if let Some(trace_state) = trace.context.trace_state.as_ref() {
            attributes.push(OtelAttribute {
                key: "trace.state".into(),
                value: OtelValue::String(trace_state.as_str().into()),
            });
        }
    }

    fn otel_diagnostic_bag(&'a self, attributes: &mut Vec<OtelAttribute<'a>>) {
        if !self.display_causes.is_empty() {
            attributes.push(OtelAttribute {
                key: "diagnostic_bag.display_causes".into(),
                value: otel_value_from_owned(build_display_causes(
                    self.display_causes,
                    self.display_causes_state,
                )),
            });
        }
        if let Some(source_errors) = self.origin_source_errors.as_ref() {
            attributes.push(OtelAttribute {
                key: "diagnostic_bag.origin_source_errors".into(),
                value: otel_value_from_owned(build_origin_src_errs_val(source_errors)),
            });
        }
        if let Some(source_errors) = self.diagnostic_source_errors.as_ref() {
            attributes.push(OtelAttribute {
                key: "diagnostic_bag.diagnostic_source_errors".into(),
                value: otel_value_from_owned(build_diag_src_errs_val(source_errors)),
            });
        }
    }

    fn otel_attach_attrs(&'a self, attributes: &mut Vec<OtelAttribute<'a>>) {
        for (key, value) in self.context {
            attributes.push(OtelAttribute {
                key: key.as_ref().into(),
                value: OtelValue::from(value),
            });
        }
        for (key, value) in self.system {
            attributes.push(OtelAttribute {
                key: format!("system.{}", key.as_ref()).into(),
                value: OtelValue::from(value),
            });
        }
        for attachment in self.attachments {
            match attachment {
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
                            value: OtelValue::String(media_type.clone().into()),
                        });
                    }
                }
            }
        }
    }

    #[cfg(feature = "trace")]
    fn otel_trace_ev(&'a self, records: &mut Vec<OtelEvent<'a>>) {
        let trace = match self.trace {
            Some(t) => t,
            None => return,
        };
        let fallback_level = self.metadata.required_severity();
        for trace_event in trace.events.iter() {
            let (severity_text, severity_number) = match trace_event.level {
                Some(level) => {
                    let severity_text = trace_event_level_ref(level);
                    let severity_number = trace_event_level_to_otel_number(level);
                    (Some(severity_text), Some(severity_number))
                }
                None => (
                    Some(severity_ref(fallback_level)),
                    Some(severity_to_otel_number(fallback_level)),
                ),
            };
            let mut attributes = trace_event
                .attributes
                .iter()
                .map(|attr| OtelAttribute {
                    key: attr.key.clone().into(),
                    value: OtelValue::from(&attr.value),
                })
                .collect::<Vec<_>>();
            if let Some(parent_span_id) = trace.context.parent_span_id.as_ref() {
                attributes.push(OtelAttribute {
                    key: "trace.parent_span_id".into(),
                    value: OtelValue::String(parent_span_id.as_ref().into()),
                });
            }
            if let Some(trace_state) = trace.context.trace_state.as_ref() {
                attributes.push(OtelAttribute {
                    key: "trace.state".into(),
                    value: OtelValue::String(trace_state.as_str().into()),
                });
            }
            records.push(OtelEvent {
                name: trace_event.name.clone().into(),
                body: None,
                timestamp_unix_nano: trace_event.timestamp_unix_nano,
                observed_timestamp_unix_nano: None,
                severity_text,
                severity_number,
                trace_id: trace.context.trace_id.as_ref().map(|v| v.as_ref().into()),
                span_id: trace.context.span_id.as_ref().map(|v| v.as_ref().into()),
                trace_flags: trace.context.flags.map(|flags| flags.bits()),
                attributes,
            });
        }
    }
}

fn severity_to_otel_number(level: crate::report::Severity) -> u8 {
    match level {
        crate::report::Severity::Trace => 1,
        crate::report::Severity::Debug => 5,
        crate::report::Severity::Info => 9,
        crate::report::Severity::Warn => 13,
        crate::report::Severity::Error => 17,
        crate::report::Severity::Fatal => 21,
    }
}

#[cfg(feature = "trace")]
fn trace_event_level_to_otel_number(level: crate::report::TraceEventLevel) -> u8 {
    match level {
        crate::report::TraceEventLevel::Trace => 1,
        crate::report::TraceEventLevel::Debug => 5,
        crate::report::TraceEventLevel::Info => 9,
        crate::report::TraceEventLevel::Warn => 13,
        crate::report::TraceEventLevel::Error => 17,
    }
}

#[cfg(feature = "trace")]
fn trace_event_level_ref(level: crate::report::TraceEventLevel) -> ref_str::StaticRefStr {
    match level {
        crate::report::TraceEventLevel::Trace => "trace".into(),
        crate::report::TraceEventLevel::Debug => "debug".into(),
        crate::report::TraceEventLevel::Info => "info".into(),
        crate::report::TraceEventLevel::Warn => "warn".into(),
        crate::report::TraceEventLevel::Error => "error".into(),
    }
}

fn severity_ref(level: crate::report::Severity) -> ref_str::StaticRefStr {
    match level {
        crate::report::Severity::Trace => "trace".into(),
        crate::report::Severity::Debug => "debug".into(),
        crate::report::Severity::Info => "info".into(),
        crate::report::Severity::Warn => "warn".into(),
        crate::report::Severity::Error => "error".into(),
        crate::report::Severity::Fatal => "fatal".into(),
    }
}

fn otel_value_from_owned(value: AttachmentValue) -> OtelValue<'static> {
    match value {
        AttachmentValue::Null => OtelValue::Null,
        AttachmentValue::String(v) => OtelValue::String(v.to_string().into()),
        AttachmentValue::Integer(v) => OtelValue::Int(v),
        AttachmentValue::Unsigned(v) => OtelValue::U64(v),
        AttachmentValue::Float(v) => OtelValue::Double(v),
        AttachmentValue::Bool(v) => OtelValue::Bool(v),
        AttachmentValue::Array(values) => {
            OtelValue::Array(values.into_iter().map(otel_value_from_owned).collect())
        }
        AttachmentValue::Object(values) => OtelValue::KvList(
            values
                .into_sorted_entries()
                .into_iter()
                .map(|(key, value)| OtelAttribute {
                    key: key.into(),
                    value: otel_value_from_owned(value),
                })
                .collect(),
        ),
        AttachmentValue::Bytes(v) => OtelValue::Bytes(v),
        AttachmentValue::Redacted { kind, reason } => OtelValue::KvList(
            [("kind", kind), ("reason", reason)]
                .into_iter()
                .map(|(key, value)| OtelAttribute {
                    key: key.into(),
                    value: value
                        .map(|v| OtelValue::String(v.to_string().into()))
                        .unwrap_or(OtelValue::Null),
                })
                .collect(),
        ),
    }
}
