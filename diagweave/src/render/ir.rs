use alloc::string::ToString;
use alloc::sync::Arc;
#[cfg(feature = "trace")]
use alloc::vec::Vec;
use core::any;
use core::error::Error;
use core::fmt::{self, Display, Formatter};
use ref_str::RefStr;
#[cfg(feature = "json")]
use ref_str::StaticRefStr;
use std::borrow::Cow;

#[cfg(feature = "json")]
use crate::render_impl::REPORT_JSON_SCHEMA_VERSION;
#[cfg(any(feature = "trace", feature = "otel"))]
use crate::report::AttachmentValue;
use crate::report::SourceErrorChain;
#[cfg(any(feature = "trace", feature = "otel"))]
use crate::report::StackFrame;
use crate::report::{
    Attachment, CauseTraversalState, ContextMap, ErrorCode, HasSeverity, MissingSeverity, Report,
    Severity, SeverityState, StackTrace,
};
#[cfg(feature = "trace")]
use crate::report::{ReportTrace, TraceContext, TraceEvent};
#[cfg(any(feature = "trace", feature = "otel"))]
use crate::utils::FastMap;
/// A structured diagnostic error node shared by renderers and adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct DiagnosticIrErrorNode<'a> {
    pub message: DiagnosticIrMessage<'a>,
    pub r#type: RefStr<'a>,
}

/// Alias for the structured diagnostic error node used in the IR.
pub type DiagnosticIrError<'a> = DiagnosticIrErrorNode<'a>;

/// Lazily-resolved diagnostic message payload.
#[derive(Clone)]
pub enum DiagnosticIrMessage<'a> {
    Borrowed(&'a str),
    Owned(RefStr<'a>),
    Display(&'a (dyn Display + 'a)),
}

impl DiagnosticIrMessage<'_> {
    pub fn as_cow(&self) -> Cow<'_, str> {
        match self {
            Self::Borrowed(v) => Cow::Borrowed(v),
            Self::Owned(v) => Cow::Borrowed(v),
            Self::Display(v) => Cow::Owned(v.to_string()),
        }
    }
}

impl Display for DiagnosticIrMessage<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Borrowed(v) => f.write_str(v),
            Self::Owned(v) => f.write_str(v.as_str()),
            Self::Display(v) => write!(f, "{v}"),
        }
    }
}

impl core::fmt::Debug for DiagnosticIrMessage<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.as_cow())
    }
}

impl PartialEq for DiagnosticIrMessage<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.as_cow() == other.as_cow()
    }
}

impl Eq for DiagnosticIrMessage<'_> {}

impl PartialEq<&str> for DiagnosticIrMessage<'_> {
    fn eq(&self, other: &&str) -> bool {
        match self {
            Self::Borrowed(v) => v == other,
            Self::Owned(v) => v.as_str() == *other,
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
        serializer.serialize_str(&self.as_cow())
    }
}

/// Metadata information in the Diagnostic Intermediate Representation.
#[derive(Clone)]
pub struct DiagnosticIrMetadata<'a, State = MissingSeverity> {
    error_code: Option<&'a ErrorCode>,
    severity: State,
    category: Option<&'a str>,
    retryable: Option<bool>,
    stack_trace: Option<&'a StackTrace>,
}

impl<'a, State> DiagnosticIrMetadata<'a, State>
where
    State: SeverityState,
{
    /// Returns the error code, if present.
    pub fn error_code(&self) -> Option<&'a ErrorCode> {
        self.error_code
    }

    /// Returns the severity, if present.
    pub fn severity(&self) -> Option<Severity> {
        self.severity.severity()
    }

    /// Returns the category, if present.
    pub fn category(&self) -> Option<&'a str> {
        self.category
    }

    /// Returns whether the diagnostic is retryable, if present.
    pub fn retryable(&self) -> Option<bool> {
        self.retryable
    }

    /// Returns the attached stack trace, if present.
    pub fn stack_trace(&self) -> Option<&'a StackTrace> {
        self.stack_trace
    }

    fn map_severity<NewState>(self, severity: NewState) -> DiagnosticIrMetadata<'a, NewState>
    where
        NewState: SeverityState,
    {
        DiagnosticIrMetadata {
            error_code: self.error_code,
            severity,
            category: self.category,
            retryable: self.retryable,
            stack_trace: self.stack_trace,
        }
    }

    /// Replaces the metadata typestate with a concrete severity.
    pub fn with_severity(self, level: Severity) -> DiagnosticIrMetadata<'a, HasSeverity> {
        self.map_severity(HasSeverity::new(level))
    }
}

impl DiagnosticIrMetadata<'_, HasSeverity> {
    /// Returns the guaranteed severity.
    pub const fn required_severity(&self) -> Severity {
        self.severity.severity()
    }
}

/// A platform-agnostic intermediate representation of a diagnostic report.
#[derive(Clone)]
pub struct DiagnosticIr<'a, State = MissingSeverity> {
    #[cfg(feature = "json")]
    pub schema_version: StaticRefStr,
    pub error: DiagnosticIrError<'a>,
    pub metadata: DiagnosticIrMetadata<'a, State>,
    #[cfg(feature = "trace")]
    pub trace: &'a ReportTrace,
    pub context: &'a ContextMap,
    pub system: &'a ContextMap,
    pub attachments: &'a [Attachment],
    pub display_causes: &'a [Arc<dyn Display + Send + Sync + 'static>],
    pub display_causes_state: CauseTraversalState,
    pub origin_source_errors: Option<SourceErrorChain>,
    pub diagnostic_source_errors: Option<SourceErrorChain>,
    pub context_count: usize,
    pub system_count: usize,
    pub attachment_count: usize,
}

impl<'a, State> DiagnosticIr<'a, State>
where
    State: SeverityState,
{
    /// Replaces the IR typestate with a concrete severity.
    pub fn with_severity(self, level: Severity) -> DiagnosticIr<'a, HasSeverity> {
        let Self {
            #[cfg(feature = "json")]
            schema_version,
            error,
            metadata,
            #[cfg(feature = "trace")]
            trace,
            context,
            system,
            attachments,
            display_causes,
            display_causes_state,
            origin_source_errors,
            diagnostic_source_errors,
            context_count,
            system_count,
            attachment_count,
        } = self;

        DiagnosticIr {
            #[cfg(feature = "json")]
            schema_version,
            error,
            metadata: metadata.with_severity(level),
            #[cfg(feature = "trace")]
            trace,
            context,
            system,
            attachments,
            display_causes,
            display_causes_state,
            origin_source_errors,
            diagnostic_source_errors,
            context_count,
            system_count,
            attachment_count,
        }
    }
}

impl<E, State> Report<E, State>
where
    State: SeverityState,
{
    /// Builds the renderer-agnostic diagnostic intermediate representation.
    pub fn to_diagnostic_ir(&self) -> DiagnosticIr<'_, State>
    where
        E: Error + Display + 'static,
    {
        let metadata = self.metadata();
        let (context_count, system_count, attachment_count) = count_attachments(self);
        let display_causes_state = self
            .visit_causes_ext(self.options().as_cause_options(), |_| Ok(()))
            .unwrap_or_default();
        DiagnosticIr {
            #[cfg(feature = "json")]
            schema_version: REPORT_JSON_SCHEMA_VERSION.into(),
            error: DiagnosticIrErrorNode {
                message: DiagnosticIrMessage::Display(self.inner()),
                r#type: any::type_name::<E>().into(),
            },
            metadata: DiagnosticIrMetadata {
                error_code: metadata.error_code(),
                severity: self.severity_state(),
                category: metadata.category(),
                retryable: metadata.retryable(),
                stack_trace: self.stack_trace(),
            },
            #[cfg(feature = "trace")]
            trace: self.trace(),
            context: self.context(),
            system: self.system(),
            attachments: self.attachments(),
            display_causes: self.display_causes(),
            display_causes_state,
            origin_source_errors: self.origin_src_err_view(self.options().as_cause_options()),
            diagnostic_source_errors: self.diag_src_err_view(self.options().as_cause_options()),
            context_count,
            system_count,
            attachment_count,
        }
    }
}

fn count_attachments<State>(report: &Report<impl Error + 'static, State>) -> (usize, usize, usize)
where
    State: SeverityState,
{
    (
        report.context().len(),
        report.system().len(),
        report.attachments().len(),
    )
}

#[cfg(feature = "trace")]
pub(crate) fn build_ctx_and_attachments(
    context: &ContextMap,
    system: &ContextMap,
    attachments: &[Attachment],
) -> (AttachmentValue, AttachmentValue, Vec<AttachmentValue>) {
    let mut context_map = FastMap::new();
    let mut system_map = FastMap::new();
    let mut attachment_items = Vec::new();

    for (key, value) in context {
        context_map.insert(key.clone(), AttachmentValue::from(value));
    }
    for (key, value) in system {
        system_map.insert(key.clone(), AttachmentValue::from(value));
    }

    for attachment in attachments {
        match attachment {
            Attachment::Note { message } => {
                let mut map = FastMap::new();
                map.insert("kind".into(), AttachmentValue::String("note".into()));
                map.insert(
                    "message".into(),
                    AttachmentValue::String(message.to_string().into()),
                );
                attachment_items.push(AttachmentValue::Object(map));
            }
            Attachment::Payload {
                name,
                value,
                media_type,
            } => {
                let mut map = FastMap::new();
                map.insert("kind".into(), AttachmentValue::String("payload".into()));
                map.insert("name".into(), AttachmentValue::String(name.clone()));
                map.insert("value".into(), value.clone());
                if let Some(media_type) = media_type.as_ref() {
                    map.insert(
                        "media_type".into(),
                        AttachmentValue::String(media_type.clone()),
                    );
                }
                attachment_items.push(AttachmentValue::Object(map));
            }
        }
    }

    (
        AttachmentValue::Object(context_map),
        AttachmentValue::Object(system_map),
        attachment_items,
    )
}

#[cfg(any(feature = "trace", feature = "otel"))]
pub(crate) fn build_error_value(error: &DiagnosticIrError<'_>) -> AttachmentValue {
    let mut map = FastMap::new();
    map.insert(
        "message".into(),
        AttachmentValue::String(error.message.to_string().into()),
    );
    map.insert(
        "type".into(),
        AttachmentValue::String(error.r#type.to_string().into()),
    );
    AttachmentValue::Object(map)
}

#[cfg(feature = "trace")]
pub(crate) fn build_trace_value(
    trace: &ReportTrace,
    error: &DiagnosticIrError<'_>,
) -> AttachmentValue {
    let mut trace_obj = FastMap::new();
    trace_obj.insert("error".into(), build_error_value(error));
    trace_obj.insert("context".into(), build_trace_ctx_value_opt(trace.context()));
    trace_obj.insert(
        "events".into(),
        AttachmentValue::Array(
            trace
                .events()
                .map(|e| e.iter().map(build_trace_event_value).collect())
                .unwrap_or_default(),
        ),
    );
    AttachmentValue::Object(trace_obj)
}

#[cfg(feature = "trace")]
fn build_trace_ctx_value_opt(context: Option<&TraceContext>) -> AttachmentValue {
    match context {
        Some(ctx) => build_trace_ctx_value(ctx),
        None => AttachmentValue::Object(FastMap::new()),
    }
}

#[cfg(feature = "trace")]
fn build_trace_ctx_value(context: &TraceContext) -> AttachmentValue {
    let mut ctx = FastMap::new();
    if let Some(value) = context.trace_id.as_ref() {
        ctx.insert("trace_id".into(), AttachmentValue::String(value.clone().into_inner()));
    }
    if let Some(value) = context.span_id.as_ref() {
        ctx.insert("span_id".into(), AttachmentValue::String(value.clone().into_inner()));
    }
    if let Some(value) = context.parent_span_id.as_ref() {
        ctx.insert(
            "parent_span_id".into(),
            AttachmentValue::String(value.clone().into_inner()),
        );
    }
    if let Some(value) = context.sampled {
        ctx.insert("sampled".into(), AttachmentValue::Bool(value));
    }
    if let Some(value) = context.trace_state.as_ref() {
        ctx.insert(
            "trace_state".into(),
            AttachmentValue::String(value.as_static_ref().clone()),
        );
    }
    if let Some(value) = context.flags {
        ctx.insert(
            "flags".into(),
            AttachmentValue::Unsigned(value.bits() as u64),
        );
    }
    AttachmentValue::Object(ctx)
}

#[cfg(feature = "trace")]
fn build_trace_event_value(event: &TraceEvent) -> AttachmentValue {
    let mut map = FastMap::new();
    map.insert("name".into(), AttachmentValue::String(event.name.clone()));
    if let Some(value) = event.level {
        map.insert("level".into(), AttachmentValue::String(value.as_str().into()));
    }
    if let Some(value) = event.timestamp_unix_nano {
        map.insert("timestamp_unix_nano".into(), AttachmentValue::Unsigned(value));
    }
    map.insert(
        "attributes".into(),
        AttachmentValue::Array(
            event
                .attributes
                .iter()
                .map(|attr| {
                    let mut kv = FastMap::new();
                    kv.insert("key".into(), AttachmentValue::String(attr.key.clone()));
                    kv.insert("value".into(), attr.value.clone());
                    AttachmentValue::Object(kv)
                })
                .collect(),
        ),
    );
    AttachmentValue::Object(map)
}

#[cfg(any(feature = "trace", feature = "otel"))]
pub(crate) fn build_stack_trace_value(stack_trace: &StackTrace) -> AttachmentValue {
    let mut map = FastMap::new();
    let format = match stack_trace.format {
        crate::report::StackTraceFormat::Native => "native",
        crate::report::StackTraceFormat::Raw => "raw",
    };
    map.insert("format".into(), AttachmentValue::String(format.into()));
    map.insert(
        "frames".into(),
        AttachmentValue::Array(
            stack_trace
                .frames
                .iter()
                .map(build_stack_frame_value)
                .collect(),
        ),
    );
    if let Some(value) = stack_trace.raw.as_ref() {
        map.insert("raw".into(), AttachmentValue::String(value.clone()));
    }
    AttachmentValue::Object(map)
}

#[cfg(any(feature = "trace", feature = "otel"))]
fn build_stack_frame_value(frame: &StackFrame) -> AttachmentValue {
    let mut map = FastMap::new();
    if let Some(value) = frame.symbol.as_ref() {
        map.insert("symbol".into(), AttachmentValue::String(value.clone()));
    }
    if let Some(value) = frame.module_path.as_ref() {
        map.insert("module_path".into(), AttachmentValue::String(value.clone()));
    }
    if let Some(value) = frame.file.as_ref() {
        map.insert("file".into(), AttachmentValue::String(value.clone()));
    }
    if let Some(value) = frame.line {
        map.insert("line".into(), AttachmentValue::Unsigned(value as u64));
    }
    if let Some(value) = frame.column {
        map.insert("column".into(), AttachmentValue::Unsigned(value as u64));
    }
    AttachmentValue::Object(map)
}

#[cfg(any(feature = "trace", feature = "otel"))]
pub(crate) fn build_display_causes(
    display_causes: &[Arc<dyn Display + Send + Sync + 'static>],
    state: CauseTraversalState,
) -> AttachmentValue {
    let mut map = FastMap::new();
    map.insert(
        "items".into(),
        AttachmentValue::Array(
            display_causes
                .iter()
                .map(|v| AttachmentValue::String(v.to_string().into()))
                .collect(),
        ),
    );
    map.insert("truncated".into(), AttachmentValue::Bool(state.truncated));
    map.insert(
        "cycle_detected".into(),
        AttachmentValue::Bool(state.cycle_detected),
    );
    AttachmentValue::Object(map)
}

#[cfg(any(feature = "trace", feature = "otel"))]
pub(crate) fn build_origin_src_errs_val(source_errors: &SourceErrorChain) -> AttachmentValue {
    build_source_errors_value(source_errors, true)
}

#[cfg(any(feature = "trace", feature = "otel"))]
pub(crate) fn build_diag_src_errs_val(source_errors: &SourceErrorChain) -> AttachmentValue {
    build_source_errors_value(source_errors, false)
}

#[cfg(any(feature = "trace", feature = "otel"))]
fn build_source_errors_value(
    source_errors: &SourceErrorChain,
    hide_report_wrapper_types: bool,
) -> AttachmentValue {
    let exported = source_errors.export_with_options(hide_report_wrapper_types);
    let mut map = FastMap::new();
    map.insert(
        "roots".into(),
        AttachmentValue::Array(
            exported
                .roots
                .iter()
                .copied()
                .map(|id| AttachmentValue::Integer(id as i64))
                .collect(),
        ),
    );
    map.insert(
        "nodes".into(),
        AttachmentValue::Array(
            exported
                .nodes
                .iter()
                .map(|node| {
                    build_source_err_node(
                        &node.message,
                        node.type_name.clone(),
                        node.source_roots.as_slice(),
                    )
                })
                .collect(),
        ),
    );
    map.insert(
        "truncated".into(),
        AttachmentValue::Bool(exported.truncated),
    );
    map.insert(
        "cycle_detected".into(),
        AttachmentValue::Bool(exported.cycle_detected),
    );
    AttachmentValue::Object(map)
}

#[cfg(any(feature = "trace", feature = "otel"))]
fn build_source_err_node(
    message: &str,
    type_name: Option<StaticRefStr>,
    source_roots: &[usize],
) -> AttachmentValue {
    let mut map = FastMap::new();
    map.insert(
        "message".into(),
        AttachmentValue::String(message.to_string().into()),
    );
    if let Some(value) = type_name {
        map.insert("type".into(), AttachmentValue::String(value));
    }
    map.insert(
        "source_roots".into(),
        AttachmentValue::Array(
            source_roots
                .iter()
                .copied()
                .map(|id| AttachmentValue::Integer(id as i64))
                .collect(),
        ),
    );
    AttachmentValue::Object(map)
}
