use alloc::string::String;
use core::error::Error;
use core::fmt::Display;
use ref_str::StaticRefStr;

use super::{
    Attachment, AttachmentValue, ContextValue, ErrorCode, HasSeverity, MissingSeverity, Report,
    ReportMetadata, Severity, SeverityState, StackTrace,
};
#[cfg(feature = "trace")]
use super::{
    ParentSpanId, ReportTrace, SpanId, TraceEvent, TraceEventAttribute, TraceEventLevel, TraceId,
};

/// A trait for types that can be converted into a diagnostic result.
pub trait Diagnostic {
    /// The success value type.
    type Value;
    /// The error type.
    type Error;

    fn diag(self) -> Result<Self::Value, Report<Self::Error>>;

    fn diag_note(
        self,
        message: impl Display + Send + Sync + 'static,
    ) -> Result<Self::Value, Report<Self::Error>>
    where
        Self: Sized,
    {
        self.diag().attach_note(message)
    }
}

impl<T, E> Diagnostic for Result<T, E> {
    type Value = T;
    type Error = E;

    fn diag(self) -> Result<Self::Value, Report<Self::Error>> {
        self.map_err(Report::new)
    }
}

/// Extension trait for `Result<T, Report<E, State>>` to add diagnostic information.
pub trait ReportResultExt<T, E, State = MissingSeverity>
where
    State: SeverityState,
{
    fn attach_printable(
        self,
        message: impl Display + Send + Sync + 'static,
    ) -> Result<T, Report<E, State>>;

    fn attach_payload(
        self,
        name: impl Into<StaticRefStr>,
        value: impl Into<AttachmentValue>,
        media_type: Option<impl Into<StaticRefStr>>,
    ) -> Result<T, Report<E, State>>;

    fn attach_note_lazy(self, make_message: impl FnOnce() -> String)
    -> Result<T, Report<E, State>>;

    fn attach_note(
        self,
        message: impl Display + Send + Sync + 'static,
    ) -> Result<T, Report<E, State>>;

    fn with_ctx(
        self,
        key: impl Into<StaticRefStr>,
        value: impl Into<ContextValue>,
    ) -> Result<T, Report<E, State>>;

    fn with_ctx_lazy(
        self,
        key: impl Into<StaticRefStr>,
        make_value: impl FnOnce() -> ContextValue,
    ) -> Result<T, Report<E, State>>;

    fn with_system(
        self,
        key: impl Into<StaticRefStr>,
        value: impl Into<ContextValue>,
    ) -> Result<T, Report<E, State>>;

    fn with_metadata<NewState>(
        self,
        metadata: ReportMetadata<NewState>,
    ) -> Result<T, Report<E, NewState>>
    where
        NewState: SeverityState;

    #[cfg(feature = "trace")]
    fn with_trace(self, trace: ReportTrace) -> Result<T, Report<E, State>>;

    #[cfg(feature = "trace")]
    fn with_trace_ids(self, trace_id: TraceId, span_id: SpanId) -> Result<T, Report<E, State>>;

    #[cfg(feature = "trace")]
    fn with_parent_span_id(self, parent_span_id: ParentSpanId) -> Result<T, Report<E, State>>;

    #[cfg(feature = "trace")]
    fn with_trace_sampled(self, sampled: bool) -> Result<T, Report<E, State>>;

    #[cfg(feature = "trace")]
    fn with_trace_state(self, trace_state: impl Into<StaticRefStr>) -> Result<T, Report<E, State>>;

    #[cfg(feature = "trace")]
    fn with_trace_flags(self, flags: u8) -> Result<T, Report<E, State>>;

    #[cfg(feature = "trace")]
    fn with_trace_event(self, event: TraceEvent) -> Result<T, Report<E, State>>;

    #[cfg(feature = "trace")]
    fn push_trace_event(self, name: impl Into<StaticRefStr>) -> Result<T, Report<E, State>>;

    #[cfg(feature = "trace")]
    fn push_trace_event_with(
        self,
        name: impl Into<StaticRefStr>,
        level: Option<TraceEventLevel>,
        timestamp_unix_nano: Option<u64>,
        attributes: impl IntoIterator<Item = TraceEventAttribute>,
    ) -> Result<T, Report<E, State>>;

    fn with_error_code(self, error_code: impl Into<ErrorCode>) -> Result<T, Report<E, State>>;

    fn with_severity(self, severity: Severity) -> Result<T, Report<E, HasSeverity>>;

    fn with_category(self, category: impl Into<StaticRefStr>) -> Result<T, Report<E, State>>;

    fn with_retryable(self, retryable: bool) -> Result<T, Report<E, State>>;

    fn with_stack_trace(self, stack_trace: StackTrace) -> Result<T, Report<E, State>>;

    fn clear_stack_trace(self) -> Result<T, Report<E, State>>;

    #[cfg(feature = "std")]
    fn capture_stack_trace(self) -> Result<T, Report<E, State>>;

    fn with_display_cause(
        self,
        cause: impl Display + Send + Sync + 'static,
    ) -> Result<T, Report<E, State>>;

    fn with_display_causes<I, TCause>(self, causes: I) -> Result<T, Report<E, State>>
    where
        I: IntoIterator<Item = TCause>,
        TCause: Display + Send + Sync + 'static;

    fn with_diag_src_err(
        self,
        err: impl Error + Send + Sync + 'static,
    ) -> Result<T, Report<E, State>>;

    fn boundary<Outer>(self, outer: Outer) -> Result<T, Report<Outer, MissingSeverity>>
    where
        Report<E, State>: Error + Send + Sync + 'static,
        E: Error + Send + Sync + 'static;

    fn map_inner<Outer>(self, map: impl FnOnce(E) -> Outer) -> Result<T, Report<Outer, State>>;
}

/// Read-only extension trait for `Result<T, Report<E, State>>`.
pub trait ReportResultInspectExt<T, E, State = MissingSeverity>
where
    State: SeverityState,
{
    fn report_ref(&self) -> Option<&Report<E, State>>;

    fn report_attachments(&self) -> Option<&[Attachment]>;

    fn report_metadata(&self) -> Option<&ReportMetadata<State>>;

    fn report_error_code(&self) -> Option<&ErrorCode>;

    fn report_severity(&self) -> Option<Severity>;

    fn report_category(&self) -> Option<&str>;

    fn report_retryable(&self) -> Option<bool>;
}

impl<T, E, State> ReportResultExt<T, E, State> for Result<T, Report<E, State>>
where
    State: SeverityState,
{
    fn attach_printable(
        self,
        message: impl Display + Send + Sync + 'static,
    ) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.attach_printable(message))
    }

    fn attach_note(
        self,
        message: impl Display + Send + Sync + 'static,
    ) -> Result<T, Report<E, State>> {
        self.attach_printable(message)
    }

    fn attach_payload(
        self,
        name: impl Into<StaticRefStr>,
        value: impl Into<AttachmentValue>,
        media_type: Option<impl Into<StaticRefStr>>,
    ) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.attach_payload(name, value, media_type))
    }

    fn with_ctx(
        self,
        key: impl Into<StaticRefStr>,
        value: impl Into<ContextValue>,
    ) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.with_ctx(key, value))
    }

    fn with_system(
        self,
        key: impl Into<StaticRefStr>,
        value: impl Into<ContextValue>,
    ) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.with_system(key, value))
    }

    fn with_metadata<NewState>(
        self,
        metadata: ReportMetadata<NewState>,
    ) -> Result<T, Report<E, NewState>>
    where
        NewState: SeverityState,
    {
        self.map_err(|report| report.with_metadata(metadata))
    }

    #[cfg(feature = "trace")]
    fn with_trace(self, trace: ReportTrace) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.with_trace(trace))
    }

    #[cfg(feature = "trace")]
    fn with_trace_ids(self, trace_id: TraceId, span_id: SpanId) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.with_trace_ids(trace_id, span_id))
    }

    #[cfg(feature = "trace")]
    fn with_parent_span_id(self, parent_span_id: ParentSpanId) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.with_parent_span_id(parent_span_id))
    }

    #[cfg(feature = "trace")]
    fn with_trace_sampled(self, sampled: bool) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.with_trace_sampled(sampled))
    }

    #[cfg(feature = "trace")]
    fn with_trace_state(self, trace_state: impl Into<StaticRefStr>) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.with_trace_state(trace_state))
    }

    #[cfg(feature = "trace")]
    fn with_trace_flags(self, flags: u8) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.with_trace_flags(flags))
    }

    #[cfg(feature = "trace")]
    fn with_trace_event(self, event: TraceEvent) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.with_trace_event(event))
    }

    #[cfg(feature = "trace")]
    fn push_trace_event(self, name: impl Into<StaticRefStr>) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.push_trace_event(name))
    }

    #[cfg(feature = "trace")]
    fn push_trace_event_with(
        self,
        name: impl Into<StaticRefStr>,
        level: Option<TraceEventLevel>,
        timestamp_unix_nano: Option<u64>,
        attributes: impl IntoIterator<Item = TraceEventAttribute>,
    ) -> Result<T, Report<E, State>> {
        self.map_err(|report| {
            report.push_trace_event_with(name, level, timestamp_unix_nano, attributes)
        })
    }

    fn with_error_code(self, error_code: impl Into<ErrorCode>) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.with_error_code(error_code))
    }

    fn with_severity(self, severity: Severity) -> Result<T, Report<E, HasSeverity>> {
        self.map_err(|report| report.with_severity(severity))
    }

    fn with_category(self, category: impl Into<StaticRefStr>) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.with_category(category))
    }

    fn with_retryable(self, retryable: bool) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.with_retryable(retryable))
    }

    fn with_stack_trace(self, stack_trace: StackTrace) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.with_stack_trace(stack_trace))
    }

    fn clear_stack_trace(self) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.clear_stack_trace())
    }

    #[cfg(feature = "std")]
    fn capture_stack_trace(self) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.capture_stack_trace())
    }

    fn with_display_cause(
        self,
        cause: impl Display + Send + Sync + 'static,
    ) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.with_display_cause(cause))
    }

    fn with_display_causes<I, TCause>(self, causes: I) -> Result<T, Report<E, State>>
    where
        I: IntoIterator<Item = TCause>,
        TCause: Display + Send + Sync + 'static,
    {
        self.map_err(|report| report.with_display_causes(causes))
    }

    fn with_diag_src_err(
        self,
        err: impl Error + Send + Sync + 'static,
    ) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.with_diag_src_err(err))
    }

    fn with_ctx_lazy(
        self,
        key: impl Into<StaticRefStr>,
        make_value: impl FnOnce() -> ContextValue,
    ) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.with_ctx(key, make_value()))
    }

    fn attach_note_lazy(
        self,
        make_message: impl FnOnce() -> String,
    ) -> Result<T, Report<E, State>> {
        self.map_err(|report| report.attach_printable(make_message()))
    }

    fn boundary<Outer>(self, outer: Outer) -> Result<T, Report<Outer, MissingSeverity>>
    where
        Report<E, State>: Error + Send + Sync + 'static,
        E: Error + Send + Sync + 'static,
    {
        self.map_err(|report| report.boundary(outer))
    }

    fn map_inner<Outer>(self, map: impl FnOnce(E) -> Outer) -> Result<T, Report<Outer, State>> {
        self.map_err(|report| report.map_err(map))
    }
}

impl<T, E, State> ReportResultInspectExt<T, E, State> for Result<T, Report<E, State>>
where
    State: SeverityState,
{
    fn report_ref(&self) -> Option<&Report<E, State>> {
        self.as_ref().err()
    }

    fn report_attachments(&self) -> Option<&[Attachment]> {
        self.report_ref().map(Report::attachments)
    }

    fn report_metadata(&self) -> Option<&ReportMetadata<State>> {
        self.report_ref().map(Report::metadata)
    }

    fn report_error_code(&self) -> Option<&ErrorCode> {
        self.report_ref().and_then(Report::error_code)
    }

    fn report_severity(&self) -> Option<Severity> {
        self.report_ref().and_then(Report::severity)
    }

    fn report_category(&self) -> Option<&str> {
        self.report_ref().and_then(Report::category)
    }

    fn report_retryable(&self) -> Option<bool> {
        self.report_ref().and_then(Report::retryable)
    }
}
