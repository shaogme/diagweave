use alloc::borrow::Cow;
#[cfg(feature = "trace")]
use alloc::format;
use alloc::string::{String, ToString};
#[cfg(feature = "trace")]
use alloc::vec;
use alloc::vec::Vec;

use crate::render::DiagnosticIr;
use crate::report::{AttachmentValue, ErrorCode};

/// A generic value type used for Adapters (e.g., Tracing, OpenTelemetry).
#[derive(Debug, Clone, PartialEq)]
pub enum AdapterValue {
    String(Cow<'static, str>),
    I64(i64),
    U64(u64),
    F64(f64),
    Bool(bool),
}

impl AdapterValue {
    /// Converts the value to its string representation.
    pub fn as_string(&self) -> String {
        match self {
            Self::String(v) => v.to_string(),
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
            AttachmentValue::Null => Self::String("null".into()),
            AttachmentValue::String(v) => Self::String(v.clone()),
            AttachmentValue::Integer(v) => Self::I64(*v),
            AttachmentValue::Unsigned(v) => Self::U64(*v),
            AttachmentValue::Float(v) => Self::F64(*v),
            AttachmentValue::Bool(v) => Self::Bool(*v),
            AttachmentValue::Array(_)
            | AttachmentValue::Object(_)
            | AttachmentValue::Bytes(_)
            | AttachmentValue::Redacted { .. } => Self::String(value.to_string().into()),
        }
    }
}

impl From<&ErrorCode> for AdapterValue {
    fn from(value: &ErrorCode) -> Self {
        match value {
            ErrorCode::Integer(v) => Self::I64(*v),
            ErrorCode::String(v) => Self::String(v.clone()),
        }
    }
}

/// A key-value pair for Tracing fields.
#[derive(Debug, Clone, PartialEq)]
pub struct TracingField {
    pub key: Cow<'static, str>,
    pub value: AdapterValue,
}

/// An attribute for OpenTelemetry.
#[derive(Debug, Clone, PartialEq)]
pub struct OtelAttribute {
    pub key: Cow<'static, str>,
    pub value: AdapterValue,
}

/// An event for OpenTelemetry, consisting of a name and attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct OtelEvent {
    pub name: Cow<'static, str>,
    pub attributes: Vec<OtelAttribute>,
}

/// A collection of attributes and events for OpenTelemetry export.
#[derive(Debug, Clone, PartialEq)]
pub struct OtelEnvelope {
    pub attributes: Vec<OtelAttribute>,
    pub events: Vec<OtelEvent>,
}

impl DiagnosticIr<'_> {
    /// Converts the diagnostic IR to a vector of tracing fields.
    pub fn to_tracing_fields(&self) -> Vec<TracingField> {
        let mut fields = Vec::new();

        self.tracing_error(&mut fields);
        self.tracing_meta(&mut fields);
        #[cfg(feature = "trace")]
        self.tracing_trace(&mut fields);
        self.tracing_causes(&mut fields);
        self.tracing_stats(&mut fields);

        fields
    }

    fn tracing_error(&self, fields: &mut Vec<TracingField>) {
        fields.push(TracingField {
            key: "error.message".into(),
            value: AdapterValue::String(self.error.message.to_string_owned().into()),
        });
        fields.push(TracingField {
            key: "error.type".into(),
            value: AdapterValue::String(self.error.r#type.clone().into_owned().into()),
        });
    }

    fn tracing_meta(&self, fields: &mut Vec<TracingField>) {
        if let Some(error_code) = &self.metadata.error_code {
            fields.push(TracingField {
                key: "error.code".into(),
                value: AdapterValue::from(*error_code),
            });
        }
        if let Some(severity) = self.metadata.severity {
            fields.push(TracingField {
                key: "error.severity".into(),
                value: AdapterValue::String(severity.into()),
            });
        }
        if let Some(category) = self.metadata.category {
            fields.push(TracingField {
                key: "error.category".into(),
                value: AdapterValue::String((*category).clone()),
            });
        }
        if let Some(retryable) = self.metadata.retryable {
            fields.push(TracingField {
                key: "error.retryable".into(),
                value: AdapterValue::Bool(retryable),
            });
        }
        if let Some(stack_trace) = &self.metadata.stack_trace {
            fields.push(TracingField {
                key: "stack_trace.present".into(),
                value: AdapterValue::Bool(true),
            });
            fields.push(TracingField {
                key: "stack_trace.frame_count".into(),
                value: AdapterValue::U64(stack_trace.frames.len() as u64),
            });
        } else {
            fields.push(TracingField {
                key: "stack_trace.present".into(),
                value: AdapterValue::Bool(false),
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
                value: AdapterValue::String(trace_id.clone()),
            });
        }
        if let Some(span_id) = &trace.context.span_id {
            fields.push(TracingField {
                key: "trace.span_id".into(),
                value: AdapterValue::String(span_id.clone()),
            });
        }
        if let Some(parent_span_id) = &trace.context.parent_span_id {
            fields.push(TracingField {
                key: "trace.parent_span_id".into(),
                value: AdapterValue::String(parent_span_id.clone()),
            });
        }
        if let Some(sampled) = trace.context.sampled {
            fields.push(TracingField {
                key: "trace.sampled".into(),
                value: AdapterValue::Bool(sampled),
            });
        }
        if let Some(trace_state) = &trace.context.trace_state {
            fields.push(TracingField {
                key: "trace.state".into(),
                value: AdapterValue::String(trace_state.clone()),
            });
        }
        if let Some(flags) = trace.context.flags {
            fields.push(TracingField {
                key: "trace.flags".into(),
                value: AdapterValue::U64(flags as u64),
            });
        }
        fields.push(TracingField {
            key: "trace.event_count".into(),
            value: AdapterValue::U64(trace.events.len() as u64),
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
                value: AdapterValue::String(event.name.clone()),
            });
            if let Some(level) = event.level {
                fields.push(TracingField {
                    key: format!("trace.event.{idx}.level").into(),
                    value: AdapterValue::String(level.into()),
                });
            }
            if let Some(ts) = event.timestamp_unix_nano {
                fields.push(TracingField {
                    key: format!("trace.event.{idx}.timestamp_unix_nano").into(),
                    value: AdapterValue::U64(ts),
                });
            }
            for attr in &event.attributes {
                fields.push(TracingField {
                    key: format!("trace.event.{idx}.attr.{}", attr.key).into(),
                    value: AdapterValue::from(&attr.value),
                });
            }
        }
    }

    fn tracing_causes(&self, fields: &mut Vec<TracingField>) {
        if let Some(display_causes) = &self.metadata.display_causes {
            fields.push(TracingField {
                key: "display_causes.present".into(),
                value: AdapterValue::Bool(true),
            });
            fields.push(TracingField {
                key: "display_causes.count".into(),
                value: AdapterValue::U64(display_causes.count as u64),
            });
            fields.push(TracingField {
                key: "display_causes.truncated".into(),
                value: AdapterValue::Bool(display_causes.truncated),
            });
            fields.push(TracingField {
                key: "display_causes.cycle_detected".into(),
                value: AdapterValue::Bool(display_causes.cycle_detected),
            });
        } else {
            fields.push(TracingField {
                key: "display_causes.present".into(),
                value: AdapterValue::Bool(false),
            });
        }

        if let Some(source_errors) = &self.metadata.source_errors {
            fields.push(TracingField {
                key: "source_errors.present".into(),
                value: AdapterValue::Bool(true),
            });
            fields.push(TracingField {
                key: "source_errors.count".into(),
                value: AdapterValue::U64(source_errors.count as u64),
            });
            fields.push(TracingField {
                key: "source_errors.truncated".into(),
                value: AdapterValue::Bool(source_errors.truncated),
            });
            fields.push(TracingField {
                key: "source_errors.cycle_detected".into(),
                value: AdapterValue::Bool(source_errors.cycle_detected),
            });
        } else {
            fields.push(TracingField {
                key: "source_errors.present".into(),
                value: AdapterValue::Bool(false),
            });
        }
    }

    fn tracing_stats(&self, fields: &mut Vec<TracingField>) {
        fields.push(TracingField {
            key: "report.context_count".into(),
            value: AdapterValue::U64(self.context_count as u64),
        });
        fields.push(TracingField {
            key: "report.attachment_count".into(),
            value: AdapterValue::U64(self.attachment_count as u64),
        });
    }

    /// Converts the diagnostic IR to an OpenTelemetry envelope.
    pub fn to_otel_envelope(&self) -> OtelEnvelope {
        let mut attributes = Vec::new();
        #[cfg(feature = "trace")]
        let mut events = Vec::new();
        #[cfg(not(feature = "trace"))]
        let events = Vec::new();

        self.otel_error(&mut attributes);
        self.otel_meta(&mut attributes);
        self.otel_causes(&mut attributes);
        self.otel_stats(&mut attributes);
        #[cfg(feature = "trace")]
        self.otel_trace(&mut attributes);
        #[cfg(feature = "trace")]
        self.otel_trace_ev(&mut events);

        OtelEnvelope { attributes, events }
    }

    fn otel_error(&self, attributes: &mut Vec<OtelAttribute>) {
        attributes.push(OtelAttribute {
            key: "error.message".into(),
            value: AdapterValue::String(self.error.message.to_string_owned().into()),
        });
        attributes.push(OtelAttribute {
            key: "error.type".into(),
            value: AdapterValue::String(self.error.r#type.clone().into_owned().into()),
        });
    }

    fn otel_meta(&self, attributes: &mut Vec<OtelAttribute>) {
        if let Some(error_code) = &self.metadata.error_code {
            attributes.push(OtelAttribute {
                key: "error.code".into(),
                value: AdapterValue::from(*error_code),
            });
        }
        if let Some(severity) = self.metadata.severity {
            attributes.push(OtelAttribute {
                key: "error.severity".into(),
                value: AdapterValue::String(severity.into()),
            });
        }
        if let Some(category) = self.metadata.category {
            attributes.push(OtelAttribute {
                key: "error.category".into(),
                value: AdapterValue::String((*category).clone()),
            });
        }
        if let Some(retryable) = self.metadata.retryable {
            attributes.push(OtelAttribute {
                key: "error.retryable".into(),
                value: AdapterValue::Bool(retryable),
            });
        }
        if let Some(stack_trace) = &self.metadata.stack_trace {
            attributes.push(OtelAttribute {
                key: "stack_trace.present".into(),
                value: AdapterValue::Bool(true),
            });
            attributes.push(OtelAttribute {
                key: "stack_trace.frame_count".into(),
                value: AdapterValue::U64(stack_trace.frames.len() as u64),
            });
        } else {
            attributes.push(OtelAttribute {
                key: "stack_trace.present".into(),
                value: AdapterValue::Bool(false),
            });
        }
    }

    fn otel_causes(&self, attributes: &mut Vec<OtelAttribute>) {
        if let Some(display_causes) = &self.metadata.display_causes {
            attributes.push(OtelAttribute {
                key: "display_causes.present".into(),
                value: AdapterValue::Bool(true),
            });
            attributes.push(OtelAttribute {
                key: "display_causes.count".into(),
                value: AdapterValue::U64(display_causes.count as u64),
            });
            attributes.push(OtelAttribute {
                key: "display_causes.truncated".into(),
                value: AdapterValue::Bool(display_causes.truncated),
            });
            attributes.push(OtelAttribute {
                key: "display_causes.cycle_detected".into(),
                value: AdapterValue::Bool(display_causes.cycle_detected),
            });
        } else {
            attributes.push(OtelAttribute {
                key: "display_causes.present".into(),
                value: AdapterValue::Bool(false),
            });
        }

        if let Some(source_errors) = &self.metadata.source_errors {
            attributes.push(OtelAttribute {
                key: "source_errors.present".into(),
                value: AdapterValue::Bool(true),
            });
            attributes.push(OtelAttribute {
                key: "source_errors.count".into(),
                value: AdapterValue::U64(source_errors.count as u64),
            });
            attributes.push(OtelAttribute {
                key: "source_errors.truncated".into(),
                value: AdapterValue::Bool(source_errors.truncated),
            });
            attributes.push(OtelAttribute {
                key: "source_errors.cycle_detected".into(),
                value: AdapterValue::Bool(source_errors.cycle_detected),
            });
        } else {
            attributes.push(OtelAttribute {
                key: "source_errors.present".into(),
                value: AdapterValue::Bool(false),
            });
        }
    }

    fn otel_stats(&self, attributes: &mut Vec<OtelAttribute>) {
        attributes.push(OtelAttribute {
            key: "report.context_count".into(),
            value: AdapterValue::U64(self.context_count as u64),
        });
        attributes.push(OtelAttribute {
            key: "report.attachment_count".into(),
            value: AdapterValue::U64(self.attachment_count as u64),
        });
        #[cfg(feature = "trace")]
        if let Some(trace) = self.trace {
            attributes.push(OtelAttribute {
                key: "trace.event_count".into(),
                value: AdapterValue::U64(trace.events.len() as u64),
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
                value: AdapterValue::String(trace_id.clone()),
            });
        }
        if let Some(span_id) = &trace.context.span_id {
            attributes.push(OtelAttribute {
                key: "trace.span_id".into(),
                value: AdapterValue::String(span_id.clone()),
            });
        }
        if let Some(parent_span_id) = &trace.context.parent_span_id {
            attributes.push(OtelAttribute {
                key: "trace.parent_span_id".into(),
                value: AdapterValue::String(parent_span_id.clone()),
            });
        }
        if let Some(sampled) = trace.context.sampled {
            attributes.push(OtelAttribute {
                key: "trace.sampled".into(),
                value: AdapterValue::Bool(sampled),
            });
        }
        if let Some(trace_state) = &trace.context.trace_state {
            attributes.push(OtelAttribute {
                key: "trace.state".into(),
                value: AdapterValue::String(trace_state.clone()),
            });
        }
        if let Some(flags) = trace.context.flags {
            attributes.push(OtelAttribute {
                key: "trace.flags".into(),
                value: AdapterValue::U64(flags as u64),
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
                    value: AdapterValue::U64(idx as u64),
                },
                OtelAttribute {
                    key: "trace.event.name".into(),
                    value: AdapterValue::String(trace_event.name.clone()),
                },
            ];
            if let Some(level) = trace_event.level {
                event_attributes.push(OtelAttribute {
                    key: "trace.event.level".into(),
                    value: AdapterValue::String(level.into()),
                });
            }
            if let Some(ts) = trace_event.timestamp_unix_nano {
                event_attributes.push(OtelAttribute {
                    key: "trace.event.timestamp_unix_nano".into(),
                    value: AdapterValue::U64(ts),
                });
            }
            for attr in &trace_event.attributes {
                event_attributes.push(OtelAttribute {
                    key: format!("trace.event.attr.{}", attr.key).into(),
                    value: AdapterValue::from(&attr.value),
                });
            }
            events.push(OtelEvent {
                name: "trace.event".into(),
                attributes: event_attributes,
            });
        }
    }
}
