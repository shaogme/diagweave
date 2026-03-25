use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::render::{DiagnosticIr, DiagnosticIrAttachment};
use crate::report::AttachmentValue;

/// A generic value type used for Adapters (e.g., Tracing, OpenTelemetry).
#[derive(Debug, Clone, PartialEq)]
pub enum AdapterValue {
    String(String),
    I64(i64),
    U64(u64),
    F64(f64),
    Bool(bool),
}

impl AdapterValue {
    /// Converts the value to its string representation.
    pub fn as_string(&self) -> String {
        match self {
            Self::String(v) => v.clone(),
            Self::I64(v) => v.to_string(),
            Self::U64(v) => v.to_string(),
            Self::F64(v) => v.to_string(),
            Self::Bool(v) => v.to_string(),
        }
    }
}

impl From<&AttachmentValue> for AdapterValue {
    fn from(value: &AttachmentValue) -> Self {
        match value {
            AttachmentValue::Null => Self::String("null".to_owned()),
            AttachmentValue::String(v) => Self::String(v.clone()),
            AttachmentValue::Integer(v) => Self::I64(*v),
            AttachmentValue::Unsigned(v) => Self::U64(*v),
            AttachmentValue::Float(v) => Self::F64(*v),
            AttachmentValue::Bool(v) => Self::Bool(*v),
            AttachmentValue::Array(_)
            | AttachmentValue::Object(_)
            | AttachmentValue::Bytes(_)
            | AttachmentValue::Redacted { .. } => Self::String(value.to_string()),
        }
    }
}

/// A key-value pair for Tracing fields.
#[derive(Debug, Clone, PartialEq)]
pub struct TracingField {
    pub key: String,
    pub value: AdapterValue,
}

/// An attribute for OpenTelemetry.
#[derive(Debug, Clone, PartialEq)]
pub struct OtelAttribute {
    pub key: String,
    pub value: AdapterValue,
}

/// An event for OpenTelemetry, consisting of a name and attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct OtelEvent {
    pub name: String,
    pub attributes: Vec<OtelAttribute>,
}

/// A collection of attributes and events for OpenTelemetry export.
#[derive(Debug, Clone, PartialEq)]
pub struct OtelEnvelope {
    pub attributes: Vec<OtelAttribute>,
    pub events: Vec<OtelEvent>,
}

impl DiagnosticIr {
    /// Converts the diagnostic IR to a vector of tracing fields.
    pub fn to_tracing_fields(&self) -> Vec<TracingField> {
        let mut fields = Vec::new();

        self.tracing_error(&mut fields);
        self.tracing_meta(&mut fields);
        #[cfg(feature = "trace")]
        self.tracing_trace(&mut fields);
        self.tracing_causes(&mut fields);
        self.tracing_context(&mut fields);
        self.tracing_attachments(&mut fields);

        fields
    }

    fn tracing_error(&self, fields: &mut Vec<TracingField>) {
        fields.push(TracingField {
            key: "error.message".to_owned(),
            value: AdapterValue::String(self.error.message.clone()),
        });
        fields.push(TracingField {
            key: "error.type".to_owned(),
            value: AdapterValue::String(self.error.r#type.clone()),
        });
    }

    fn tracing_meta(&self, fields: &mut Vec<TracingField>) {
        if let Some(error_code) = &self.metadata.error_code {
            fields.push(TracingField {
                key: "error.code".to_owned(),
                value: AdapterValue::String(error_code.clone()),
            });
        }
        if let Some(severity) = self.metadata.severity {
            fields.push(TracingField {
                key: "error.severity".to_owned(),
                value: AdapterValue::String(severity.to_string()),
            });
        }
        if let Some(category) = &self.metadata.category {
            fields.push(TracingField {
                key: "error.category".to_owned(),
                value: AdapterValue::String(category.clone()),
            });
        }
        if let Some(retryable) = self.metadata.retryable {
            fields.push(TracingField {
                key: "error.retryable".to_owned(),
                value: AdapterValue::Bool(retryable),
            });
        }
        if let Some(stack_trace) = &self.metadata.stack_trace {
            fields.push(TracingField {
                key: "stack_trace.present".to_owned(),
                value: AdapterValue::Bool(true),
            });
            fields.push(TracingField {
                key: "stack_trace.frame_count".to_owned(),
                value: AdapterValue::U64(stack_trace.frames.len() as u64),
            });
        } else {
            fields.push(TracingField {
                key: "stack_trace.present".to_owned(),
                value: AdapterValue::Bool(false),
            });
        }
    }

    #[cfg(feature = "trace")]
    fn tracing_trace(&self, fields: &mut Vec<TracingField>) {
        if let Some(trace_id) = &self.trace.context.trace_id {
            fields.push(TracingField {
                key: "trace.trace_id".to_owned(),
                value: AdapterValue::String(trace_id.clone()),
            });
        }
        if let Some(span_id) = &self.trace.context.span_id {
            fields.push(TracingField {
                key: "trace.span_id".to_owned(),
                value: AdapterValue::String(span_id.clone()),
            });
        }
        if let Some(parent_span_id) = &self.trace.context.parent_span_id {
            fields.push(TracingField {
                key: "trace.parent_span_id".to_owned(),
                value: AdapterValue::String(parent_span_id.clone()),
            });
        }
        if let Some(sampled) = self.trace.context.sampled {
            fields.push(TracingField {
                key: "trace.sampled".to_owned(),
                value: AdapterValue::Bool(sampled),
            });
        }
        if let Some(trace_state) = &self.trace.context.trace_state {
            fields.push(TracingField {
                key: "trace.state".to_owned(),
                value: AdapterValue::String(trace_state.clone()),
            });
        }
        if let Some(flags) = self.trace.context.flags {
            fields.push(TracingField {
                key: "trace.flags".to_owned(),
                value: AdapterValue::U64(flags as u64),
            });
        }
        fields.push(TracingField {
            key: "trace.event_count".to_owned(),
            value: AdapterValue::U64(self.trace.events.len() as u64),
        });

        self.tracing_trace_events(fields);
    }

    #[cfg(feature = "trace")]
    fn tracing_trace_events(&self, fields: &mut Vec<TracingField>) {
        for (idx, event) in self.trace.events.iter().enumerate() {
            fields.push(TracingField {
                key: format!("trace.event.{idx}.name"),
                value: AdapterValue::String(event.name.clone()),
            });
            if let Some(level) = event.level {
                fields.push(TracingField {
                    key: format!("trace.event.{idx}.level"),
                    value: AdapterValue::String(level.to_string()),
                });
            }
            if let Some(ts) = event.timestamp_unix_nano {
                fields.push(TracingField {
                    key: format!("trace.event.{idx}.timestamp_unix_nano"),
                    value: AdapterValue::U64(ts),
                });
            }
            for attr in &event.attributes {
                fields.push(TracingField {
                    key: format!("trace.event.{idx}.attr.{}", attr.key),
                    value: AdapterValue::from(&attr.value),
                });
            }
        }
    }

    fn tracing_causes(&self, fields: &mut Vec<TracingField>) {
        if let Some(causes) = &self.metadata.causes {
            fields.push(TracingField {
                key: "causes.present".to_owned(),
                value: AdapterValue::Bool(true),
            });
            fields.push(TracingField {
                key: "causes.count".to_owned(),
                value: AdapterValue::U64(causes.items.len() as u64),
            });
            fields.push(TracingField {
                key: "causes.truncated".to_owned(),
                value: AdapterValue::Bool(causes.truncated),
            });
            fields.push(TracingField {
                key: "causes.cycle_detected".to_owned(),
                value: AdapterValue::Bool(causes.cycle_detected),
            });
            for (idx, cause) in causes.items.iter().enumerate() {
                fields.push(TracingField {
                    key: format!("causes.{idx}.kind"),
                    value: AdapterValue::String(cause.kind.to_string()),
                });
                fields.push(TracingField {
                    key: format!("causes.{idx}.message"),
                    value: AdapterValue::String(cause.message.clone()),
                });
            }
        } else {
            fields.push(TracingField {
                key: "causes.present".to_owned(),
                value: AdapterValue::Bool(false),
            });
        }
    }

    fn tracing_context(&self, fields: &mut Vec<TracingField>) {
        for item in &self.context {
            fields.push(TracingField {
                key: format!("context.{}", item.key),
                value: AdapterValue::from(&item.value),
            });
        }
    }

    fn tracing_attachments(&self, fields: &mut Vec<TracingField>) {
        for (idx, item) in self.attachments.iter().enumerate() {
            match item {
                DiagnosticIrAttachment::Note { message } => fields.push(TracingField {
                    key: format!("attachment.note.{idx}"),
                    value: AdapterValue::String(message.clone()),
                }),
                DiagnosticIrAttachment::Payload { name, value, .. } => fields.push(TracingField {
                    key: format!("attachment.payload.{idx}.{name}"),
                    value: AdapterValue::from(value),
                }),
            }
        }
    }

    /// Converts the diagnostic IR to an OpenTelemetry envelope.
    pub fn to_otel_envelope(&self) -> OtelEnvelope {
        let mut attributes = Vec::new();
        let mut events = Vec::new();

        self.otel_error(&mut attributes);
        self.otel_meta(&mut attributes);
        self.otel_stats(&mut attributes);
        #[cfg(feature = "trace")]
        self.otel_trace(&mut attributes);

        self.otel_context(&mut attributes);
        self.otel_attachments(&mut events);
        #[cfg(feature = "trace")]
        self.otel_trace_ev(&mut events);

        OtelEnvelope { attributes, events }
    }

    fn otel_error(&self, attributes: &mut Vec<OtelAttribute>) {
        attributes.push(OtelAttribute {
            key: "error.message".to_owned(),
            value: AdapterValue::String(self.error.message.clone()),
        });
        attributes.push(OtelAttribute {
            key: "error.type".to_owned(),
            value: AdapterValue::String(self.error.r#type.clone()),
        });
    }

    fn otel_meta(&self, attributes: &mut Vec<OtelAttribute>) {
        if let Some(error_code) = &self.metadata.error_code {
            attributes.push(OtelAttribute {
                key: "error.code".to_owned(),
                value: AdapterValue::String(error_code.clone()),
            });
        }
        if let Some(severity) = self.metadata.severity {
            attributes.push(OtelAttribute {
                key: "error.severity".to_owned(),
                value: AdapterValue::String(severity.to_string()),
            });
        }
        if let Some(category) = &self.metadata.category {
            attributes.push(OtelAttribute {
                key: "error.category".to_owned(),
                value: AdapterValue::String(category.clone()),
            });
        }
        if let Some(retryable) = self.metadata.retryable {
            attributes.push(OtelAttribute {
                key: "error.retryable".to_owned(),
                value: AdapterValue::Bool(retryable),
            });
        }
        if let Some(stack_trace) = &self.metadata.stack_trace {
            attributes.push(OtelAttribute {
                key: "stack_trace.present".to_owned(),
                value: AdapterValue::Bool(true),
            });
            attributes.push(OtelAttribute {
                key: "stack_trace.frame_count".to_owned(),
                value: AdapterValue::U64(stack_trace.frames.len() as u64),
            });
        } else {
            attributes.push(OtelAttribute {
                key: "stack_trace.present".to_owned(),
                value: AdapterValue::Bool(false),
            });
        }

        self.otel_meta_causes(attributes);
    }

    fn otel_meta_causes(&self, attributes: &mut Vec<OtelAttribute>) {
        if let Some(causes) = &self.metadata.causes {
            attributes.push(OtelAttribute {
                key: "causes.present".to_owned(),
                value: AdapterValue::Bool(true),
            });
            attributes.push(OtelAttribute {
                key: "causes.count".to_owned(),
                value: AdapterValue::U64(causes.items.len() as u64),
            });
            attributes.push(OtelAttribute {
                key: "causes.truncated".to_owned(),
                value: AdapterValue::Bool(causes.truncated),
            });
            attributes.push(OtelAttribute {
                key: "causes.cycle_detected".to_owned(),
                value: AdapterValue::Bool(causes.cycle_detected),
            });
        } else {
            attributes.push(OtelAttribute {
                key: "causes.present".to_owned(),
                value: AdapterValue::Bool(false),
            });
        }
    }

    fn otel_stats(&self, attributes: &mut Vec<OtelAttribute>) {
        attributes.push(OtelAttribute {
            key: "report.context_count".to_owned(),
            value: AdapterValue::U64(self.context.len() as u64),
        });
        attributes.push(OtelAttribute {
            key: "report.attachment_count".to_owned(),
            value: AdapterValue::U64(self.attachments.len() as u64),
        });
        #[cfg(feature = "trace")]
        attributes.push(OtelAttribute {
            key: "trace.event_count".to_owned(),
            value: AdapterValue::U64(self.trace.events.len() as u64),
        });
    }

    #[cfg(feature = "trace")]
    fn otel_trace(&self, attributes: &mut Vec<OtelAttribute>) {
        if let Some(trace_id) = &self.trace.context.trace_id {
            attributes.push(OtelAttribute {
                key: "trace.trace_id".to_owned(),
                value: AdapterValue::String(trace_id.clone()),
            });
        }
        if let Some(span_id) = &self.trace.context.span_id {
            attributes.push(OtelAttribute {
                key: "trace.span_id".to_owned(),
                value: AdapterValue::String(span_id.clone()),
            });
        }
        if let Some(parent_span_id) = &self.trace.context.parent_span_id {
            attributes.push(OtelAttribute {
                key: "trace.parent_span_id".to_owned(),
                value: AdapterValue::String(parent_span_id.clone()),
            });
        }
        if let Some(sampled) = self.trace.context.sampled {
            attributes.push(OtelAttribute {
                key: "trace.sampled".to_owned(),
                value: AdapterValue::Bool(sampled),
            });
        }
        if let Some(trace_state) = &self.trace.context.trace_state {
            attributes.push(OtelAttribute {
                key: "trace.state".to_owned(),
                value: AdapterValue::String(trace_state.clone()),
            });
        }
        if let Some(flags) = self.trace.context.flags {
            attributes.push(OtelAttribute {
                key: "trace.flags".to_owned(),
                value: AdapterValue::U64(flags as u64),
            });
        }
    }

    fn otel_context(&self, attributes: &mut Vec<OtelAttribute>) {
        for item in &self.context {
            attributes.push(OtelAttribute {
                key: format!("context.{}", item.key),
                value: AdapterValue::from(&item.value),
            });
        }
    }

    fn otel_attachments(&self, events: &mut Vec<OtelEvent>) {
        for (idx, item) in self.attachments.iter().enumerate() {
            match item {
                DiagnosticIrAttachment::Note { message } => events.push(OtelEvent {
                    name: "report.attachment.note".to_owned(),
                    attributes: vec![
                        OtelAttribute {
                            key: "attachment.index".to_owned(),
                            value: AdapterValue::U64(idx as u64),
                        },
                        OtelAttribute {
                            key: "attachment.message".to_owned(),
                            value: AdapterValue::String(message.clone()),
                        },
                    ],
                }),
                DiagnosticIrAttachment::Payload {
                    name,
                    value,
                    media_type,
                } => {
                    let mut event_attributes = vec![
                        OtelAttribute {
                            key: "attachment.index".to_owned(),
                            value: AdapterValue::U64(idx as u64),
                        },
                        OtelAttribute {
                            key: "attachment.name".to_owned(),
                            value: AdapterValue::String(name.clone()),
                        },
                        OtelAttribute {
                            key: "attachment.value".to_owned(),
                            value: AdapterValue::from(value),
                        },
                    ];
                    if let Some(media_type) = media_type {
                        event_attributes.push(OtelAttribute {
                            key: "attachment.media_type".to_owned(),
                            value: AdapterValue::String(media_type.clone()),
                        });
                    }
                    events.push(OtelEvent {
                        name: "report.attachment.payload".to_owned(),
                        attributes: event_attributes,
                    });
                }
            }
        }
    }

    #[cfg(feature = "trace")]
    fn otel_trace_ev(&self, events: &mut Vec<OtelEvent>) {
        for (idx, trace_event) in self.trace.events.iter().enumerate() {
            let mut event_attributes = vec![
                OtelAttribute {
                    key: "trace.event.index".to_owned(),
                    value: AdapterValue::U64(idx as u64),
                },
                OtelAttribute {
                    key: "trace.event.name".to_owned(),
                    value: AdapterValue::String(trace_event.name.clone()),
                },
            ];
            if let Some(level) = trace_event.level {
                event_attributes.push(OtelAttribute {
                    key: "trace.event.level".to_owned(),
                    value: AdapterValue::String(level.to_string()),
                });
            }
            if let Some(ts) = trace_event.timestamp_unix_nano {
                event_attributes.push(OtelAttribute {
                    key: "trace.event.timestamp_unix_nano".to_owned(),
                    value: AdapterValue::U64(ts),
                });
            }
            for attr in &trace_event.attributes {
                event_attributes.push(OtelAttribute {
                    key: format!("trace.event.attr.{}", attr.key),
                    value: AdapterValue::from(&attr.value),
                });
            }
            events.push(OtelEvent {
                name: "trace.event".to_owned(),
                attributes: event_attributes,
            });
        }
    }
}
