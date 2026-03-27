use alloc::borrow::Cow;
use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::any;
use core::error::Error;
use core::fmt::{self, Display, Formatter};

use crate::report::{
    Attachment, AttachmentVisit, CauseCollectOptions, CauseTraversalState, ErrorCode, Report,
    Severity, StackTrace,
};
#[cfg(any(feature = "trace", feature = "otel"))]
use alloc::collections::BTreeMap;
#[cfg(any(feature = "trace", feature = "otel"))]
use crate::report::AttachmentValue;
#[cfg(any(feature = "trace", feature = "otel"))]
use crate::report::StackFrame;
#[cfg(feature = "trace")]
use crate::report::{ReportTrace, TraceContext, TraceEvent};

/// Error information in the Diagnostic Intermediate Representation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct DiagnosticIrError<'a> {
    pub message: DiagnosticIrMessage<'a>,
    pub r#type: Cow<'a, str>,
}

/// Lazily-resolved diagnostic message payload.
#[derive(Clone)]
pub enum DiagnosticIrMessage<'a> {
    Borrowed(&'a str),
    Owned(Cow<'a, str>),
    Display(&'a (dyn Display + 'a)),
}

impl DiagnosticIrMessage<'_> {
    pub fn to_string_owned(&self) -> String {
        match self {
            Self::Borrowed(v) => (*v).to_owned(),
            Self::Owned(v) => v.as_ref().to_owned(),
            Self::Display(v) => v.to_string(),
        }
    }
}

impl Display for DiagnosticIrMessage<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Borrowed(v) => f.write_str(v),
            Self::Owned(v) => f.write_str(v.as_ref()),
            Self::Display(v) => write!(f, "{v}"),
        }
    }
}

impl core::fmt::Debug for DiagnosticIrMessage<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.to_string_owned())
    }
}

impl PartialEq for DiagnosticIrMessage<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.to_string_owned() == other.to_string_owned()
    }
}

impl Eq for DiagnosticIrMessage<'_> {}

impl PartialEq<&str> for DiagnosticIrMessage<'_> {
    fn eq(&self, other: &&str) -> bool {
        match self {
            Self::Borrowed(v) => v == other,
            Self::Owned(v) => v.as_ref() == *other,
            Self::Display(v) => v.to_string() == *other,
        }
    }
}

#[cfg(feature = "json")]
impl serde::Serialize for DiagnosticIrMessage<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string_owned())
    }
}

/// Metadata information in the Diagnostic Intermediate Representation.
pub struct DiagnosticIrMetadata<'a> {
    pub error_code: Option<&'a ErrorCode>,
    pub severity: Option<Severity>,
    pub category: Option<&'a Cow<'static, str>>,
    pub retryable: Option<bool>,
    pub stack_trace: Option<&'a StackTrace>,
}

/// A platform-agnostic intermediate representation of a diagnostic report.
pub struct DiagnosticIr<'a> {
    #[cfg(feature = "json")]
    pub schema_version: Cow<'static, str>,
    pub error: DiagnosticIrError<'a>,
    pub metadata: DiagnosticIrMetadata<'a>,
    #[cfg(feature = "trace")]
    pub trace: Option<&'a ReportTrace>,
    pub attachments: &'a [Attachment],
    pub display_causes: &'a [Box<dyn Display + 'static>],
    pub display_causes_state: CauseTraversalState,
    pub source_errors: Vec<String>,
    pub source_errors_state: CauseTraversalState,
    pub context_count: usize,
    pub attachment_count: usize,
}

impl<E> Report<E> {
    pub fn to_diagnostic_ir(&self) -> DiagnosticIr<'_>
    where
        E: Error + Display + 'static,
    {
        let metadata = self.metadata();
        let (context_count, attachment_count) = count_attachments(self);
        let display_causes_state = self
            .visit_causes_ext(CauseCollectOptions::default(), |_| Ok(()))
            .unwrap_or_default();
        let mut source_errors = Vec::new();
        let source_errors_state = self
            .visit_sources_ext(CauseCollectOptions::default(), |err| {
                source_errors.push(err.to_string());
                Ok(())
            })
            .unwrap_or_default();

        DiagnosticIr {
            #[cfg(feature = "json")]
            schema_version: Cow::Borrowed(crate::render_impl::REPORT_JSON_SCHEMA_VERSION),
            error: DiagnosticIrError {
                message: DiagnosticIrMessage::Display(self.inner()),
                r#type: Cow::Borrowed(any::type_name::<E>()),
            },
            metadata: DiagnosticIrMetadata {
                error_code: metadata.error_code.as_ref(),
                severity: metadata.severity,
                category: metadata.category.as_ref(),
                retryable: metadata.retryable,
                stack_trace: self.stack_trace(),
            },
            #[cfg(feature = "trace")]
            trace: self.trace(),
            attachments: self.attachments(),
            display_causes: self.display_causes(),
            display_causes_state,
            source_errors,
            source_errors_state,
            context_count,
            attachment_count,
        }
    }
}

fn count_attachments(report: &Report<impl Error + 'static>) -> (usize, usize) {
    let mut context = 0usize;
    let mut attachments = 0usize;
    match report.visit_attachments(|item| {
        match item {
            AttachmentVisit::Context { .. } => context += 1,
            AttachmentVisit::Note { .. } | AttachmentVisit::Payload { .. } => attachments += 1,
        }
        Ok(())
    }) {
        Ok(()) => (context, attachments),
        Err(_) => (0, 0),
    }
}

#[cfg(feature = "trace")]
pub(crate) fn build_context_and_attachments(
    attachments: &[Attachment],
) -> (Vec<AttachmentValue>, Vec<AttachmentValue>) {
    let mut context_items = Vec::new();
    let mut attachment_items = Vec::new();

    for attachment in attachments {
        match attachment {
            Attachment::Context { key, value } => {
                let mut map = BTreeMap::new();
                map.insert("key".to_string(), AttachmentValue::String(key.clone()));
                map.insert("value".to_string(), value.clone());
                context_items.push(AttachmentValue::Object(map));
            }
            Attachment::Note { message } => {
                let mut map = BTreeMap::new();
                map.insert("kind".to_string(), AttachmentValue::String("note".into()));
                map.insert(
                    "message".to_string(),
                    AttachmentValue::String(message.to_string().into()),
                );
                attachment_items.push(AttachmentValue::Object(map));
            }
            Attachment::Payload {
                name,
                value,
                media_type,
            } => {
                let mut map = BTreeMap::new();
                map.insert(
                    "kind".to_string(),
                    AttachmentValue::String("payload".into()),
                );
                map.insert("name".to_string(), AttachmentValue::String(name.clone()));
                map.insert("value".to_string(), value.clone());
                map.insert(
                    "media_type".to_string(),
                    media_type
                        .as_ref()
                        .map(|v| AttachmentValue::String(v.clone()))
                        .unwrap_or(AttachmentValue::Null),
                );
                attachment_items.push(AttachmentValue::Object(map));
            }
        }
    }

    (context_items, attachment_items)
}

#[cfg(feature = "trace")]
pub(crate) fn build_error_value(error: &DiagnosticIrError<'_>) -> AttachmentValue {
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
pub(crate) fn build_trace_value(
    trace: &ReportTrace,
    error: &DiagnosticIrError<'_>,
) -> AttachmentValue {
    let mut trace_obj = BTreeMap::new();
    trace_obj.insert("error".to_string(), build_error_value(error));
    trace_obj.insert(
        "context".to_string(),
        build_trace_attachment_context_value(&trace.context),
    );
    trace_obj.insert(
        "events".to_string(),
        AttachmentValue::Array(
            trace
                .events
                .iter()
                .map(build_trace_attachment_event_value)
                .collect(),
        ),
    );
    AttachmentValue::Object(trace_obj)
}

#[cfg(feature = "trace")]
fn build_trace_attachment_context_value(context: &TraceContext) -> AttachmentValue {
    let mut ctx = BTreeMap::new();
    ctx.insert(
        "trace_id".to_string(),
        context
            .trace_id
            .as_ref()
            .map(|v| AttachmentValue::String(v.as_cow()))
            .unwrap_or(AttachmentValue::Null),
    );
    ctx.insert(
        "span_id".to_string(),
        context
            .span_id
            .as_ref()
            .map(|v| AttachmentValue::String(v.as_cow()))
            .unwrap_or(AttachmentValue::Null),
    );
    ctx.insert(
        "parent_span_id".to_string(),
        context
            .parent_span_id
            .as_ref()
            .map(|v| AttachmentValue::String(v.as_cow()))
            .unwrap_or(AttachmentValue::Null),
    );
    ctx.insert(
        "sampled".to_string(),
        context
            .sampled
            .map(AttachmentValue::Bool)
            .unwrap_or(AttachmentValue::Null),
    );
    ctx.insert(
        "trace_state".to_string(),
        context
            .trace_state
            .as_ref()
            .map(|v| AttachmentValue::String(v.clone()))
            .unwrap_or(AttachmentValue::Null),
    );
    ctx.insert(
        "flags".to_string(),
        context
            .flags
            .map(|v| AttachmentValue::Unsigned(v as u64))
            .unwrap_or(AttachmentValue::Null),
    );
    AttachmentValue::Object(ctx)
}

#[cfg(feature = "trace")]
fn build_trace_attachment_event_value(event: &TraceEvent) -> AttachmentValue {
    let mut map = BTreeMap::new();
    map.insert(
        "name".to_string(),
        AttachmentValue::String(event.name.clone()),
    );
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
    map.insert(
        "attributes".to_string(),
        AttachmentValue::Array(
            event
                .attributes
                .iter()
                .map(|attr| {
                    let mut kv = BTreeMap::new();
                    kv.insert("key".to_string(), AttachmentValue::String(attr.key.clone()));
                    kv.insert("value".to_string(), attr.value.clone());
                    AttachmentValue::Object(kv)
                })
                .collect(),
        ),
    );
    AttachmentValue::Object(map)
}

#[cfg(any(feature = "trace", feature = "otel"))]
pub(crate) fn build_stack_trace_value(stack_trace: &StackTrace) -> AttachmentValue {
    let mut map = BTreeMap::new();
    let format = match stack_trace.format {
        crate::report::StackTraceFormat::Native => "native",
        crate::report::StackTraceFormat::Raw => "raw",
    };
    map.insert("format".to_string(), AttachmentValue::String(format.into()));
    map.insert(
        "frames".to_string(),
        AttachmentValue::Array(
            stack_trace
                .frames
                .iter()
                .map(build_stack_frame_value)
                .collect(),
        ),
    );
    map.insert(
        "raw".to_string(),
        stack_trace
            .raw
            .as_ref()
            .map(|v| AttachmentValue::String(v.clone().into()))
            .unwrap_or(AttachmentValue::Null),
    );
    AttachmentValue::Object(map)
}

#[cfg(any(feature = "trace", feature = "otel"))]
fn build_stack_frame_value(frame: &StackFrame) -> AttachmentValue {
    let mut map = BTreeMap::new();
    map.insert(
        "symbol".to_string(),
        frame
            .symbol
            .as_ref()
            .map(|v| AttachmentValue::String(v.clone().into()))
            .unwrap_or(AttachmentValue::Null),
    );
    map.insert(
        "module_path".to_string(),
        frame
            .module_path
            .as_ref()
            .map(|v| AttachmentValue::String(v.clone().into()))
            .unwrap_or(AttachmentValue::Null),
    );
    map.insert(
        "file".to_string(),
        frame
            .file
            .as_ref()
            .map(|v| AttachmentValue::String(v.clone().into()))
            .unwrap_or(AttachmentValue::Null),
    );
    map.insert(
        "line".to_string(),
        frame
            .line
            .map(|v| AttachmentValue::Unsigned(v as u64))
            .unwrap_or(AttachmentValue::Null),
    );
    map.insert(
        "column".to_string(),
        frame
            .column
            .map(|v| AttachmentValue::Unsigned(v as u64))
            .unwrap_or(AttachmentValue::Null),
    );
    AttachmentValue::Object(map)
}

#[cfg(any(feature = "trace", feature = "otel"))]
pub(crate) fn build_display_causes_value(
    display_causes: &[Box<dyn Display + 'static>],
    state: CauseTraversalState,
) -> AttachmentValue {
    let mut map = BTreeMap::new();
    map.insert(
        "items".to_string(),
        AttachmentValue::Array(
            display_causes
                .iter()
                .map(|v: &Box<dyn Display + 'static>| AttachmentValue::String(v.to_string().into()))
                .collect(),
        ),
    );
    map.insert(
        "truncated".to_string(),
        AttachmentValue::Bool(state.truncated),
    );
    map.insert(
        "cycle_detected".to_string(),
        AttachmentValue::Bool(state.cycle_detected),
    );
    AttachmentValue::Object(map)
}

#[cfg(any(feature = "trace", feature = "otel"))]
pub(crate) fn build_source_errors_value(
    source_errors: &[String],
    state: CauseTraversalState,
) -> AttachmentValue {
    let mut map = BTreeMap::new();
    map.insert(
        "items".to_string(),
        AttachmentValue::Array(
            source_errors
                .iter()
                .map(|message| {
                    let mut item = BTreeMap::new();
                    item.insert(
                        "message".to_string(),
                        AttachmentValue::String(message.clone().into()),
                    );
                    AttachmentValue::Object(item)
                })
                .collect(),
        ),
    );
    map.insert(
        "truncated".to_string(),
        AttachmentValue::Bool(state.truncated),
    );
    map.insert(
        "cycle_detected".to_string(),
        AttachmentValue::Bool(state.cycle_detected),
    );
    AttachmentValue::Object(map)
}
