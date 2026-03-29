use alloc::string::ToString;
use alloc::vec::Vec;
use ref_str::RefStr;

use crate::render_impl::{
    DiagnosticIr, build_ctx_and_attachments, build_diag_src_errs_val, build_display_causes,
    build_error_value, build_origin_src_errs_val, build_stack_trace_value, build_trace_value,
};
use crate::report::{AttachmentValue, ErrorCode, ObservabilityState};

fn error_code_value(value: &ErrorCode) -> AttachmentValue {
    match value {
        ErrorCode::Integer(v) => AttachmentValue::Integer(*v),
        ErrorCode::String(v) => AttachmentValue::String(v.clone()),
    }
}

/// A key-value pair for Tracing fields.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(bound(deserialize = "'de: 'a")))]
pub struct TracingField<'a> {
    pub key: RefStr<'a>,
    pub value: AttachmentValue,
}

impl<State> DiagnosticIr<'_, State>
where
    State: ObservabilityState,
{
    /// Converts the diagnostic IR to a vector of tracing fields.
    pub fn to_tracing_fields(&self) -> Vec<TracingField<'_>> {
        let mut fields = Vec::new();

        self.tracing_error(&mut fields);
        self.tracing_meta(&mut fields);
        self.tracing_stack_causes(&mut fields);
        #[cfg(feature = "trace")]
        self.tracing_trace(&mut fields);
        self.tracing_ctx_attrs(&mut fields);

        fields
    }

    fn tracing_error(&self, fields: &mut Vec<TracingField<'_>>) {
        fields.push(TracingField {
            key: "error".into(),
            value: build_error_value(&self.error),
        });
    }

    fn tracing_meta(&self, fields: &mut Vec<TracingField<'_>>) {
        if let Some(error_code) = self.metadata.error_code() {
            fields.push(TracingField {
                key: "metadata.error_code".into(),
                value: error_code_value(error_code),
            });
        }
        if let Some(severity) = self.metadata.severity() {
            fields.push(TracingField {
                key: "metadata.severity".into(),
                value: AttachmentValue::String(severity.to_string().into()),
            });
        }
        if let Some(level) = self.metadata.observability_level() {
            fields.push(TracingField {
                key: "metadata.observability_level".into(),
                value: AttachmentValue::String(level.to_string().into()),
            });
        }
        if let Some(category) = self.metadata.category() {
            fields.push(TracingField {
                key: "metadata.category".into(),
                value: AttachmentValue::String(category.to_string().into()),
            });
        }
        if let Some(retryable) = self.metadata.retryable() {
            fields.push(TracingField {
                key: "metadata.retryable".into(),
                value: AttachmentValue::Bool(retryable),
            });
        }
    }

    #[cfg(feature = "trace")]
    fn tracing_trace(&self, fields: &mut Vec<TracingField<'_>>) {
        let trace = match self.trace {
            Some(t) => t,
            None => return,
        };
        if trace.is_empty() {
            return;
        }
        let trace_value = build_trace_value(trace, &self.error);
        fields.push(TracingField {
            key: "trace".into(),
            value: trace_value,
        });
    }

    fn tracing_stack_causes(&self, fields: &mut Vec<TracingField<'_>>) {
        if let Some(stack_trace) = self.metadata.stack_trace() {
            fields.push(TracingField {
                key: "diagnostic_bag.stack_trace".into(),
                value: build_stack_trace_value(stack_trace),
            });
        }
        if !self.display_causes.is_empty() {
            fields.push(TracingField {
                key: "diagnostic_bag.display_causes".into(),
                value: build_display_causes(self.display_causes, self.display_causes_state),
            });
        }
        if let Some(source_errors) = self.origin_source_errors.as_ref() {
            fields.push(TracingField {
                key: "diagnostic_bag.origin_source_errors".into(),
                value: build_origin_src_errs_val(source_errors),
            });
        }
        if let Some(source_errors) = self.diagnostic_source_errors.as_ref() {
            fields.push(TracingField {
                key: "diagnostic_bag.diagnostic_source_errors".into(),
                value: build_diag_src_errs_val(source_errors),
            });
        }
    }

    fn tracing_ctx_attrs(&self, fields: &mut Vec<TracingField<'_>>) {
        let (context_items, attachment_items) = build_ctx_and_attachments(self.attachments);

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
