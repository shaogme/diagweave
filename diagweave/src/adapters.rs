use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
#[cfg(feature = "trace")]
use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;

use crate::render_impl::{
    DiagnosticIr, DiagnosticIrError, build_context_and_attachments, build_display_causes_value,
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
            AttachmentValue::Unsigned(v) => {
                Self::U64(*v)
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
    #[cfg(feature = "trace")]
    pub trace_context: Option<OtelTraceContext>,
}

/// Trace context for OTLP span/log correlation.
#[cfg(feature = "trace")]
#[derive(Debug, Clone, PartialEq)]
pub struct OtelTraceContext {
    pub trace_id: Option<Cow<'static, str>>,
    pub span_id: Option<Cow<'static, str>>,
    pub parent_span_id: Option<Cow<'static, str>>,
    pub sampled: Option<bool>,
    pub trace_state: Option<Cow<'static, str>>,
    pub flags: Option<u32>,
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
        self.tracing_context_and_attachments(&mut fields);

        fields
    }

    fn tracing_error(&self, fields: &mut Vec<TracingField>) {
        fields.push(TracingField {
            key: "error".into(),
            value: build_error_value(&self.error),
        });
    }

    fn tracing_meta(&self, fields: &mut Vec<TracingField>) {
        if let Some(error_code) = &self.metadata.error_code {
            fields.push(TracingField {
                key: "metadata.error_code".into(),
                value: error_code_value(*error_code),
            });
        }
        if let Some(severity) = self.metadata.severity {
            fields.push(TracingField {
                key: "metadata.severity".into(),
                value: AttachmentValue::String(severity.into()),
            });
        }
        if let Some(category) = self.metadata.category {
            fields.push(TracingField {
                key: "metadata.category".into(),
                value: AttachmentValue::String((*category).clone()),
            });
        }
        if let Some(retryable) = self.metadata.retryable {
            fields.push(TracingField {
                key: "metadata.retryable".into(),
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
        let trace_value = build_trace_value(trace, &self.error);
        fields.push(TracingField {
            key: "trace".into(),
            value: trace_value,
        });
    }

    fn tracing_stack_trace_and_causes(&self, fields: &mut Vec<TracingField>) {
        if let Some(stack_trace) = &self.metadata.stack_trace {
            fields.push(TracingField {
                key: "diagnostic_bag.stack_trace".into(),
                value: build_stack_trace_value(stack_trace),
            });
        }
        if !self.display_causes.is_empty() {
            fields.push(TracingField {
                key: "diagnostic_bag.display_causes".into(),
                value: build_display_causes_value(
                    self.display_causes,
                    self.display_causes_state,
                ),
            });
        }
        if !self.source_errors.is_empty() {
            fields.push(TracingField {
                key: "diagnostic_bag.source_errors".into(),
                value: build_source_errors_value(&self.source_errors, self.source_errors_state),
            });
        }
    }

    fn tracing_context_and_attachments(&self, fields: &mut Vec<TracingField>) {
        let (context_items, attachment_items) = build_context_and_attachments(self.attachments);

        fields.push(TracingField {
            key: "context".into(),
            value: AttachmentValue::Array(context_items),
        });
        fields.push(TracingField {
            key: "attachments".into(),
            value: AttachmentValue::Array(attachment_items),
        });
    }

    /// Converts the diagnostic IR to an OpenTelemetry envelope.
    pub fn to_otel_envelope(&self) -> OtelEnvelope {
        let mut attributes = Vec::new();
        let mut context = Vec::new();
        let mut attachments = Vec::new();
        let mut events = Vec::new();

        self.otel_error(&mut attributes);
        self.otel_meta(&mut attributes);
        self.otel_stack_trace_and_causes(&mut attributes);
        self.otel_stats(&mut attributes);
        #[cfg(feature = "trace")]
        self.otel_trace_ev(&mut events);
        self.otel_context_and_attachments(&mut context, &mut attachments);

        OtelEnvelope {
            attributes,
            events,
            context,
            attachments,
            #[cfg(feature = "trace")]
            trace_context: self.otel_trace_context(),
        }
    }

    fn otel_error(&self, attributes: &mut Vec<OtelAttribute>) {
        let fields = vec![
            OtelAttribute {
                key: "message".into(),
                value: OtelValue::String(self.error.message.to_string_owned().into()),
            },
            OtelAttribute {
                key: "type".into(),
                value: OtelValue::String(self.error.r#type.clone().into_owned().into()),
            },
        ];
        attributes.push(OtelAttribute {
            key: "error".into(),
            value: OtelValue::KvList(fields),
        });
    }

    fn otel_meta(&self, attributes: &mut Vec<OtelAttribute>) {
        if let Some(error_code) = &self.metadata.error_code {
            attributes.push(OtelAttribute {
                key: "metadata.error_code".into(),
                value: error_code_otel_value(*error_code),
            });
        }
        if let Some(severity) = self.metadata.severity {
            attributes.push(OtelAttribute {
                key: "metadata.severity".into(),
                value: OtelValue::String(Cow::from(severity)),
            });
        }
        if let Some(category) = self.metadata.category {
            attributes.push(OtelAttribute {
                key: "metadata.category".into(),
                value: OtelValue::String((*category).clone()),
            });
        }
        if let Some(retryable) = self.metadata.retryable {
            attributes.push(OtelAttribute {
                key: "metadata.retryable".into(),
                value: OtelValue::Bool(retryable),
            });
        }
    }

    fn otel_stats(&self, _attributes: &mut Vec<OtelAttribute>) {}

    fn otel_stack_trace_and_causes(&self, attributes: &mut Vec<OtelAttribute>) {
        if let Some(stack_trace) = &self.metadata.stack_trace {
            attributes.push(OtelAttribute {
                key: "diagnostic_bag.stack_trace".into(),
                value: OtelValue::from(&build_stack_trace_value(stack_trace)),
            });
        }
        if !self.display_causes.is_empty() {
            attributes.push(OtelAttribute {
                key: "diagnostic_bag.display_causes".into(),
                value: OtelValue::from(&build_display_causes_value(
                    self.display_causes,
                    self.display_causes_state,
                )),
            });
        }
        if !self.source_errors.is_empty() {
            attributes.push(OtelAttribute {
                key: "diagnostic_bag.source_errors".into(),
                value: OtelValue::from(&build_source_errors_value(
                    &self.source_errors,
                    self.source_errors_state,
                )),
            });
        }
    }

    #[cfg(feature = "trace")]
    fn otel_trace_ev(&self, events: &mut Vec<OtelEvent>) {
        let trace = match self.trace {
            Some(t) => t,
            None => return,
        };
        for trace_event in trace.events.iter() {
            let mut event_attributes = Vec::new();
            if let Some(level) = trace_event.level {
                event_attributes.push(OtelAttribute {
                    key: "level".into(),
                    value: OtelValue::String(level.into()),
                });
            }
            if let Some(ts) = trace_event.timestamp_unix_nano {
                event_attributes.push(OtelAttribute {
                    key: "timestamp_unix_nano".into(),
                    value: OtelValue::U64(ts),
                });
            }
            if !trace_event.attributes.is_empty() {
                let attrs = trace_event
                    .attributes
                    .iter()
                    .map(|attr| OtelAttribute {
                        key: attr.key.clone(),
                        value: OtelValue::from(&attr.value),
                    })
                    .collect();
                event_attributes.push(OtelAttribute {
                    key: "attributes".into(),
                    value: OtelValue::KvList(attrs),
                });
            }
            events.push(OtelEvent {
                name: trace_event.name.clone(),
                attributes: event_attributes,
            });
        }
    }

    #[cfg(feature = "trace")]
    fn otel_trace_context(&self) -> Option<OtelTraceContext> {
        let trace = self.trace?;
        if trace.context.is_empty() {
            return None;
        }
        Some(OtelTraceContext {
            trace_id: trace.context.trace_id.as_ref().map(|v| v.as_cow()),
            span_id: trace.context.span_id.as_ref().map(|v| v.as_cow()),
            parent_span_id: trace.context.parent_span_id.as_ref().map(|v| v.as_cow()),
            sampled: trace.context.sampled,
            trace_state: trace.context.trace_state.clone(),
            flags: trace.context.flags,
        })
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

#[cfg(feature = "trace")]
fn build_error_value(error: &DiagnosticIrError<'_>) -> AttachmentValue {
    let mut map = BTreeMap::new();
    map.insert(
        "message".to_string(),
        AttachmentValue::String(error.message.to_string_owned().into()),
    );
    map.insert(
        "type".to_string(),
        AttachmentValue::String(error.r#type.clone().into_owned().into()),
    );
    AttachmentValue::Object(map)
}

#[cfg(feature = "trace")]
fn build_trace_value(
    trace: &crate::report::ReportTrace,
    error: &DiagnosticIrError<'_>,
) -> AttachmentValue {
    let mut ctx = BTreeMap::new();
    ctx.insert(
        "trace_id".to_string(),
        trace
            .context
            .trace_id
            .as_ref()
            .map(|v| AttachmentValue::String(v.as_cow()))
            .unwrap_or(AttachmentValue::Null),
    );
    ctx.insert(
        "span_id".to_string(),
        trace
            .context
            .span_id
            .as_ref()
            .map(|v| AttachmentValue::String(v.as_cow()))
            .unwrap_or(AttachmentValue::Null),
    );
    ctx.insert(
        "parent_span_id".to_string(),
        trace
            .context
            .parent_span_id
            .as_ref()
            .map(|v| AttachmentValue::String(v.as_cow()))
            .unwrap_or(AttachmentValue::Null),
    );
    ctx.insert(
        "sampled".to_string(),
        trace
            .context
            .sampled
            .map(AttachmentValue::Bool)
            .unwrap_or(AttachmentValue::Null),
    );
    ctx.insert(
        "trace_state".to_string(),
        trace
            .context
            .trace_state
            .as_ref()
            .map(|v| AttachmentValue::String(v.clone()))
            .unwrap_or(AttachmentValue::Null),
    );
    ctx.insert(
        "flags".to_string(),
        trace
            .context
            .flags
            .map(|v| AttachmentValue::Unsigned(v as u64))
            .unwrap_or(AttachmentValue::Null),
    );

    let events = trace
        .events
        .iter()
        .map(|event| {
            let mut map = BTreeMap::new();
            map.insert("name".to_string(), AttachmentValue::String(event.name.clone()));
            map.insert(
                "level".to_string(),
                event
                    .level
                    .map(|v| AttachmentValue::String(v.into()))
                    .unwrap_or(AttachmentValue::Null),
            );
            map.insert(
                "timestamp_unix_nano".to_string(),
                event
                    .timestamp_unix_nano
                    .map(AttachmentValue::Unsigned)
                    .unwrap_or(AttachmentValue::Null),
            );
            let attrs = event
                .attributes
                .iter()
                .map(|attr| {
                    let mut kv = BTreeMap::new();
                    kv.insert("key".to_string(), AttachmentValue::String(attr.key.clone()));
                    kv.insert("value".to_string(), attr.value.clone());
                    AttachmentValue::Object(kv)
                })
                .collect();
            map.insert("attributes".to_string(), AttachmentValue::Array(attrs));
            AttachmentValue::Object(map)
        })
        .collect();

    let mut trace_obj = BTreeMap::new();
    trace_obj.insert("error".to_string(), build_error_value(error));
    trace_obj.insert("context".to_string(), AttachmentValue::Object(ctx));
    trace_obj.insert("events".to_string(), AttachmentValue::Array(events));
    AttachmentValue::Object(trace_obj)
}
