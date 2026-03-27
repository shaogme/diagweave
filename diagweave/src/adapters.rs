use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
#[cfg(feature = "trace")]
use alloc::format;
use alloc::string::ToString;
#[cfg(feature = "trace")]
use alloc::vec;
use alloc::vec::Vec;

use crate::render_impl::{
    DiagnosticIr, build_context_and_attachments, build_display_causes_value,
    build_source_errors_value, build_stack_trace_value,
};
use crate::report::{Attachment, AttachmentValue, ErrorCode};

fn error_code_value(value: &ErrorCode) -> AttachmentValue {
    match value {
        ErrorCode::Integer(v) => AttachmentValue::Integer(*v),
        ErrorCode::String(v) => AttachmentValue::String(v.clone()),
    }
}

fn error_code_otel_value(value: &ErrorCode) -> OtelValue {
    match value {
        ErrorCode::Integer(v) => OtelValue::Int(*v),
        ErrorCode::String(v) => OtelValue::String(v.clone()),
    }
}

/// A key-value pair for Tracing fields.
#[derive(Debug, Clone, PartialEq)]
pub struct TracingField {
    pub key: Cow<'static, str>,
    pub value: AttachmentValue,
}

/// An attribute for OpenTelemetry.
#[derive(Debug, Clone, PartialEq)]
pub struct OtelAttribute {
    pub key: Cow<'static, str>,
    pub value: OtelValue,
}

/// An event for OpenTelemetry, consisting of a name and attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct OtelEvent {
    pub name: Cow<'static, str>,
    pub attributes: Vec<OtelAttribute>,
}

/// A context key-value pair for OpenTelemetry export.
#[derive(Debug, Clone, PartialEq)]
pub struct OtelContextItem {
    pub key: Cow<'static, str>,
    pub value: OtelValue,
}

/// An attachment for OpenTelemetry export.
#[derive(Debug, Clone, PartialEq)]
pub enum OtelAttachment {
    Note {
        message: Cow<'static, str>,
    },
    Payload {
        name: Cow<'static, str>,
        value: OtelValue,
        media_type: Option<Cow<'static, str>>,
    },
}

/// OTLP-friendly OpenTelemetry value representation.
#[derive(Debug, Clone, PartialEq)]
pub enum OtelValue {
    Null,
    String(Cow<'static, str>),
    Int(i64),
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
            AttachmentValue::Unsigned(v) => {
                if *v <= i64::MAX as u64 {
                    Self::Int(*v as i64)
                } else {
                    // OTLP has no u64; preserve exact data as string.
                    Self::String(v.to_string().into())
                }
            }
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

/// A collection of attributes and events for OpenTelemetry export.
#[derive(Debug, Clone, PartialEq)]
pub struct OtelEnvelope {
    pub attributes: Vec<OtelAttribute>,
    pub events: Vec<OtelEvent>,
    pub context: Vec<OtelContextItem>,
    pub attachments: Vec<OtelAttachment>,
}

impl DiagnosticIr<'_> {
    /// Converts the diagnostic IR to a vector of tracing fields.
    pub fn to_tracing_fields(&self) -> Vec<TracingField> {
        let mut fields = Vec::new();

        self.tracing_error(&mut fields);
        self.tracing_meta(&mut fields);
        self.tracing_stack_trace_and_causes(&mut fields);
        #[cfg(feature = "trace")]
        self.tracing_trace(&mut fields);
        self.tracing_stats(&mut fields);
        self.tracing_context_and_attachments(&mut fields);

        fields
    }

    fn tracing_error(&self, fields: &mut Vec<TracingField>) {
        fields.push(TracingField {
            key: "error.message".into(),
            value: AttachmentValue::String(self.error.message.to_string_owned().into()),
        });
        fields.push(TracingField {
            key: "error.type".into(),
            value: AttachmentValue::String(self.error.r#type.clone().into_owned().into()),
        });
    }

    fn tracing_meta(&self, fields: &mut Vec<TracingField>) {
        if let Some(error_code) = &self.metadata.error_code {
            fields.push(TracingField {
                key: "error.code".into(),
                value: error_code_value(*error_code),
            });
        }
        if let Some(severity) = self.metadata.severity {
            fields.push(TracingField {
                key: "error_severity".into(),
                value: AttachmentValue::String(severity.into()),
            });
        }
        if let Some(category) = self.metadata.category {
            fields.push(TracingField {
                key: "error_category".into(),
                value: AttachmentValue::String((*category).clone()),
            });
        }
        if let Some(retryable) = self.metadata.retryable {
            fields.push(TracingField {
                key: "error_retryable".into(),
                value: AttachmentValue::Bool(retryable),
            });
        }
    }

    #[cfg(feature = "trace")]
    fn tracing_trace(&self, fields: &mut Vec<TracingField>) {
        let trace = match self.trace {
            Some(t) => t,
            None => return,
        };
        if let Some(trace_id) = &trace.context.trace_id {
            fields.push(TracingField {
                key: "trace_id".into(),
                value: AttachmentValue::String(trace_id.clone()),
            });
        }
        if let Some(span_id) = &trace.context.span_id {
            fields.push(TracingField {
                key: "span_id".into(),
                value: AttachmentValue::String(span_id.clone()),
            });
        }
        if let Some(parent_span_id) = &trace.context.parent_span_id {
            fields.push(TracingField {
                key: "parent_span_id".into(),
                value: AttachmentValue::String(parent_span_id.clone()),
            });
        }
        if let Some(sampled) = trace.context.sampled {
            fields.push(TracingField {
                key: "trace_sampled".into(),
                value: AttachmentValue::Bool(sampled),
            });
        }
        if let Some(trace_state) = &trace.context.trace_state {
            fields.push(TracingField {
                key: "trace_state".into(),
                value: AttachmentValue::String(trace_state.clone()),
            });
        }
        if let Some(flags) = trace.context.flags {
            fields.push(TracingField {
                key: "trace_flags".into(),
                value: AttachmentValue::Unsigned(flags as u64),
            });
        }
        fields.push(TracingField {
            key: "trace_event_count".into(),
            value: AttachmentValue::Unsigned(trace.events.len() as u64),
        });

        self.tracing_trace_events(fields);
    }

    #[cfg(feature = "trace")]
    fn tracing_trace_events(&self, fields: &mut Vec<TracingField>) {
        let trace = match self.trace {
            Some(t) => t,
            None => return,
        };
        for (idx, event) in trace.events.iter().enumerate() {
            fields.push(TracingField {
                key: format!("trace_event.{idx}.name").into(),
                value: AttachmentValue::String(event.name.clone()),
            });
            if let Some(level) = event.level {
                fields.push(TracingField {
                    key: format!("trace_event.{idx}.level").into(),
                    value: AttachmentValue::String(level.into()),
                });
            }
            if let Some(ts) = event.timestamp_unix_nano {
                fields.push(TracingField {
                    key: format!("trace_event.{idx}.timestamp_unix_nano").into(),
                    value: AttachmentValue::Unsigned(ts),
                });
            }
            for attr in &event.attributes {
                fields.push(TracingField {
                    key: format!("trace_event.{idx}.attr.{}", attr.key).into(),
                    value: attr.value.clone(),
                });
            }
        }
    }

    fn tracing_stats(&self, fields: &mut Vec<TracingField>) {
        fields.push(TracingField {
            key: "report_context_count".into(),
            value: AttachmentValue::Unsigned(self.context_count as u64),
        });
        fields.push(TracingField {
            key: "report_attachment_count".into(),
            value: AttachmentValue::Unsigned(self.attachment_count as u64),
        });
    }

    fn tracing_stack_trace_and_causes(&self, fields: &mut Vec<TracingField>) {
        if let Some(stack_trace) = &self.metadata.stack_trace {
            fields.push(TracingField {
                key: "report_stack_trace".into(),
                value: build_stack_trace_value(stack_trace),
            });
        }
        if !self.display_causes.is_empty() {
            fields.push(TracingField {
                key: "report_display_causes".into(),
                value: build_display_causes_value(
                    self.display_causes,
                    self.display_causes_state,
                ),
            });
        }
        if !self.source_errors.is_empty() {
            fields.push(TracingField {
                key: "report_source_errors".into(),
                value: build_source_errors_value(&self.source_errors, self.source_errors_state),
            });
        }
    }

    fn tracing_context_and_attachments(&self, fields: &mut Vec<TracingField>) {
        let (context_items, attachment_items) = build_context_and_attachments(self.attachments);

        fields.push(TracingField {
            key: "report_context".into(),
            value: AttachmentValue::Array(context_items),
        });
        fields.push(TracingField {
            key: "report_attachments".into(),
            value: AttachmentValue::Array(attachment_items),
        });
    }

    /// Converts the diagnostic IR to an OpenTelemetry envelope.
    pub fn to_otel_envelope(&self) -> OtelEnvelope {
        let mut attributes = Vec::new();
        let mut context = Vec::new();
        let mut attachments = Vec::new();
        #[cfg(feature = "trace")]
        let mut events = Vec::new();
        #[cfg(not(feature = "trace"))]
        let events = Vec::new();

        self.otel_error(&mut attributes);
        self.otel_meta(&mut attributes);
        self.otel_stack_trace_and_causes(&mut attributes);
        self.otel_stats(&mut attributes);
        #[cfg(feature = "trace")]
        self.otel_trace(&mut attributes);
        #[cfg(feature = "trace")]
        self.otel_trace_ev(&mut events);
        self.otel_context_and_attachments(&mut context, &mut attachments);

        OtelEnvelope {
            attributes,
            events,
            context,
            attachments,
        }
    }

    fn otel_error(&self, attributes: &mut Vec<OtelAttribute>) {
        attributes.push(OtelAttribute {
            key: "error.message".into(),
            value: OtelValue::String(self.error.message.to_string_owned().into()),
        });
        attributes.push(OtelAttribute {
            key: "error.type".into(),
            value: OtelValue::String(self.error.r#type.clone().into_owned().into()),
        });
    }

    fn otel_meta(&self, attributes: &mut Vec<OtelAttribute>) {
        if let Some(error_code) = &self.metadata.error_code {
            attributes.push(OtelAttribute {
                key: "error_code".into(),
                value: error_code_otel_value(*error_code),
            });
        }
        if let Some(severity) = self.metadata.severity {
            attributes.push(OtelAttribute {
                key: "error_severity".into(),
                value: OtelValue::String(Cow::from(severity)),
            });
        }
        if let Some(category) = self.metadata.category {
            attributes.push(OtelAttribute {
                key: "error_category".into(),
                value: OtelValue::String((*category).clone()),
            });
        }
        if let Some(retryable) = self.metadata.retryable {
            attributes.push(OtelAttribute {
                key: "error_retryable".into(),
                value: OtelValue::Bool(retryable),
            });
        }
    }

    fn otel_stats(&self, attributes: &mut Vec<OtelAttribute>) {
        attributes.push(OtelAttribute {
            key: "report_context_count".into(),
            value: OtelValue::Int(self.context_count as i64),
        });
        attributes.push(OtelAttribute {
            key: "report_attachment_count".into(),
            value: OtelValue::Int(self.attachment_count as i64),
        });
        #[cfg(feature = "trace")]
        if let Some(trace) = self.trace {
            attributes.push(OtelAttribute {
                key: "trace_event_count".into(),
                value: OtelValue::Int(trace.events.len() as i64),
            });
        }
    }

    fn otel_stack_trace_and_causes(&self, attributes: &mut Vec<OtelAttribute>) {
        if let Some(stack_trace) = &self.metadata.stack_trace {
            attributes.push(OtelAttribute {
                key: "report_stack_trace".into(),
                value: OtelValue::from(&build_stack_trace_value(stack_trace)),
            });
        }
        if !self.display_causes.is_empty() {
            attributes.push(OtelAttribute {
                key: "report_display_causes".into(),
                value: OtelValue::from(&build_display_causes_value(
                    self.display_causes,
                    self.display_causes_state,
                )),
            });
        }
        if !self.source_errors.is_empty() {
            attributes.push(OtelAttribute {
                key: "report_source_errors".into(),
                value: OtelValue::from(&build_source_errors_value(
                    &self.source_errors,
                    self.source_errors_state,
                )),
            });
        }
    }

    #[cfg(feature = "trace")]
    fn otel_trace(&self, attributes: &mut Vec<OtelAttribute>) {
        let trace = match self.trace {
            Some(t) => t,
            None => return,
        };
        if let Some(trace_id) = &trace.context.trace_id {
            attributes.push(OtelAttribute {
                key: "trace_id".into(),
                value: OtelValue::String(trace_id.clone()),
            });
        }
        if let Some(span_id) = &trace.context.span_id {
            attributes.push(OtelAttribute {
                key: "span_id".into(),
                value: OtelValue::String(span_id.clone()),
            });
        }
        if let Some(parent_span_id) = &trace.context.parent_span_id {
            attributes.push(OtelAttribute {
                key: "parent_span_id".into(),
                value: OtelValue::String(parent_span_id.clone()),
            });
        }
        if let Some(sampled) = trace.context.sampled {
            attributes.push(OtelAttribute {
                key: "trace_sampled".into(),
                value: OtelValue::Bool(sampled),
            });
        }
        if let Some(trace_state) = &trace.context.trace_state {
            attributes.push(OtelAttribute {
                key: "trace_state".into(),
                value: OtelValue::String(trace_state.clone()),
            });
        }
        if let Some(flags) = trace.context.flags {
            attributes.push(OtelAttribute {
                key: "trace_flags".into(),
                value: OtelValue::Int(flags as i64),
            });
        }
    }

    #[cfg(feature = "trace")]
    fn otel_trace_ev(&self, events: &mut Vec<OtelEvent>) {
        let trace = match self.trace {
            Some(t) => t,
            None => return,
        };
        for (idx, trace_event) in trace.events.iter().enumerate() {
            let mut event_attributes = vec![
                OtelAttribute {
                    key: "trace_event.index".into(),
                    value: OtelValue::Int(idx as i64),
                },
                OtelAttribute {
                    key: "trace_event.name".into(),
                    value: OtelValue::String(trace_event.name.clone()),
                },
            ];
            if let Some(level) = trace_event.level {
                event_attributes.push(OtelAttribute {
                    key: "trace_event.level".into(),
                    value: OtelValue::String(level.into()),
                });
            }
            if let Some(ts) = trace_event.timestamp_unix_nano {
                event_attributes.push(OtelAttribute {
                    key: "trace_event.timestamp_unix_nano".into(),
                    value: OtelValue::Int(if ts <= i64::MAX as u64 {
                        ts as i64
                    } else {
                        i64::MAX
                    }),
                });
            }
            for attr in &trace_event.attributes {
                event_attributes.push(OtelAttribute {
                    key: format!("trace_event.attr.{}", attr.key).into(),
                    value: OtelValue::from(&attr.value),
                });
            }
            events.push(OtelEvent {
                name: "trace.event".into(),
                attributes: event_attributes,
            });
        }
    }

    fn otel_context_and_attachments(
        &self,
        context: &mut Vec<OtelContextItem>,
        attachments: &mut Vec<OtelAttachment>,
    ) {
        for attachment in self.attachments {
            match attachment {
                Attachment::Context { key, value } => {
                    context.push(OtelContextItem {
                        key: key.clone(),
                        value: OtelValue::from(value),
                    });
                }
                Attachment::Note { message } => {
                    attachments.push(OtelAttachment::Note {
                        message: message.to_string().into(),
                    });
                }
                Attachment::Payload {
                    name,
                    value,
                    media_type,
                } => {
                    attachments.push(OtelAttachment::Payload {
                        name: name.clone(),
                        value: OtelValue::from(value),
                        media_type: media_type.clone(),
                    });
                }
            }
        }
    }
}
