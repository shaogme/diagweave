#[cfg(feature = "json")]
#[path = "render/json.rs"]
mod json;
#[path = "render/pretty.rs"]
mod pretty;

#[cfg(all(feature = "trace", feature = "json"))]
use crate::report::TraceEventAttribute;
use crate::report::{
    Attachment, AttachmentValue, AttachmentVisit, CauseCollectOptions, CauseTraversalState,
    ErrorCode, Report, Severity, StackFrame, StackTrace,
};
#[cfg(feature = "trace")]
use crate::report::{ReportTrace, TraceContext, TraceEvent};
use alloc::borrow::Cow;
use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::any;
use core::error::Error;
use core::fmt::{self, Display, Formatter};

pub use pretty::Pretty;

#[cfg(feature = "json")]
pub use json::{Json, REPORT_JSON_SCHEMA_DRAFT, REPORT_JSON_SCHEMA_VERSION, report_json_schema};

#[cfg(all(feature = "trace", feature = "json"))]
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct TraceSectionValue {
    pub context: TraceContextValue,
    pub events: Vec<TraceEventValue>,
}

#[cfg(all(feature = "trace", feature = "json"))]
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct TraceContextValue {
    pub trace_id: Option<Cow<'static, str>>,
    pub span_id: Option<Cow<'static, str>>,
    pub parent_span_id: Option<Cow<'static, str>>,
    pub sampled: Option<bool>,
    pub trace_state: Option<Cow<'static, str>>,
    pub flags: Option<u8>,
}

#[cfg(all(feature = "trace", feature = "json"))]
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct TraceEventValue {
    pub name: Cow<'static, str>,
    pub level: Option<Cow<'static, str>>,
    pub timestamp_unix_nano: Option<u64>,
    pub attributes: Vec<TraceAttributeValue>,
}

#[cfg(all(feature = "trace", feature = "json"))]
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct TraceAttributeValue {
    pub key: Cow<'static, str>,
    pub value: AttachmentValue,
}

/// Options for rendering a diagnostic report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct ReportRenderOptions {
    pub max_source_depth: usize,
    pub detect_source_cycle: bool,
    pub pretty_indent: PrettyIndent,
    pub show_type_name: bool,
    pub show_empty_sections: bool,
    pub show_governance_section: bool,
    pub show_trace_section: bool,
    pub show_stack_trace_section: bool,
    pub show_context_section: bool,
    pub show_attachments_section: bool,
    pub show_cause_chains_section: bool,
    pub stack_trace_max_lines: usize,
    pub stack_trace_include_raw: bool,
    pub stack_trace_include_frames: bool,
    pub json_pretty: bool,
}

/// Indentation style for pretty rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(rename_all = "snake_case"))]
pub enum PrettyIndent {
    Spaces(u8),
    Tab,
}

impl Default for ReportRenderOptions {
    fn default() -> Self {
        Self {
            max_source_depth: 16,
            detect_source_cycle: true,
            pretty_indent: PrettyIndent::Spaces(2),
            show_type_name: true,
            show_empty_sections: true,
            show_governance_section: true,
            show_trace_section: true,
            show_stack_trace_section: true,
            show_context_section: true,
            show_attachments_section: true,
            show_cause_chains_section: true,
            stack_trace_max_lines: 24,
            stack_trace_include_raw: true,
            stack_trace_include_frames: true,
            json_pretty: false,
        }
    }
}

/// A trait for rendering a diagnostic report using a specific format.
pub trait ReportRenderer<E> {
    fn render(&self, report: &Report<E>, f: &mut Formatter<'_>) -> fmt::Result;
}

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
    /// Materializes the value as an owned `String`.
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

/// A renderer that produces a compact display of the report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Compact;

impl<E> Report<E> {
    /// Returns a renderer for compact output.
    pub fn compact(&self) -> RenderedReport<'_, E, Compact> {
        self.render(Compact)
    }

    /// Returns a renderer for pretty-printed output.
    pub fn pretty(&self) -> RenderedReport<'_, E, Pretty> {
        self.render(Pretty::default())
    }

    /// Returns a renderer for JSON output.
    #[cfg(feature = "json")]
    pub fn json(&self) -> RenderedReport<'_, E, Json> {
        self.render(Json::default())
    }

    /// Returns a renderer for the given renderer implementation.
    pub fn render<R>(&self, renderer: R) -> RenderedReport<'_, E, R> {
        RenderedReport {
            report: self,
            renderer,
        }
    }
}

impl<E> Report<E>
where
    E: Error + Display + 'static,
{
    /// Converts the report to a platform-agnostic intermediate representation.
    pub fn to_diagnostic_ir(&self) -> DiagnosticIr<'_> {
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
            schema_version: Cow::Borrowed(REPORT_JSON_SCHEMA_VERSION),
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

#[cfg(all(feature = "trace", feature = "json"))]
pub(crate) fn build_trace_section_value(trace: &ReportTrace) -> TraceSectionValue {
    TraceSectionValue {
        context: build_trace_wire_context_value(&trace.context),
        events: trace
            .events
            .iter()
            .map(build_trace_wire_event_value)
            .collect(),
    }
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

#[cfg(all(feature = "trace", feature = "json"))]
fn build_trace_wire_context_value(context: &TraceContext) -> TraceContextValue {
    TraceContextValue {
        trace_id: context.trace_id.as_ref().map(|v| v.as_cow()),
        span_id: context.span_id.as_ref().map(|v| v.as_cow()),
        parent_span_id: context.parent_span_id.as_ref().map(|v| v.as_cow()),
        sampled: context.sampled,
        trace_state: context.trace_state.clone(),
        flags: context.flags,
    }
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

#[cfg(all(feature = "trace", feature = "json"))]
fn build_trace_wire_event_value(event: &TraceEvent) -> TraceEventValue {
    TraceEventValue {
        name: event.name.clone(),
        level: event.level.map(Cow::from),
        timestamp_unix_nano: event.timestamp_unix_nano,
        attributes: event
            .attributes
            .iter()
            .map(build_trace_wire_attribute_value)
            .collect(),
    }
}

#[cfg(all(feature = "trace", feature = "json"))]
fn build_trace_wire_attribute_value(attr: &TraceEventAttribute) -> TraceAttributeValue {
    TraceAttributeValue {
        key: attr.key.clone(),
        value: attr.value.clone(),
    }
}

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

/// A report that has been paired with a renderer, implementing `Display`.
pub struct RenderedReport<'a, E, R> {
    report: &'a Report<E>,
    renderer: R,
}

impl<E> ReportRenderer<E> for Compact
where
    E: Display,
{
    fn render(&self, report: &Report<E>, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{report}")
    }
}

impl<E, R> Display for RenderedReport<'_, E, R>
where
    R: ReportRenderer<E>,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.renderer.render(self.report, f)
    }
}
