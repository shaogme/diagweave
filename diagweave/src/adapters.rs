use alloc::borrow::Cow;
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
    pub value: AttachmentValue,
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
    pub value: AttachmentValue,
}

/// An attachment for OpenTelemetry export.
#[derive(Debug, Clone, PartialEq)]
pub enum OtelAttachment {
    Note {
        message: Cow<'static, str>,
    },
    Payload {
        name: Cow<'static, str>,
        value: AttachmentValue,
        media_type: Option<Cow<'static, str>>,
    },
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
                key: "error.severity".into(),
                value: AttachmentValue::String(severity.into()),
            });
        }
        if let Some(category) = self.metadata.category {
            fields.push(TracingField {
                key: "error.category".into(),
                value: AttachmentValue::String((*category).clone()),
            });
        }
        if let Some(retryable) = self.metadata.retryable {
            fields.push(TracingField {
                key: "error.retryable".into(),
                value: AttachmentValue::Bool(retryable),
            });
        }
        if let Some(stack_trace) = &self.metadata.stack_trace {
            fields.push(TracingField {
                key: "stack_trace.present".into(),
                value: AttachmentValue::Bool(true),
            });
            fields.push(TracingField {
                key: "stack_trace.frame_count".into(),
                value: AttachmentValue::Unsigned(stack_trace.frames.len() as u64),
            });
        } else {
            fields.push(TracingField {
                key: "stack_trace.present".into(),
                value: AttachmentValue::Bool(false),
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
                key: "trace.trace_id".into(),
                value: AttachmentValue::String(trace_id.clone()),
            });
        }
        if let Some(span_id) = &trace.context.span_id {
            fields.push(TracingField {
                key: "trace.span_id".into(),
                value: AttachmentValue::String(span_id.clone()),
            });
        }
        if let Some(parent_span_id) = &trace.context.parent_span_id {
            fields.push(TracingField {
                key: "trace.parent_span_id".into(),
                value: AttachmentValue::String(parent_span_id.clone()),
            });
        }
        if let Some(sampled) = trace.context.sampled {
            fields.push(TracingField {
                key: "trace.sampled".into(),
                value: AttachmentValue::Bool(sampled),
            });
        }
        if let Some(trace_state) = &trace.context.trace_state {
            fields.push(TracingField {
                key: "trace.state".into(),
                value: AttachmentValue::String(trace_state.clone()),
            });
        }
        if let Some(flags) = trace.context.flags {
            fields.push(TracingField {
                key: "trace.flags".into(),
                value: AttachmentValue::Unsigned(flags as u64),
            });
        }
        fields.push(TracingField {
            key: "trace.event_count".into(),
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
                key: format!("trace.event.{idx}.name").into(),
                value: AttachmentValue::String(event.name.clone()),
            });
            if let Some(level) = event.level {
                fields.push(TracingField {
                    key: format!("trace.event.{idx}.level").into(),
                    value: AttachmentValue::String(level.into()),
                });
            }
            if let Some(ts) = event.timestamp_unix_nano {
                fields.push(TracingField {
                    key: format!("trace.event.{idx}.timestamp_unix_nano").into(),
                    value: AttachmentValue::Unsigned(ts),
                });
            }
            for attr in &event.attributes {
                fields.push(TracingField {
                    key: format!("trace.event.{idx}.attr.{}", attr.key).into(),
                    value: attr.value.clone(),
                });
            }
        }
    }

    fn tracing_stats(&self, fields: &mut Vec<TracingField>) {
        fields.push(TracingField {
            key: "report.context_count".into(),
            value: AttachmentValue::Unsigned(self.context_count as u64),
        });
        fields.push(TracingField {
            key: "report.attachment_count".into(),
            value: AttachmentValue::Unsigned(self.attachment_count as u64),
        });
    }

    fn tracing_stack_trace_and_causes(&self, fields: &mut Vec<TracingField>) {
        if let Some(stack_trace) = &self.metadata.stack_trace {
            fields.push(TracingField {
                key: "report.stack_trace".into(),
                value: build_stack_trace_value(stack_trace),
            });
        }
        if !self.display_causes.is_empty() {
            fields.push(TracingField {
                key: "report.display_causes".into(),
                value: build_display_causes_value(self.display_causes),
            });
        }
        if !self.source_errors.is_empty() {
            fields.push(TracingField {
                key: "report.source_errors".into(),
                value: build_source_errors_value(self.source_errors),
            });
        }
    }

    fn tracing_context_and_attachments(&self, fields: &mut Vec<TracingField>) {
        let (context_items, attachment_items) = build_context_and_attachments(self.attachments);

        fields.push(TracingField {
            key: "report.context".into(),
            value: AttachmentValue::Array(context_items),
        });
        fields.push(TracingField {
            key: "report.attachments".into(),
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
            value: AttachmentValue::String(self.error.message.to_string_owned().into()),
        });
        attributes.push(OtelAttribute {
            key: "error.type".into(),
            value: AttachmentValue::String(self.error.r#type.clone().into_owned().into()),
        });
    }

    fn otel_meta(&self, attributes: &mut Vec<OtelAttribute>) {
        if let Some(error_code) = &self.metadata.error_code {
            attributes.push(OtelAttribute {
                key: "error.code".into(),
                value: error_code_value(*error_code),
            });
        }
        if let Some(severity) = self.metadata.severity {
            attributes.push(OtelAttribute {
                key: "error.severity".into(),
                value: AttachmentValue::String(severity.into()),
            });
        }
        if let Some(category) = self.metadata.category {
            attributes.push(OtelAttribute {
                key: "error.category".into(),
                value: AttachmentValue::String((*category).clone()),
            });
        }
        if let Some(retryable) = self.metadata.retryable {
            attributes.push(OtelAttribute {
                key: "error.retryable".into(),
                value: AttachmentValue::Bool(retryable),
            });
        }
        if let Some(stack_trace) = &self.metadata.stack_trace {
            attributes.push(OtelAttribute {
                key: "stack_trace.present".into(),
                value: AttachmentValue::Bool(true),
            });
            attributes.push(OtelAttribute {
                key: "stack_trace.frame_count".into(),
                value: AttachmentValue::Unsigned(stack_trace.frames.len() as u64),
            });
        } else {
            attributes.push(OtelAttribute {
                key: "stack_trace.present".into(),
                value: AttachmentValue::Bool(false),
            });
        }
    }

    fn otel_stats(&self, attributes: &mut Vec<OtelAttribute>) {
        attributes.push(OtelAttribute {
            key: "report.context_count".into(),
            value: AttachmentValue::Unsigned(self.context_count as u64),
        });
        attributes.push(OtelAttribute {
            key: "report.attachment_count".into(),
            value: AttachmentValue::Unsigned(self.attachment_count as u64),
        });
        #[cfg(feature = "trace")]
        if let Some(trace) = self.trace {
            attributes.push(OtelAttribute {
                key: "trace.event_count".into(),
                value: AttachmentValue::Unsigned(trace.events.len() as u64),
            });
        }
    }

    fn otel_stack_trace_and_causes(&self, attributes: &mut Vec<OtelAttribute>) {
        if let Some(stack_trace) = &self.metadata.stack_trace {
            attributes.push(OtelAttribute {
                key: "report.stack_trace".into(),
                value: build_stack_trace_value(stack_trace),
            });
        }
        if !self.display_causes.is_empty() {
            attributes.push(OtelAttribute {
                key: "report.display_causes".into(),
                value: build_display_causes_value(self.display_causes),
            });
        }
        if !self.source_errors.is_empty() {
            attributes.push(OtelAttribute {
                key: "report.source_errors".into(),
                value: build_source_errors_value(self.source_errors),
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
                key: "trace.trace_id".into(),
                value: AttachmentValue::String(trace_id.clone()),
            });
        }
        if let Some(span_id) = &trace.context.span_id {
            attributes.push(OtelAttribute {
                key: "trace.span_id".into(),
                value: AttachmentValue::String(span_id.clone()),
            });
        }
        if let Some(parent_span_id) = &trace.context.parent_span_id {
            attributes.push(OtelAttribute {
                key: "trace.parent_span_id".into(),
                value: AttachmentValue::String(parent_span_id.clone()),
            });
        }
        if let Some(sampled) = trace.context.sampled {
            attributes.push(OtelAttribute {
                key: "trace.sampled".into(),
                value: AttachmentValue::Bool(sampled),
            });
        }
        if let Some(trace_state) = &trace.context.trace_state {
            attributes.push(OtelAttribute {
                key: "trace.state".into(),
                value: AttachmentValue::String(trace_state.clone()),
            });
        }
        if let Some(flags) = trace.context.flags {
            attributes.push(OtelAttribute {
                key: "trace.flags".into(),
                value: AttachmentValue::Unsigned(flags as u64),
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
                    key: "trace.event.index".into(),
                    value: AttachmentValue::Unsigned(idx as u64),
                },
                OtelAttribute {
                    key: "trace.event.name".into(),
                    value: AttachmentValue::String(trace_event.name.clone()),
                },
            ];
            if let Some(level) = trace_event.level {
                event_attributes.push(OtelAttribute {
                    key: "trace.event.level".into(),
                    value: AttachmentValue::String(level.into()),
                });
            }
            if let Some(ts) = trace_event.timestamp_unix_nano {
                event_attributes.push(OtelAttribute {
                    key: "trace.event.timestamp_unix_nano".into(),
                    value: AttachmentValue::Unsigned(ts),
                });
            }
            for attr in &trace_event.attributes {
                event_attributes.push(OtelAttribute {
                    key: format!("trace.event.attr.{}", attr.key).into(),
                    value: attr.value.clone(),
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
                        value: value.clone(),
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
                        value: value.clone(),
                        media_type: media_type.clone(),
                    });
                }
            }
        }
    }
}
