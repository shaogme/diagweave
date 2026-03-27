use alloc::borrow::Cow;
use alloc::string::ToString;
use alloc::vec::Vec;

use crate::render_impl::{
    DiagnosticIr, build_context_and_attachments, build_display_causes_value, build_error_value,
    build_source_errors_value, build_stack_trace_value,
};
use crate::report::AttachmentValue;
use crate::report::ErrorCode;

fn error_code_value(value: &ErrorCode) -> AttachmentValue {
    match value {
        ErrorCode::Integer(v) => AttachmentValue::Integer(*v),
        ErrorCode::String(v) => AttachmentValue::String(v.clone()),
    }
}

/// A key-value pair for Tracing fields.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct TracingField {
    pub key: Cow<'static, str>,
    pub value: AttachmentValue,
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
                value: AttachmentValue::String(severity.to_string().into()),
            });
        }
        if let Some(category) = self.metadata.category.as_deref() {
            fields.push(TracingField {
                key: "metadata.category".into(),
                value: AttachmentValue::String(category.to_string().into()),
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
        if trace.is_empty() {
            return;
        }
        let trace_value = crate::render_impl::build_trace_value(trace, &self.error);
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
                value: build_display_causes_value(self.display_causes, self.display_causes_state),
            });
        }
        if let Some(source_errors) = self.source_errors.as_ref() {
            fields.push(TracingField {
                key: "diagnostic_bag.source_errors".into(),
                value: build_source_errors_value(source_errors),
            });
        }
    }

    fn tracing_context_and_attachments(&self, fields: &mut Vec<TracingField>) {
        let (context_items, attachment_items) = build_context_and_attachments(self.attachments);

        if !context_items.is_empty() {
            fields.push(TracingField {
                key: "context".into(),
                value: AttachmentValue::Array(context_items),
            });
        }
        if !attachment_items.is_empty() {
            fields.push(TracingField {
                key: "attachments".into(),
                value: AttachmentValue::Array(attachment_items),
            });
        }
    }
}
