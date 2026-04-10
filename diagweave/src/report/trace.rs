use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt::{self, Display, Formatter};
use ref_str::StaticRefStr;

use super::{Report, SeverityState, types::AttachmentValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Severity level for a trace event.
pub enum TraceEventLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Display for TraceEventLevel {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        };
        write!(f, "{label}")
    }
}

#[derive(Debug, Clone, PartialEq)]
/// A key-value attribute attached to a trace event.
pub struct TraceEventAttribute {
    pub key: StaticRefStr,
    pub value: AttachmentValue,
}

impl Default for TraceEventAttribute {
    fn default() -> Self {
        Self {
            key: "".into(),
            value: AttachmentValue::Null,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
/// A single event emitted within a trace.
pub struct TraceEvent {
    pub name: StaticRefStr,
    pub level: Option<TraceEventLevel>,
    pub timestamp_unix_nano: Option<u64>,
    pub attributes: Vec<TraceEventAttribute>,
}

impl Default for TraceEvent {
    fn default() -> Self {
        Self {
            name: "".into(),
            level: None,
            timestamp_unix_nano: None,
            attributes: Vec::new(),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
/// Trace context values associated with a report.
pub struct TraceContext {
    pub trace_id: Option<TraceId>,
    pub span_id: Option<SpanId>,
    pub parent_span_id: Option<ParentSpanId>,
    pub sampled: Option<bool>,
    pub trace_state: Option<TraceState>,
    pub flags: Option<TraceFlags>,
}

impl TraceContext {
    /// Returns true if the trace context is empty (no IDs or flags).
    pub fn is_empty(&self) -> bool {
        self.trace_id.is_none()
            && self.span_id.is_none()
            && self.parent_span_id.is_none()
            && self.sampled.is_none()
            && self.trace_state.is_none()
            && self.flags.is_none()
    }
}

/// Inner trace payload attached to a report.
/// This struct contains the actual trace data and is boxed inside ReportMetadata.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct ReportTraceInner {
    context: TraceContext,
    events: Vec<TraceEvent>,
}

impl ReportTraceInner {
    /// Returns true if the report trace is empty (no context and no events).
    fn is_empty(&self) -> bool {
        self.context.is_empty() && self.events.is_empty()
    }
}

/// Trace payload attached to a report.
///
/// Contains trace context (trace ID, span ID, etc.) and trace events.
/// Uses lazy allocation via `Option<Box<ReportTraceInner>>` to minimize
/// overhead when no trace information is present.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReportTrace {
    inner: Option<Box<ReportTraceInner>>,
}

impl ReportTrace {
    /// Creates a new empty ReportTrace.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if the report trace is empty (no context and no events).
    pub fn is_empty(&self) -> bool {
        self.inner.as_ref().is_none_or(|inner| inner.is_empty())
    }

    /// Returns the trace context, if any.
    pub fn context(&self) -> Option<&TraceContext> {
        self.inner.as_ref().map(|inner| &inner.context)
    }

    /// Returns the trace events, if any.
    pub fn events(&self) -> Option<&[TraceEvent]> {
        self.inner.as_ref().map(|inner| inner.events.as_slice())
    }

    /// Ensures the inner trace data is allocated, creating it if necessary.
    fn ensure_inner(&mut self) -> &mut ReportTraceInner {
        self.inner
            .get_or_insert_with(|| Box::new(ReportTraceInner::default()))
    }

    /// Returns a mutable reference to the trace context, if any.
    pub fn context_mut(&mut self) -> Option<&mut TraceContext> {
        self.inner.as_mut().map(|inner| &mut inner.context)
    }

    /// Sets the trace context, replacing any existing value.
    pub fn set_context(mut self, context: TraceContext) -> Self {
        self.ensure_inner().context = context;
        self
    }

    /// Sets the trace context only if not already set.
    pub fn with_context(mut self, context: TraceContext) -> Self {
        if self.context().is_none() || self.context().is_none_or(|c| c.is_empty()) {
            self.ensure_inner().context = context;
        }
        self
    }

    /// Adds a trace event.
    pub fn with_event(mut self, event: TraceEvent) -> Self {
        self.ensure_inner().events.push(event);
        self
    }

    /// Sets the trace ID, replacing any existing value.
    pub fn set_trace_id(mut self, trace_id: TraceId) -> Self {
        self.ensure_inner().context.trace_id = Some(trace_id);
        self
    }

    /// Sets the trace ID only if not already set.
    pub fn with_trace_id(mut self, trace_id: TraceId) -> Self {
        if self.context().and_then(|c| c.trace_id.as_ref()).is_none() {
            self.ensure_inner().context.trace_id = Some(trace_id);
        }
        self
    }

    /// Sets the span ID, replacing any existing value.
    pub fn set_span_id(mut self, span_id: SpanId) -> Self {
        self.ensure_inner().context.span_id = Some(span_id);
        self
    }

    /// Sets the span ID only if not already set.
    pub fn with_span_id(mut self, span_id: SpanId) -> Self {
        if self.context().and_then(|c| c.span_id.as_ref()).is_none() {
            self.ensure_inner().context.span_id = Some(span_id);
        }
        self
    }

    /// Sets the parent span ID, replacing any existing value.
    pub fn set_parent_span_id(mut self, parent_span_id: ParentSpanId) -> Self {
        self.ensure_inner().context.parent_span_id = Some(parent_span_id);
        self
    }

    /// Sets the parent span ID only if not already set.
    pub fn with_parent_span_id(mut self, parent_span_id: ParentSpanId) -> Self {
        if self
            .context()
            .and_then(|c| c.parent_span_id.as_ref())
            .is_none()
        {
            self.ensure_inner().context.parent_span_id = Some(parent_span_id);
        }
        self
    }

    /// Sets whether the trace is sampled, replacing any existing value.
    pub fn set_sampled(mut self, sampled: bool) -> Self {
        let inner = self.ensure_inner();
        inner.context.sampled = Some(sampled);
        sync_flags_with_sampled(&mut inner.context);
        self
    }

    /// Sets whether the trace is sampled only if not already set.
    pub fn with_sampled(mut self, sampled: bool) -> Self {
        if self.context().and_then(|c| c.sampled).is_none() {
            let inner = self.ensure_inner();
            inner.context.sampled = Some(sampled);
            sync_flags_with_sampled(&mut inner.context);
        }
        self
    }

    /// Sets the trace state, replacing any existing value.
    pub fn set_trace_state(mut self, trace_state: impl Into<StaticRefStr>) -> Self {
        self.ensure_inner().context.trace_state = Some(TraceState::from(trace_state.into()));
        self
    }

    /// Sets the trace state only if not already set.
    pub fn with_trace_state(mut self, trace_state: impl Into<StaticRefStr>) -> Self {
        if self
            .context()
            .and_then(|c| c.trace_state.as_ref())
            .is_none()
        {
            self.ensure_inner().context.trace_state = Some(TraceState::from(trace_state.into()));
        }
        self
    }

    /// Sets the trace flags, replacing any existing value.
    pub fn set_flags(mut self, flags: impl Into<TraceFlags>) -> Self {
        let inner = self.ensure_inner();
        inner.context.flags = Some(flags.into());
        sync_sampled_with_flags(&mut inner.context);
        self
    }

    /// Sets the trace flags only if not already set.
    pub fn with_flags(mut self, flags: impl Into<TraceFlags>) -> Self {
        if self.context().and_then(|c| c.flags.as_ref()).is_none() {
            let inner = self.ensure_inner();
            inner.context.flags = Some(flags.into());
            sync_sampled_with_flags(&mut inner.context);
        }
        self
    }

    /// Sets the trace ID from an Option, only if not already set.
    pub fn set_trace_id_opt(mut self, trace_id: Option<TraceId>) -> Self {
        if let Some(tid) = trace_id
            && self.context().and_then(|c| c.trace_id.as_ref()).is_none()
        {
            self.ensure_inner().context.trace_id = Some(tid);
        }
        self
    }

    /// Sets the span ID from an Option, only if not already set.
    pub fn set_span_id_opt(mut self, span_id: Option<SpanId>) -> Self {
        if let Some(sid) = span_id
            && self.context().and_then(|c| c.span_id.as_ref()).is_none()
        {
            self.ensure_inner().context.span_id = Some(sid);
        }
        self
    }

    /// Sets the parent span ID from an Option, only if not already set.
    pub fn set_parent_span_id_opt(mut self, parent_span_id: Option<ParentSpanId>) -> Self {
        if let Some(psid) = parent_span_id
            && self
                .context()
                .and_then(|c| c.parent_span_id.as_ref())
                .is_none()
        {
            self.ensure_inner().context.parent_span_id = Some(psid);
        }
        self
    }

    /// Sets the sampled flag from an Option, only if not already set.
    pub fn set_sampled_opt(mut self, sampled: Option<bool>) -> Self {
        if let Some(s) = sampled
            && self.context().and_then(|c| c.sampled).is_none()
        {
            let inner = self.ensure_inner();
            inner.context.sampled = Some(s);
            sync_flags_with_sampled(&mut inner.context);
        }
        self
    }

    /// Sets the trace state from an Option, only if not already set.
    pub fn set_trace_state_opt(mut self, trace_state: Option<TraceState>) -> Self {
        if let Some(ts) = trace_state
            && self
                .context()
                .and_then(|c| c.trace_state.as_ref())
                .is_none()
        {
            self.ensure_inner().context.trace_state = Some(ts);
        }
        self
    }

    /// Sets the trace flags from an Option, only if not already set.
    pub fn set_flags_opt(mut self, flags: Option<TraceFlags>) -> Self {
        if let Some(f) = flags
            && self.context().and_then(|c| c.flags.as_ref()).is_none()
        {
            let inner = self.ensure_inner();
            inner.context.flags = Some(f);
            sync_sampled_with_flags(&mut inner.context);
        }
        self
    }

    /// Returns a mutable reference to the inner events, allocating if necessary.
    fn events_mut(&mut self) -> &mut Vec<TraceEvent> {
        &mut self.ensure_inner().events
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Fixed-length non-zero hexadecimal identifier.
pub struct HexId<const N: usize>(StaticRefStr);

impl<const N: usize> HexId<N> {
    /// Creates a validated hexadecimal identifier.
    pub fn new(value: impl Into<StaticRefStr>) -> Result<Self, ()> {
        HexId::try_from(value.into())
    }

    /// Creates an identifier without validation.
    ///
    /// # Safety
    /// The caller must ensure `value` is a valid, non-zero hex string of length `N`.
    pub unsafe fn new_unchecked(value: impl Into<StaticRefStr>) -> Self {
        Self(value.into())
    }

    /// Returns whether the input is a valid identifier for this width.
    pub fn is_valid(value: &str) -> bool {
        if value.len() != N {
            return false;
        }
        if value.bytes().all(|b| b == b'0') {
            return false;
        }
        value.bytes().all(|b: u8| b.is_ascii_hexdigit())
    }

    /// Returns the owned inner string.
    pub fn into_inner(self) -> StaticRefStr {
        self.0
    }
}

impl<const N: usize> TryFrom<StaticRefStr> for HexId<N> {
    type Error = ();
    fn try_from(value: StaticRefStr) -> Result<Self, Self::Error> {
        if Self::is_valid(value.as_str()) {
            Ok(Self(value))
        } else {
            Err(())
        }
    }
}

impl<const N: usize> TryFrom<&'static str> for HexId<N> {
    type Error = ();
    fn try_from(value: &'static str) -> Result<Self, Self::Error> {
        if Self::is_valid(value) {
            Ok(Self(value.into()))
        } else {
            Err(())
        }
    }
}

impl<const N: usize> AsRef<str> for HexId<N> {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl<const N: usize> Display for HexId<N> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.as_ref())
    }
}

/// 16-byte trace id encoded as 32 lowercase hex chars.
pub type TraceId = HexId<32>;
/// 8-byte span id encoded as 16 lowercase hex chars.
pub type SpanId = HexId<16>;
/// Parent span id encoded as 16 lowercase hex chars.
pub type ParentSpanId = HexId<16>;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct TraceState(StaticRefStr);

impl TraceState {
    pub fn new(value: impl Into<StaticRefStr>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }

    pub fn as_static_ref(&self) -> &StaticRefStr {
        &self.0
    }
}

impl From<StaticRefStr> for TraceState {
    fn from(value: StaticRefStr) -> Self {
        Self(value)
    }
}

impl Display for TraceState {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct TraceFlags(u8);

impl TraceFlags {
    pub fn new(value: u8) -> Self {
        Self(value)
    }

    pub fn bits(self) -> u8 {
        self.0
    }
}

impl From<u8> for TraceFlags {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl Display for TraceFlags {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<E, State> Report<E, State>
where
    State: SeverityState,
{
    /// Returns the trace information associated with the report, if any.
    pub fn trace(&self) -> &ReportTrace {
        &self.trace
    }

    /// Sets the trace information for the report, replacing any existing value.
    pub fn set_trace(mut self, trace: ReportTrace) -> Self {
        self.trace = trace;
        self
    }

    /// Sets the trace information only if not already present.
    pub fn with_trace(mut self, trace: ReportTrace) -> Self {
        if self.trace.is_empty() {
            self.trace = trace;
        }
        self
    }

    /// Sets the trace and span IDs for the report, replacing any existing values.
    pub fn set_trace_ids(mut self, trace_id: TraceId, span_id: SpanId) -> Self {
        let inner = self.trace_mut().ensure_inner();
        inner.context.trace_id = Some(trace_id);
        inner.context.span_id = Some(span_id);
        self
    }

    /// Sets the trace and span IDs only if not already set.
    pub fn with_trace_ids(mut self, trace_id: TraceId, span_id: SpanId) -> Self {
        let trace_ref = self.trace();
        let needs_trace_id = trace_ref.is_empty()
            || trace_ref
                .context()
                .and_then(|c| c.trace_id.as_ref())
                .is_none();
        let needs_span_id = trace_ref.is_empty()
            || trace_ref
                .context()
                .and_then(|c| c.span_id.as_ref())
                .is_none();
        if needs_trace_id {
            self.trace_mut().ensure_inner().context.trace_id = Some(trace_id);
        }
        if needs_span_id {
            self.trace_mut().ensure_inner().context.span_id = Some(span_id);
        }
        self
    }

    /// Sets the parent span ID for the report, replacing any existing value.
    pub fn set_parent_span_id(mut self, parent_span_id: ParentSpanId) -> Self {
        self.trace_mut().ensure_inner().context.parent_span_id = Some(parent_span_id);
        self
    }

    /// Sets the parent span ID only if not already set.
    pub fn with_parent_span_id(mut self, parent_span_id: ParentSpanId) -> Self {
        if self
            .trace()
            .context()
            .and_then(|c| c.parent_span_id.as_ref())
            .is_none()
        {
            self.trace_mut().ensure_inner().context.parent_span_id = Some(parent_span_id);
        }
        self
    }

    /// Sets whether the trace is sampled, replacing any existing value.
    pub fn set_trace_sampled(mut self, sampled: bool) -> Self {
        let inner = self.trace_mut().ensure_inner();
        inner.context.sampled = Some(sampled);
        sync_flags_with_sampled(&mut inner.context);
        self
    }

    /// Sets whether the trace is sampled only if not already set.
    pub fn with_trace_sampled(mut self, sampled: bool) -> Self {
        if self.trace().context().and_then(|c| c.sampled).is_none() {
            let inner = self.trace_mut().ensure_inner();
            inner.context.sampled = Some(sampled);
            sync_flags_with_sampled(&mut inner.context);
        }
        self
    }

    /// Sets the trace state, replacing any existing value.
    pub fn set_trace_state(mut self, trace_state: impl Into<StaticRefStr>) -> Self {
        self.trace_mut().ensure_inner().context.trace_state =
            Some(TraceState::from(trace_state.into()));
        self
    }

    /// Sets the trace state only if not already set.
    pub fn with_trace_state(mut self, trace_state: impl Into<StaticRefStr>) -> Self {
        if self
            .trace()
            .context()
            .and_then(|c| c.trace_state.as_ref())
            .is_none()
        {
            self.trace_mut().ensure_inner().context.trace_state =
                Some(TraceState::from(trace_state.into()));
        }
        self
    }

    /// Sets the trace flags, replacing any existing value.
    pub fn set_trace_flags(mut self, flags: impl Into<TraceFlags>) -> Self {
        let inner = self.trace_mut().ensure_inner();
        inner.context.flags = Some(flags.into());
        sync_sampled_with_flags(&mut inner.context);
        self
    }

    /// Sets the trace flags only if not already set.
    pub fn with_trace_flags(mut self, flags: impl Into<TraceFlags>) -> Self {
        if self
            .trace()
            .context()
            .and_then(|c| c.flags.as_ref())
            .is_none()
        {
            let inner = self.trace_mut().ensure_inner();
            inner.context.flags = Some(flags.into());
            sync_sampled_with_flags(&mut inner.context);
        }
        self
    }

    /// Adds a trace event to the report.
    pub fn with_trace_event(mut self, event: TraceEvent) -> Self {
        self.trace_mut().events_mut().push(event);
        self
    }

    /// Pushes a trace event with the specified name.
    pub fn push_trace_event(mut self, name: impl Into<StaticRefStr>) -> Self {
        self.trace_mut().events_mut().push(TraceEvent {
            name: name.into(),
            ..TraceEvent::default()
        });
        self
    }

    /// Pushes a trace event with detailed information.
    pub fn push_trace_event_with(
        mut self,
        name: impl Into<StaticRefStr>,
        level: Option<TraceEventLevel>,
        timestamp_unix_nano: Option<u64>,
        attributes: impl IntoIterator<Item = TraceEventAttribute>,
    ) -> Self {
        self.trace_mut().events_mut().push(TraceEvent {
            name: name.into(),
            level,
            timestamp_unix_nano,
            attributes: attributes.into_iter().collect::<Vec<_>>(),
        });
        self
    }

    fn trace_mut(&mut self) -> &mut ReportTrace {
        &mut self.trace
    }
}

#[cfg(feature = "trace")]
fn sync_flags_with_sampled(context: &mut TraceContext) {
    let Some(sampled) = context.sampled else {
        return;
    };
    match context.flags.as_mut() {
        Some(flags) => {
            if sampled {
                *flags = TraceFlags::new(flags.bits() | 1);
            } else {
                *flags = TraceFlags::new(flags.bits() & !1);
            }
        }
        None => {
            context.flags = Some(TraceFlags::new(if sampled { 1 } else { 0 }));
        }
    }
}

#[cfg(feature = "trace")]
fn sync_sampled_with_flags(context: &mut TraceContext) {
    let Some(flags) = context.flags else {
        return;
    };
    context.sampled = Some((flags.bits() & 1) == 1);
}
