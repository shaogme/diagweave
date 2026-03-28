use alloc::string::String;
use core::fmt::Display;
use ref_str::StaticRefStr;

use super::{Attachment, AttachmentValue, ErrorCode, Report, ReportMetadata, Severity, StackTrace};
#[cfg(feature = "trace")]
use super::{
    ParentSpanId, ReportTrace, SpanId, TraceEvent, TraceEventAttribute, TraceEventLevel, TraceId,
};
use core::error::Error;

/// A trait for types that can be converted into a diagnostic result.
pub trait Diagnostic {
    /// The success value type.
    type Value;
    /// The error type.
    type Error;

    fn diag(self) -> Result<Self::Value, Report<Self::Error>>;

    fn diag_context(
        self,
        key: impl Into<StaticRefStr>,
        value: impl Into<AttachmentValue>,
    ) -> Result<Self::Value, Report<Self::Error>>
    where
        Self: Sized,
    {
        self.diag().with_context(key, value)
    }

    fn diag_note(
        self,
        message: impl Display + Send + Sync + 'static,
    ) -> Result<Self::Value, Report<Self::Error>>
    where
        Self: Sized,
    {
        self.diag().with_note(message)
    }
}

impl<T, E> Diagnostic for Result<T, E> {
    type Value = T;
    type Error = E;

    fn diag(self) -> Result<Self::Value, Report<Self::Error>> {
        self.map_err(Report::new)
    }
}

/// Extension trait for `Result<T, Report<E>>` to add diagnostic information.
pub trait ReportResultExt<T, E> {
    fn attach(
        self,
        key: impl Into<StaticRefStr>,
        value: impl Into<AttachmentValue>,
    ) -> Result<T, Report<E>>;

    fn attach_printable(
        self,
        message: impl Display + Send + Sync + 'static,
    ) -> Result<T, Report<E>>;

    fn attach_payload(
        self,
        name: impl Into<StaticRefStr>,
        value: impl Into<AttachmentValue>,
        media_type: Option<impl Into<StaticRefStr>>,
    ) -> Result<T, Report<E>>;

    fn with_context(
        self,
        key: impl Into<StaticRefStr>,
        value: impl Into<AttachmentValue>,
    ) -> Result<T, Report<E>>;

    fn with_note(self, message: impl Display + Send + Sync + 'static) -> Result<T, Report<E>>;

    fn with_payload(
        self,
        name: impl Into<StaticRefStr>,
        value: impl Into<AttachmentValue>,
        media_type: Option<impl Into<StaticRefStr>>,
    ) -> Result<T, Report<E>>;

    fn with_metadata(self, metadata: ReportMetadata) -> Result<T, Report<E>>;

    #[cfg(feature = "trace")]
    fn with_trace(self, trace: ReportTrace) -> Result<T, Report<E>>;

    #[cfg(feature = "trace")]
    fn with_trace_ids(self, trace_id: TraceId, span_id: SpanId) -> Result<T, Report<E>>;

    #[cfg(feature = "trace")]
    fn with_parent_span_id(self, parent_span_id: ParentSpanId) -> Result<T, Report<E>>;

    #[cfg(feature = "trace")]
    fn with_trace_sampled(self, sampled: bool) -> Result<T, Report<E>>;

    #[cfg(feature = "trace")]
    fn with_trace_state(self, trace_state: impl Into<StaticRefStr>) -> Result<T, Report<E>>;

    #[cfg(feature = "trace")]
    fn with_trace_flags(self, flags: u8) -> Result<T, Report<E>>;

    #[cfg(feature = "trace")]
    fn with_trace_event(self, event: TraceEvent) -> Result<T, Report<E>>;

    #[cfg(feature = "trace")]
    fn push_trace_event(self, name: impl Into<StaticRefStr>) -> Result<T, Report<E>>;

    #[cfg(feature = "trace")]
    fn push_trace_event_with(
        self,
        name: impl Into<StaticRefStr>,
        level: Option<TraceEventLevel>,
        timestamp_unix_nano: Option<u64>,
        attributes: impl IntoIterator<Item = TraceEventAttribute>,
    ) -> Result<T, Report<E>>;

    fn with_error_code(self, error_code: impl Into<ErrorCode>) -> Result<T, Report<E>>;

    fn with_severity(self, severity: impl Into<Severity>) -> Result<T, Report<E>>;

    fn with_category(self, category: impl Into<StaticRefStr>) -> Result<T, Report<E>>;

    fn with_retryable(self, retryable: bool) -> Result<T, Report<E>>;

    fn with_stack_trace(self, stack_trace: StackTrace) -> Result<T, Report<E>>;

    fn clear_stack_trace(self) -> Result<T, Report<E>>;

    #[cfg(feature = "std")]
    fn capture_stack_trace(self) -> Result<T, Report<E>>;

    fn with_display_cause(
        self,
        cause: impl Display + Send + Sync + 'static,
    ) -> Result<T, Report<E>>;

    fn with_display_causes<I, TCause>(self, causes: I) -> Result<T, Report<E>>
    where
        I: IntoIterator<Item = TCause>,
        TCause: Display + Send + Sync + 'static;

    fn with_diag_src_err(self, err: impl Error + Send + Sync + 'static) -> Result<T, Report<E>>;

    fn context_lazy(
        self,
        key: impl Into<StaticRefStr>,
        make_value: impl FnOnce() -> AttachmentValue,
    ) -> Result<T, Report<E>>;

    fn note_lazy(self, make_message: impl FnOnce() -> String) -> Result<T, Report<E>>;

    fn wrap<Outer>(self, outer: Outer) -> Result<T, Report<Outer>>
    where
        Report<E>: Error + Send + Sync + 'static,
        E: Error + Send + Sync + 'static;

    fn wrap_with<Outer>(self, map: impl FnOnce(E) -> Outer) -> Result<T, Report<Outer>>;
}

/// Read-only extension trait for `Result<T, Report<E>>`.
pub trait ReportResultInspectExt<T, E> {
    fn report_ref(&self) -> Option<&Report<E>>;

    fn report_attachments(&self) -> Option<&[Attachment]>;

    fn report_metadata(&self) -> Option<&ReportMetadata>;

    fn report_error_code(&self) -> Option<&ErrorCode>;

    fn report_severity(&self) -> Option<Severity>;

    fn report_category(&self) -> Option<&str>;

    fn report_retryable(&self) -> Option<bool>;
}

impl<T, E> ReportResultExt<T, E> for Result<T, Report<E>> {
    fn attach(
        self,
        key: impl Into<StaticRefStr>,
        value: impl Into<AttachmentValue>,
    ) -> Result<T, Report<E>> {
        let key = key.into();
        self.map_err(|report| report.attach(key, value))
    }

    fn attach_printable(
        self,
        message: impl Display + Send + Sync + 'static,
    ) -> Result<T, Report<E>> {
        self.map_err(|report| report.attach_printable(message))
    }

    fn attach_payload(
        self,
        name: impl Into<StaticRefStr>,
        value: impl Into<AttachmentValue>,
        media_type: Option<impl Into<StaticRefStr>>,
    ) -> Result<T, Report<E>> {
        self.map_err(|report| report.attach_payload(name, value, media_type))
    }

    fn with_context(
        self,
        key: impl Into<StaticRefStr>,
        value: impl Into<AttachmentValue>,
    ) -> Result<T, Report<E>> {
        self.attach(key, value)
    }

    fn with_note(self, message: impl Display + Send + Sync + 'static) -> Result<T, Report<E>> {
        self.attach_printable(message)
    }

    fn with_payload(
        self,
        name: impl Into<StaticRefStr>,
        value: impl Into<AttachmentValue>,
        media_type: Option<impl Into<StaticRefStr>>,
    ) -> Result<T, Report<E>> {
        self.attach_payload(name, value, media_type)
    }

    fn with_metadata(self, metadata: ReportMetadata) -> Result<T, Report<E>> {
        self.map_err(|report| report.with_metadata(metadata))
    }

    #[cfg(feature = "trace")]
    fn with_trace(self, trace: ReportTrace) -> Result<T, Report<E>> {
        self.map_err(|report| report.with_trace(trace))
    }

    #[cfg(feature = "trace")]
    fn with_trace_ids(self, trace_id: TraceId, span_id: SpanId) -> Result<T, Report<E>> {
        self.map_err(|report| report.with_trace_ids(trace_id, span_id))
    }

    #[cfg(feature = "trace")]
    fn with_parent_span_id(self, parent_span_id: ParentSpanId) -> Result<T, Report<E>> {
        self.map_err(|report| report.with_parent_span_id(parent_span_id))
    }

    #[cfg(feature = "trace")]
    fn with_trace_sampled(self, sampled: bool) -> Result<T, Report<E>> {
        self.map_err(|report| report.with_trace_sampled(sampled))
    }

    #[cfg(feature = "trace")]
    fn with_trace_state(self, trace_state: impl Into<StaticRefStr>) -> Result<T, Report<E>> {
        self.map_err(|report| report.with_trace_state(trace_state))
    }

    #[cfg(feature = "trace")]
    fn with_trace_flags(self, flags: u8) -> Result<T, Report<E>> {
        self.map_err(|report| report.with_trace_flags(flags))
    }

    #[cfg(feature = "trace")]
    fn with_trace_event(self, event: TraceEvent) -> Result<T, Report<E>> {
        self.map_err(|report| report.with_trace_event(event))
    }

    #[cfg(feature = "trace")]
    fn push_trace_event(self, name: impl Into<StaticRefStr>) -> Result<T, Report<E>> {
        self.map_err(|report| report.push_trace_event(name))
    }

    #[cfg(feature = "trace")]
    fn push_trace_event_with(
        self,
        name: impl Into<StaticRefStr>,
        level: Option<TraceEventLevel>,
        timestamp_unix_nano: Option<u64>,
        attributes: impl IntoIterator<Item = TraceEventAttribute>,
    ) -> Result<T, Report<E>> {
        self.map_err(|report| {
            report.push_trace_event_ext(name, level, timestamp_unix_nano, attributes)
        })
    }

    fn with_error_code(self, error_code: impl Into<ErrorCode>) -> Result<T, Report<E>> {
        self.map_err(|report| report.with_error_code(error_code))
    }

    fn with_severity(self, severity: impl Into<Severity>) -> Result<T, Report<E>> {
        let severity = severity.into();
        self.map_err(|report| report.with_severity(severity))
    }

    fn with_category(self, category: impl Into<StaticRefStr>) -> Result<T, Report<E>> {
        self.map_err(|report| report.with_category(category))
    }

    fn with_retryable(self, retryable: bool) -> Result<T, Report<E>> {
        self.map_err(|report| report.with_retryable(retryable))
    }

    fn with_stack_trace(self, stack_trace: StackTrace) -> Result<T, Report<E>> {
        self.map_err(|report| report.with_stack_trace(stack_trace))
    }

    fn clear_stack_trace(self) -> Result<T, Report<E>> {
        self.map_err(|report| report.clear_stack_trace())
    }

    #[cfg(feature = "std")]
    fn capture_stack_trace(self) -> Result<T, Report<E>> {
        self.map_err(|report| report.capture_stack_trace())
    }

    fn with_display_cause(
        self,
        cause: impl Display + Send + Sync + 'static,
    ) -> Result<T, Report<E>> {
        self.map_err(|report| report.with_display_cause(cause))
    }

    fn with_display_causes<I, TCause>(self, causes: I) -> Result<T, Report<E>>
    where
        I: IntoIterator<Item = TCause>,
        TCause: Display + Send + Sync + 'static,
    {
        self.map_err(|report| report.with_display_causes(causes))
    }

    fn with_diag_src_err(self, err: impl Error + Send + Sync + 'static) -> Result<T, Report<E>> {
        self.map_err(|report| report.with_diag_src_err(err))
    }

    fn context_lazy(
        self,
        key: impl Into<StaticRefStr>,
        make_value: impl FnOnce() -> AttachmentValue,
    ) -> Result<T, Report<E>> {
        self.map_err(|report| report.attach(key, make_value()))
    }

    fn note_lazy(self, make_message: impl FnOnce() -> String) -> Result<T, Report<E>> {
        self.map_err(|report| report.attach_printable(make_message()))
    }

    fn wrap<Outer>(self, outer: Outer) -> Result<T, Report<Outer>>
    where
        Report<E>: Error + Send + Sync + 'static,
        E: Error + Send + Sync + 'static,
    {
        self.map_err(|report| report.wrap(outer))
    }

    fn wrap_with<Outer>(self, map: impl FnOnce(E) -> Outer) -> Result<T, Report<Outer>> {
        self.map_err(|report| report.wrap_with(map))
    }
}

impl<T, E> ReportResultInspectExt<T, E> for Result<T, Report<E>> {
    fn report_ref(&self) -> Option<&Report<E>> {
        self.as_ref().err()
    }

    fn report_attachments(&self) -> Option<&[Attachment]> {
        self.report_ref().map(Report::attachments)
    }

    fn report_metadata(&self) -> Option<&ReportMetadata> {
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
