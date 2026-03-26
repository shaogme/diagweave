use alloc::string::String;
use core::fmt::Display;

use super::{
    AttachmentValue, CauseStore, EventCauseStore, Report, ReportMetadata, Severity, StackTrace,
};
use core::error::Error;
#[cfg(feature = "trace")]
use super::{ReportTrace, TraceEvent, TraceEventAttribute, TraceEventLevel};

/// A trait for types that can be converted into a diagnostic result.
pub trait Diagnostic {
    /// The success value type.
    type Value;
    /// The error type.
    type Error;

    fn diag(self) -> Result<Self::Value, Report<Self::Error>>;

    fn diag_with<C>(self) -> Result<Self::Value, Report<Self::Error, C>>
    where
        C: CauseStore,
        Self::Error: Sized;

    fn diag_context(
        self,
        key: impl Into<String>,
        value: impl Into<AttachmentValue>,
    ) -> Result<Self::Value, Report<Self::Error>>
    where
        Self: Sized,
    {
        self.diag().with_context(key, value)
    }

    fn diag_note(self, message: impl Display) -> Result<Self::Value, Report<Self::Error>>
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

    fn diag_with<C>(self) -> Result<Self::Value, Report<Self::Error, C>>
    where
        C: CauseStore,
        Self::Error: Sized,
    {
        self.map_err(Report::<E, C>::new_with_store)
    }
}

/// Extension trait for `Result<T, Report<E, C>>` to add diagnostic information.
pub trait ReportResultExt<T, E, C = super::DefaultCauseStore>
where
    C: CauseStore,
{
    fn attach(
        self,
        key: impl Into<String>,
        value: impl Into<AttachmentValue>,
    ) -> Result<T, Report<E, C>>;

    fn attach_printable(self, message: impl Display) -> Result<T, Report<E, C>>;

    fn attach_payload(
        self,
        name: impl Into<String>,
        value: impl Into<AttachmentValue>,
        media_type: Option<String>,
    ) -> Result<T, Report<E, C>>;

    fn with_context(
        self,
        key: impl Into<String>,
        value: impl Into<AttachmentValue>,
    ) -> Result<T, Report<E, C>>;

    fn with_note(self, message: impl Display) -> Result<T, Report<E, C>>;

    fn with_payload(
        self,
        name: impl Into<String>,
        value: impl Into<AttachmentValue>,
        media_type: Option<String>,
    ) -> Result<T, Report<E, C>>;

    fn with_metadata(self, metadata: ReportMetadata) -> Result<T, Report<E, C>>;

    #[cfg(feature = "trace")]
    fn with_trace(self, trace: ReportTrace) -> Result<T, Report<E, C>>;

    #[cfg(feature = "trace")]
    fn with_trace_ids(
        self,
        trace_id: impl Into<String>,
        span_id: impl Into<String>,
    ) -> Result<T, Report<E, C>>;

    #[cfg(feature = "trace")]
    fn with_parent_span_id(self, parent_span_id: impl Into<String>) -> Result<T, Report<E, C>>;

    #[cfg(feature = "trace")]
    fn with_trace_sampled(self, sampled: bool) -> Result<T, Report<E, C>>;

    #[cfg(feature = "trace")]
    fn with_trace_state(self, trace_state: impl Into<String>) -> Result<T, Report<E, C>>;

    #[cfg(feature = "trace")]
    fn with_trace_flags(self, flags: u32) -> Result<T, Report<E, C>>;

    #[cfg(feature = "trace")]
    fn with_trace_event(self, event: TraceEvent) -> Result<T, Report<E, C>>;

    #[cfg(feature = "trace")]
    fn push_trace_event(self, name: impl Into<String>) -> Result<T, Report<E, C>>;

    #[cfg(feature = "trace")]
    fn push_trace_event_with(
        self,
        name: impl Into<String>,
        level: Option<TraceEventLevel>,
        timestamp_unix_nano: Option<u64>,
        attributes: impl IntoIterator<Item = TraceEventAttribute>,
    ) -> Result<T, Report<E, C>>;

    fn with_error_code(self, error_code: impl Into<String>) -> Result<T, Report<E, C>>;

    fn with_severity(self, severity: impl Into<Severity>) -> Result<T, Report<E, C>>;

    fn with_category(self, category: impl Into<String>) -> Result<T, Report<E, C>>;

    fn with_retryable(self, retryable: bool) -> Result<T, Report<E, C>>;

    fn with_stack_trace(self, stack_trace: StackTrace) -> Result<T, Report<E, C>>;

    fn clear_stack_trace(self) -> Result<T, Report<E, C>>;

    #[cfg(feature = "std")]
    fn capture_stack_trace(self) -> Result<T, Report<E, C>>;

    fn with_cause(self, cause: impl Display) -> Result<T, Report<E, C>>
    where
        C: EventCauseStore;

    fn with_causes<I, TCause>(self, causes: I) -> Result<T, Report<E, C>>
    where
        I: IntoIterator<Item = TCause>,
        TCause: Display,
        C: EventCauseStore;

    fn with_error_source(self, err: impl Error + 'static) -> Result<T, Report<E, C>>;

    fn context_lazy(
        self,
        key: impl Into<String>,
        make_value: impl FnOnce() -> AttachmentValue,
    ) -> Result<T, Report<E, C>>;

    fn note_lazy(self, make_message: impl FnOnce() -> String) -> Result<T, Report<E, C>>;

    fn wrap<Outer>(self, outer: Outer) -> Result<T, Report<Outer, C>>
    where
        Report<E, C>: Error + 'static;

    fn wrap_with<Outer>(self, map: impl FnOnce(E) -> Outer) -> Result<T, Report<Outer, C>>;
}


impl<T, E, C> ReportResultExt<T, E, C> for Result<T, Report<E, C>>
where
    C: CauseStore,
{
    fn attach(
        self,
        key: impl Into<String>,
        value: impl Into<AttachmentValue>,
    ) -> Result<T, Report<E, C>> {
        let key = key.into();
        self.map_err(|report| report.attach(key, value))
    }

    fn attach_printable(self, message: impl Display) -> Result<T, Report<E, C>> {
        self.map_err(|report| report.attach_printable(message))
    }

    fn attach_payload(
        self,
        name: impl Into<String>,
        value: impl Into<AttachmentValue>,
        media_type: Option<String>,
    ) -> Result<T, Report<E, C>> {
        let name = name.into();
        self.map_err(|report| report.attach_payload(name, value, media_type))
    }

    fn with_context(
        self,
        key: impl Into<String>,
        value: impl Into<AttachmentValue>,
    ) -> Result<T, Report<E, C>> {
        self.attach(key, value)
    }

    fn with_note(self, message: impl Display) -> Result<T, Report<E, C>> {
        self.attach_printable(message)
    }

    fn with_payload(
        self,
        name: impl Into<String>,
        value: impl Into<AttachmentValue>,
        media_type: Option<String>,
    ) -> Result<T, Report<E, C>> {
        self.attach_payload(name, value, media_type)
    }

    fn with_metadata(self, metadata: ReportMetadata) -> Result<T, Report<E, C>> {
        self.map_err(|report| report.with_metadata(metadata))
    }

    #[cfg(feature = "trace")]
    fn with_trace(self, trace: ReportTrace) -> Result<T, Report<E, C>> {
        self.map_err(|report| report.with_trace(trace))
    }

    #[cfg(feature = "trace")]
    fn with_trace_ids(
        self,
        trace_id: impl Into<String>,
        span_id: impl Into<String>,
    ) -> Result<T, Report<E, C>> {
        let trace_id = trace_id.into();
        let span_id = span_id.into();
        self.map_err(|report| report.with_trace_ids(trace_id, span_id))
    }

    #[cfg(feature = "trace")]
    fn with_parent_span_id(self, parent_span_id: impl Into<String>) -> Result<T, Report<E, C>> {
        let parent_span_id = parent_span_id.into();
        self.map_err(|report| report.with_parent_span_id(parent_span_id))
    }

    #[cfg(feature = "trace")]
    fn with_trace_sampled(self, sampled: bool) -> Result<T, Report<E, C>> {
        self.map_err(|report| report.with_trace_sampled(sampled))
    }

    #[cfg(feature = "trace")]
    fn with_trace_state(self, trace_state: impl Into<String>) -> Result<T, Report<E, C>> {
        let trace_state = trace_state.into();
        self.map_err(|report| report.with_trace_state(trace_state))
    }

    #[cfg(feature = "trace")]
    fn with_trace_flags(self, flags: u32) -> Result<T, Report<E, C>> {
        self.map_err(|report| report.with_trace_flags(flags))
    }

    #[cfg(feature = "trace")]
    fn with_trace_event(self, event: TraceEvent) -> Result<T, Report<E, C>> {
        self.map_err(|report| report.with_trace_event(event))
    }

    #[cfg(feature = "trace")]
    fn push_trace_event(self, name: impl Into<String>) -> Result<T, Report<E, C>> {
        let name = name.into();
        self.map_err(|report| report.push_trace_event(name))
    }

    #[cfg(feature = "trace")]
    fn push_trace_event_with(
        self,
        name: impl Into<String>,
        level: Option<TraceEventLevel>,
        timestamp_unix_nano: Option<u64>,
        attributes: impl IntoIterator<Item = TraceEventAttribute>,
    ) -> Result<T, Report<E, C>> {
        let name = name.into();
        self.map_err(|report| {
            report.push_trace_event_ext(name, level, timestamp_unix_nano, attributes)
        })
    }

    fn with_error_code(self, error_code: impl Into<String>) -> Result<T, Report<E, C>> {
        let error_code = error_code.into();
        self.map_err(|report| report.with_error_code(error_code))
    }

    fn with_severity(self, severity: impl Into<Severity>) -> Result<T, Report<E, C>> {
        let severity = severity.into();
        self.map_err(|report| report.with_severity(severity))
    }

    fn with_category(self, category: impl Into<String>) -> Result<T, Report<E, C>> {
        let category = category.into();
        self.map_err(|report| report.with_category(category))
    }

    fn with_retryable(self, retryable: bool) -> Result<T, Report<E, C>> {
        self.map_err(|report| report.with_retryable(retryable))
    }

    fn with_stack_trace(self, stack_trace: StackTrace) -> Result<T, Report<E, C>> {
        self.map_err(|report| report.with_stack_trace(stack_trace))
    }

    fn clear_stack_trace(self) -> Result<T, Report<E, C>> {
        self.map_err(|report| report.clear_stack_trace())
    }

    #[cfg(feature = "std")]
    fn capture_stack_trace(self) -> Result<T, Report<E, C>> {
        self.map_err(|report| report.capture_stack_trace())
    }

    fn with_cause(self, cause: impl Display) -> Result<T, Report<E, C>>
    where
        C: EventCauseStore,
    {
        self.map_err(|report| report.with_cause(cause))
    }

    fn with_causes<I, TCause>(self, causes: I) -> Result<T, Report<E, C>>
    where
        I: IntoIterator<Item = TCause>,
        TCause: Display,
        C: EventCauseStore,
    {
        self.map_err(|report| report.with_causes(causes))
    }

    fn with_error_source(self, err: impl Error + 'static) -> Result<T, Report<E, C>> {
        self.map_err(|report| report.with_error_source(err))
    }

    fn context_lazy(
        self,
        key: impl Into<String>,
        make_value: impl FnOnce() -> AttachmentValue,
    ) -> Result<T, Report<E, C>> {
        let key = key.into();
        self.map_err(|report| report.attach(key, make_value()))
    }

    fn note_lazy(self, make_message: impl FnOnce() -> String) -> Result<T, Report<E, C>> {
        self.map_err(|report| report.attach_printable(make_message()))
    }

    fn wrap<Outer>(self, outer: Outer) -> Result<T, Report<Outer, C>>
    where
        Report<E, C>: Error + 'static,
    {
        self.map_err(|report| report.wrap(outer))
    }

    fn wrap_with<Outer>(self, map: impl FnOnce(E) -> Outer) -> Result<T, Report<Outer, C>> {
        self.map_err(|report| report.wrap_with(map))
    }
}
