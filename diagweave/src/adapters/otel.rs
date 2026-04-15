use alloc::borrow::Cow;
use alloc::format;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use core::convert::TryFrom;
use core::fmt;
use core::fmt::Display;
use ref_str::RefStr;

use crate::render_impl::DiagnosticIr;
use crate::report::{
    Attachment, AttachmentValue, CauseTraversalState, ContextValue, ErrorCode, HasSeverity,
    SourceErrorChain, SpanId, StackTrace, TraceId,
};

/// Severity numbers allowed by the OTLP log data model.
///
/// This wrapper prevents accidental emission of values outside the OTEL
/// severity range and validates deserialization against the schema contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(try_from = "u8", into = "u8"))]
pub struct OtelSeverityNumber(u8);

impl OtelSeverityNumber {
    pub const TRACE: Self = Self(1);
    pub const DEBUG: Self = Self(5);
    pub const INFO: Self = Self(9);
    pub const WARN: Self = Self(13);
    pub const ERROR: Self = Self(17);
    pub const FATAL: Self = Self(21);

    pub const fn as_u8(self) -> u8 {
        self.0
    }
}

impl From<OtelSeverityNumber> for u8 {
    fn from(value: OtelSeverityNumber) -> Self {
        value.0
    }
}

impl TryFrom<u8> for OtelSeverityNumber {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 | 5 | 9 | 13 | 17 | 21 => Ok(Self(value)),
            _ => Err("severity number must be one of 1, 5, 9, 13, 17, or 21"),
        }
    }
}

impl From<crate::report::Severity> for OtelSeverityNumber {
    fn from(level: crate::report::Severity) -> Self {
        match level {
            crate::report::Severity::Trace => Self::TRACE,
            crate::report::Severity::Debug => Self::DEBUG,
            crate::report::Severity::Info => Self::INFO,
            crate::report::Severity::Warn => Self::WARN,
            crate::report::Severity::Error => Self::ERROR,
            crate::report::Severity::Fatal => Self::FATAL,
        }
    }
}

#[cfg(feature = "trace")]
impl From<crate::report::TraceEventLevel> for OtelSeverityNumber {
    fn from(level: crate::report::TraceEventLevel) -> Self {
        match level {
            crate::report::TraceEventLevel::Trace => Self::TRACE,
            crate::report::TraceEventLevel::Debug => Self::DEBUG,
            crate::report::TraceEventLevel::Info => Self::INFO,
            crate::report::TraceEventLevel::Warn => Self::WARN,
            crate::report::TraceEventLevel::Error => Self::ERROR,
        }
    }
}

impl fmt::Display for OtelSeverityNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
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

/// Naming configuration for diagweave-owned OTEL keys.
///
/// When `namespace` is `None`, the current compatibility naming is preserved.
/// When it is set, all diagweave-owned keys are emitted beneath that prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(bound(deserialize = "'de: 'a")))]
pub struct OtelEnvelopeConfig<'a> {
    namespace: Option<RefStr<'a>>,
}

impl<'a> OtelEnvelopeConfig<'a> {
    /// Creates a config that preserves the compatibility naming behavior.
    pub const fn new() -> Self {
        Self { namespace: None }
    }

    /// Sets the root namespace used for diagweave-owned OTEL keys.
    pub fn with_namespace(mut self, namespace: impl Into<RefStr<'a>>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    /// Returns the configured namespace, if any.
    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_ref().map(RefStr::as_ref)
    }

    fn namespace_ref(&self) -> Option<&str> {
        self.namespace.as_ref().map(RefStr::as_ref)
    }

    fn context_key(&self, key: &'a str) -> RefStr<'a> {
        match self.namespace_ref() {
            Some(namespace) => format!("{namespace}.context.{key}").into(),
            None => key.into(),
        }
    }

    fn system_key(&self, key: &str) -> RefStr<'a> {
        match self.namespace_ref() {
            Some(namespace) => format!("{namespace}.system.{key}").into(),
            None => format!("system.{key}").into(),
        }
    }

    fn diagnostic_bag_key(&self, key: &str) -> RefStr<'a> {
        match self.namespace_ref() {
            Some(namespace) => format!("{namespace}.diagnostic_bag.{key}").into(),
            None => format!("diagnostic_bag.{key}").into(),
        }
    }

    fn attachment_key(&self, key: &str) -> RefStr<'a> {
        match self.namespace_ref() {
            Some(namespace) => format!("{namespace}.attachment.{key}").into(),
            None => format!("attachment.{key}").into(),
        }
    }
}

impl Default for OtelEnvelopeConfig<'_> {
    fn default() -> Self {
        Self::new()
    }
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
    pub severity_number: Option<OtelSeverityNumber>,
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub trace_id: Option<TraceId>,
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub span_id: Option<SpanId>,
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub trace_flags: Option<u8>,
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub trace_context: Option<OtelTraceContext>,
    pub attributes: Vec<OtelAttribute<'a>>,
}

/// Trace-context metadata carried alongside an OTEL log/event record.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct OtelTraceContext {
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub parent_span_id: Option<crate::report::ParentSpanId>,
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "Option::is_none"))]
    pub trace_state: Option<crate::report::TraceState>,
}

/// OTLP-friendly OpenTelemetry value representation.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(bound(deserialize = "'de: 'a")))]
pub enum OtelValue<'a> {
    String(RefStr<'a>),
    Int(i64),
    U64(u64),
    Double(f64),
    Bool(bool),
    Bytes(Vec<u8>),
    Array(Vec<OtelValue<'a>>),
    KvList(Vec<OtelAttribute<'a>>),
}

impl<'a> OtelValue<'a> {
    /// Returns a cow representation for debugging and examples.
    pub fn as_cow(&self) -> Cow<'_, str> {
        match self {
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

    pub fn from_attachment(value: AttachmentValue) -> Self {
        match value {
            AttachmentValue::String(v) => OtelValue::String(v.into()),
            AttachmentValue::Integer(v) => OtelValue::Int(v),
            AttachmentValue::Unsigned(v) => OtelValue::U64(v),
            AttachmentValue::Float(v) => OtelValue::Double(v),
            AttachmentValue::Bool(v) => OtelValue::Bool(v),
            AttachmentValue::Array(values) => {
                OtelValue::Array(values.into_iter().map(OtelValue::from_attachment).collect())
            }
            AttachmentValue::Object(values) => OtelValue::KvList(
                values
                    .into_iter()
                    .map(|(key, value)| OtelAttribute {
                        key: key.into(),
                        value: Self::from_attachment(value),
                    })
                    .collect(),
            ),
            AttachmentValue::Bytes(v) => OtelValue::Bytes(v),
            AttachmentValue::Redacted { kind, reason } => {
                let mut attrs = Vec::with_capacity(2);
                if let Some(kind) = kind {
                    attrs.push(OtelAttribute {
                        key: "kind".into(),
                        value: OtelValue::String(kind.into()),
                    });
                }
                if let Some(reason) = reason {
                    attrs.push(OtelAttribute {
                        key: "reason".into(),
                        value: OtelValue::String(reason.into()),
                    });
                }
                OtelValue::KvList(attrs)
            }
        }
    }

    pub fn from_attachment_ref(value: &'a AttachmentValue) -> Self {
        match value {
            AttachmentValue::String(v) => Self::String(v.as_str().into()),
            AttachmentValue::Integer(v) => Self::Int(*v),
            AttachmentValue::Unsigned(v) => Self::U64(*v),
            AttachmentValue::Float(v) => Self::Double(*v),
            AttachmentValue::Bool(v) => Self::Bool(*v),
            AttachmentValue::Array(values) => {
                Self::Array(values.iter().map(OtelValue::from_attachment_ref).collect())
            }
            AttachmentValue::Object(values) => Self::KvList(
                values
                    .iter()
                    .map(|(k, v)| OtelAttribute {
                        key: k.as_str().into(),
                        value: Self::from_attachment_ref(v),
                    })
                    .collect(),
            ),
            AttachmentValue::Bytes(v) => Self::Bytes(v.clone()),
            AttachmentValue::Redacted { kind, reason } => {
                Self::KvList(redacted_attrs(kind.as_ref(), reason.as_ref()))
            }
        }
    }

    pub fn from_context_ref(value: &'a ContextValue) -> Self {
        match value {
            ContextValue::String(v) => Self::String(v.as_str().into()),
            ContextValue::Integer(v) => Self::Int(*v),
            ContextValue::Unsigned(v) => Self::U64(*v),
            ContextValue::Float(v) => Self::Double(*v),
            ContextValue::Bool(v) => Self::Bool(*v),
            ContextValue::StringArray(values) => Self::Array(
                values
                    .iter()
                    .map(|value| OtelValue::String(value.as_str().into()))
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
            ContextValue::Redacted { kind, reason } => {
                Self::KvList(redacted_attrs(kind.as_ref(), reason.as_ref()))
            }
        }
    }
}

impl core::fmt::Display for OtelValue<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_cow().as_ref())
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
    pub fn to_otel_envelope(&'a self, config: OtelEnvelopeConfig<'a>) -> OtelEnvelope<'a> {
        let trace_ids = self.otel_trace_ids();
        let trace_context = self.otel_trace_context();

        #[cfg(feature = "trace")]
        {
            let trace_event_count = self.trace.events().map_or(0, |events| events.len());
            let mut records = Vec::with_capacity(1 + trace_event_count);
            records.push(self.otel_report_ev(&config, trace_ids.clone(), trace_context.clone()));
            self.otel_trace_ev(&config, &mut records, trace_ids, trace_context);

            OtelEnvelope { records }
        }

        #[cfg(not(feature = "trace"))]
        {
            let mut records = Vec::with_capacity(1);
            records.push(self.otel_report_ev(&config, trace_ids, trace_context));

            OtelEnvelope { records }
        }
    }

    #[cfg(feature = "trace")]
    pub fn to_otel_envelope_default(&'a self) -> OtelEnvelope<'a> {
        self.to_otel_envelope(OtelEnvelopeConfig::default())
    }

    #[cfg(not(feature = "trace"))]
    pub fn to_otel_envelope_default(&'a self) -> OtelEnvelope<'a> {
        self.to_otel_envelope(OtelEnvelopeConfig::default())
    }

    #[cfg(feature = "trace")]
    fn otel_report_ev(
        &'a self,
        config: &OtelEnvelopeConfig<'a>,
        trace_ids: (Option<TraceId>, Option<SpanId>, Option<u8>),
        trace_context: Option<OtelTraceContext>,
    ) -> OtelEvent<'a> {
        let report_level = self.metadata.required_severity();
        let severity_text = severity_ref(report_level);
        let severity_number = report_level.into();
        let error_message: RefStr<'a> = self.error.message.as_cow().into();
        let error_message_value = OtelValue::String(error_message.clone());
        let error_message_raw_data = error_message_value.clone();
        let error_type = self.error.r#type.clone();
        let error_type_value = OtelValue::String(error_type.clone());
        let mut attributes = Vec::with_capacity(self.otel_report_attr_capacity());

        self.otel_diagnostic_bag(config, &mut attributes);
        self.otel_attach_attrs(config, &mut attributes);
        let (trace_id, span_id, trace_flags) = trace_ids;

        OtelEvent {
            name: "exception".into(),
            body: Some(error_message_value.clone()),
            timestamp_unix_nano: None,
            observed_timestamp_unix_nano: None,
            severity_text: Some(severity_text),
            severity_number: Some(severity_number),
            trace_id,
            span_id,
            trace_flags,
            trace_context,
            attributes: {
                attributes.push(OtelAttribute {
                    key: "exception.type".into(),
                    value: error_type_value,
                });
                attributes.push(OtelAttribute {
                    key: "exception.message".into(),
                    value: error_message_value,
                });
                attributes.push(OtelAttribute {
                    key: "exception.raw_data".into(),
                    value: otel_error_raw_data(error_message_raw_data, error_type.clone()),
                });
                if let Some(stack_trace) = self.metadata.stack_trace() {
                    attributes.push(OtelAttribute {
                        key: "exception.stacktrace".into(),
                        value: otel_stack_trace_value(stack_trace),
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
            },
        }
    }

    #[cfg(not(feature = "trace"))]
    fn otel_report_ev(
        &'a self,
        config: &OtelEnvelopeConfig<'a>,
        trace_ids: (Option<TraceId>, Option<SpanId>, Option<u8>),
        trace_context: Option<OtelTraceContext>,
    ) -> OtelEvent<'a> {
        let report_level = self.metadata.required_severity();
        let severity_text = severity_ref(report_level);
        let severity_number = report_level.into();
        let error_message: RefStr<'a> = self.error.message.as_cow().into();
        let error_message_value = OtelValue::String(error_message.clone());
        let error_message_raw_data = error_message_value.clone();
        let error_type = self.error.r#type.clone();
        let error_type_value = OtelValue::String(error_type.clone());
        let mut attributes = Vec::with_capacity(self.otel_report_attr_capacity());
        let (trace_id, span_id, trace_flags) = trace_ids;

        self.otel_diagnostic_bag(config, &mut attributes);
        self.otel_attach_attrs(config, &mut attributes);
        attributes.push(OtelAttribute {
            key: "exception.type".into(),
            value: error_type_value,
        });
        attributes.push(OtelAttribute {
            key: "exception.message".into(),
            value: error_message_value,
        });
        attributes.push(OtelAttribute {
            key: "exception.raw_data".into(),
            value: otel_error_raw_data(error_message_raw_data, error_type.clone()),
        });
        if let Some(stack_trace) = self.metadata.stack_trace() {
            attributes.push(OtelAttribute {
                key: "exception.stacktrace".into(),
                value: otel_stack_trace_value(stack_trace),
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

        OtelEvent {
            name: "exception".into(),
            body: Some(error_message_value.clone()),
            timestamp_unix_nano: None,
            observed_timestamp_unix_nano: None,
            severity_text: Some(severity_text),
            severity_number: Some(severity_number),
            trace_id,
            span_id,
            trace_flags,
            trace_context,
            attributes,
        }
    }

    fn otel_error_code_attr(key: &'static str, value: &'a ErrorCode) -> OtelAttribute<'a> {
        OtelAttribute {
            key: key.into(),
            value: match value {
                ErrorCode::Integer(v) => OtelValue::Int(*v),
                ErrorCode::String(v) => OtelValue::String(v.as_str().into()),
            },
        }
    }

    fn otel_report_attr_capacity(&'a self) -> usize {
        let mut count = 3;
        if self.metadata.stack_trace().is_some() {
            count += 1;
        }
        if self.metadata.error_code().is_some() {
            count += 1;
        }
        if self.metadata.category().is_some() {
            count += 1;
        }
        if self.metadata.retryable().is_some() {
            count += 1;
        }
        if !self.display_causes.is_empty() {
            count += 1;
        }
        if self.origin_source_errors.is_some() {
            count += 1;
        }
        if self.diagnostic_source_errors.is_some() {
            count += 1;
        }
        count += self.context_count;
        count += self.system_count;
        count += self
            .attachments
            .iter()
            .map(|attachment| match attachment {
                Attachment::Note { .. } => 1,
                Attachment::Payload { media_type, .. } => 1 + usize::from(media_type.is_some()),
            })
            .sum::<usize>();
        count
    }

    #[cfg(feature = "trace")]
    fn otel_trace_ids(&'a self) -> (Option<TraceId>, Option<SpanId>, Option<u8>) {
        let trace_context = self.trace.context();
        let trace_id = trace_context.and_then(|ctx| ctx.trace_id.clone());
        let span_id = trace_context.and_then(|ctx| ctx.span_id.clone());
        let trace_flags = trace_context.and_then(|ctx| ctx.flags.map(|flags| flags.bits()));
        (trace_id, span_id, trace_flags)
    }

    #[cfg(not(feature = "trace"))]
    fn otel_trace_ids(&'a self) -> (Option<TraceId>, Option<SpanId>, Option<u8>) {
        (None, None, None)
    }

    #[cfg(feature = "trace")]
    fn otel_trace_context(&'a self) -> Option<OtelTraceContext> {
        let context = self.trace.context()?;
        let parent_span_id = context.parent_span_id.clone();
        let trace_state = context.trace_state.clone();
        if parent_span_id.is_none() && trace_state.is_none() {
            return None;
        }
        Some(OtelTraceContext {
            parent_span_id,
            trace_state,
        })
    }

    #[cfg(not(feature = "trace"))]
    fn otel_trace_context(&'a self) -> Option<OtelTraceContext> {
        None
    }

    fn otel_diagnostic_bag(
        &'a self,
        config: &OtelEnvelopeConfig<'a>,
        attributes: &mut Vec<OtelAttribute<'a>>,
    ) {
        if !self.display_causes.is_empty() {
            attributes.push(OtelAttribute {
                key: config.diagnostic_bag_key("display_causes"),
                value: otel_display_causes_value(self.display_causes, self.display_causes_state),
            });
        }
        if let Some(source_errors) = self.origin_source_errors.as_ref() {
            attributes.push(OtelAttribute {
                key: config.diagnostic_bag_key("origin_source_errors"),
                value: otel_source_errors_value(source_errors, true),
            });
        }
        if let Some(source_errors) = self.diagnostic_source_errors.as_ref() {
            attributes.push(OtelAttribute {
                key: config.diagnostic_bag_key("diagnostic_source_errors"),
                value: otel_source_errors_value(source_errors, false),
            });
        }
    }

    fn otel_attach_attrs(
        &'a self,
        config: &OtelEnvelopeConfig<'a>,
        attributes: &mut Vec<OtelAttribute<'a>>,
    ) {
        for (key, value) in self.context {
            attributes.push(OtelAttribute {
                key: config.context_key(key.as_ref()),
                value: OtelValue::from_context_ref(value),
            });
        }
        for (key, value) in self.system {
            attributes.push(OtelAttribute {
                key: config.system_key(key.as_ref()),
                value: OtelValue::from_context_ref(value),
            });
        }
        for attachment in self.attachments {
            match attachment {
                Attachment::Note { message } => {
                    attributes.push(OtelAttribute {
                        key: config.attachment_key("note"),
                        value: OtelValue::String(message.to_string().into()),
                    });
                }
                Attachment::Payload {
                    name,
                    value,
                    media_type,
                } => {
                    attributes.push(OtelAttribute {
                        key: config.attachment_key(&format!("payload.{name}")),
                        value: OtelValue::from_attachment_ref(value),
                    });
                    if let Some(media_type) = media_type {
                        attributes.push(OtelAttribute {
                            key: config.attachment_key(&format!("payload.{name}.media_type")),
                            value: OtelValue::String(media_type.as_str().into()),
                        });
                    }
                }
            }
        }
    }

    #[cfg(feature = "trace")]
    fn otel_trace_ev(
        &'a self,
        _config: &OtelEnvelopeConfig<'a>,
        records: &mut Vec<OtelEvent<'a>>,
        trace_ids: (Option<TraceId>, Option<SpanId>, Option<u8>),
        trace_context: Option<OtelTraceContext>,
    ) {
        let Some(events) = self.trace.events() else {
            return;
        };
        let fallback_level = self.metadata.required_severity();
        let fallback_severity_text = severity_ref(fallback_level);
        let fallback_severity_number = fallback_level.into();
        for trace_event in events.iter() {
            let (severity_text, severity_number) = trace_event.level.map_or(
                (
                    Some(fallback_severity_text.clone()),
                    Some(fallback_severity_number),
                ),
                |level| (Some(trace_event_level_ref(level)), Some(level.into())),
            );
            let mut attributes = Vec::with_capacity(trace_event.attributes.len());
            attributes.extend(trace_event.attributes.iter().map(|attr| OtelAttribute {
                key: attr.key.as_str().into(),
                value: OtelValue::from_attachment_ref(&attr.value),
            }));
            records.push(OtelEvent {
                name: trace_event.name.as_str().into(),
                body: None,
                timestamp_unix_nano: trace_event.timestamp_unix_nano,
                observed_timestamp_unix_nano: None,
                severity_text,
                severity_number,
                trace_id: trace_ids.0.clone(),
                span_id: trace_ids.1.clone(),
                trace_flags: trace_ids.2,
                trace_context: trace_context.clone(),
                attributes,
            });
        }
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

fn otel_error_raw_data<'a>(message: OtelValue<'a>, error_type: RefStr<'a>) -> OtelValue<'a> {
    OtelValue::KvList(vec![
        OtelAttribute {
            key: "message".into(),
            value: message,
        },
        OtelAttribute {
            key: "type".into(),
            value: OtelValue::String(error_type),
        },
    ])
}

fn redacted_attrs<'a>(
    kind: Option<&'a ref_str::StaticRefStr>,
    reason: Option<&'a ref_str::StaticRefStr>,
) -> Vec<OtelAttribute<'a>> {
    let mut attrs = Vec::with_capacity(2);
    if let Some(kind) = kind {
        attrs.push(OtelAttribute {
            key: "kind".into(),
            value: OtelValue::String(kind.as_str().into()),
        });
    }
    if let Some(reason) = reason {
        attrs.push(OtelAttribute {
            key: "reason".into(),
            value: OtelValue::String(reason.as_str().into()),
        });
    }
    attrs
}

fn otel_stack_trace_value<'a>(stack_trace: &'a StackTrace) -> OtelValue<'a> {
    let format = match stack_trace.format {
        crate::report::StackTraceFormat::Native => "native",
        crate::report::StackTraceFormat::Raw => "raw",
    };
    let frames = stack_trace
        .frames
        .iter()
        .map(|frame| {
            let mut attrs = Vec::with_capacity(5);
            if let Some(symbol) = frame.symbol.as_ref() {
                attrs.push(OtelAttribute {
                    key: "symbol".into(),
                    value: OtelValue::String(symbol.as_str().into()),
                });
            }
            if let Some(module_path) = frame.module_path.as_ref() {
                attrs.push(OtelAttribute {
                    key: "module_path".into(),
                    value: OtelValue::String(module_path.as_str().into()),
                });
            }
            if let Some(file) = frame.file.as_ref() {
                attrs.push(OtelAttribute {
                    key: "file".into(),
                    value: OtelValue::String(file.as_str().into()),
                });
            }
            if let Some(line) = frame.line {
                attrs.push(OtelAttribute {
                    key: "line".into(),
                    value: OtelValue::U64(line as u64),
                });
            }
            if let Some(column) = frame.column {
                attrs.push(OtelAttribute {
                    key: "column".into(),
                    value: OtelValue::U64(column as u64),
                });
            }
            OtelValue::KvList(attrs)
        })
        .collect();
    let mut attrs = vec![
        OtelAttribute {
            key: "format".into(),
            value: OtelValue::String(format.into()),
        },
        OtelAttribute {
            key: "frames".into(),
            value: OtelValue::Array(frames),
        },
    ];
    if let Some(raw) = stack_trace.raw.as_ref() {
        attrs.push(OtelAttribute {
            key: "raw".into(),
            value: OtelValue::String(raw.as_str().into()),
        });
    }
    OtelValue::KvList(attrs)
}

fn otel_display_causes_value<'a>(
    display_causes: &'a [alloc::sync::Arc<dyn Display + Send + Sync + 'static>],
    state: CauseTraversalState,
) -> OtelValue<'a> {
    OtelValue::KvList(vec![
        OtelAttribute {
            key: "items".into(),
            value: OtelValue::Array(
                display_causes
                    .iter()
                    .map(|v| OtelValue::String(v.to_string().into()))
                    .collect(),
            ),
        },
        OtelAttribute {
            key: "truncated".into(),
            value: OtelValue::Bool(state.truncated),
        },
        OtelAttribute {
            key: "cycle_detected".into(),
            value: OtelValue::Bool(state.cycle_detected),
        },
    ])
}

fn otel_source_errors_value<'a>(
    source_errors: &'a SourceErrorChain,
    hide_report_wrapper_types: bool,
) -> OtelValue<'a> {
    let exported = source_errors.export_with_options(hide_report_wrapper_types);
    let nodes = exported
        .nodes
        .into_iter()
        .map(|node| {
            let mut attrs = Vec::with_capacity(3);
            attrs.push(OtelAttribute {
                key: "message".into(),
                value: OtelValue::String(node.message.into()),
            });
            if let Some(type_name) = node.type_name.as_ref() {
                attrs.push(OtelAttribute {
                    key: "type".into(),
                    value: OtelValue::String(type_name.clone().into()),
                });
            }
            attrs.push(OtelAttribute {
                key: "source_roots".into(),
                value: OtelValue::Array(
                    node.source_roots
                        .iter()
                        .copied()
                        .map(|id| OtelValue::Int(id as i64))
                        .collect(),
                ),
            });
            OtelValue::KvList(attrs)
        })
        .collect();
    OtelValue::KvList(vec![
        OtelAttribute {
            key: "roots".into(),
            value: OtelValue::Array(
                exported
                    .roots
                    .iter()
                    .copied()
                    .map(|id| OtelValue::Int(id as i64))
                    .collect(),
            ),
        },
        OtelAttribute {
            key: "nodes".into(),
            value: OtelValue::Array(nodes),
        },
        OtelAttribute {
            key: "truncated".into(),
            value: OtelValue::Bool(exported.truncated),
        },
        OtelAttribute {
            key: "cycle_detected".into(),
            value: OtelValue::Bool(exported.cycle_detected),
        },
    ])
}
