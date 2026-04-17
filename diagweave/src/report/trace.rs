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

impl TraceEventLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

impl Display for TraceEventLevel {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
/// A key-value attribute attached to a trace event.
pub struct TraceEventAttribute {
    pub key: StaticRefStr,
    pub value: AttachmentValue,
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
}

impl TraceContext {
    /// Returns true if the trace context is empty (no IDs or flags).
    pub fn is_empty(&self) -> bool {
        self.trace_id.is_none()
            && self.span_id.is_none()
            && self.parent_span_id.is_none()
            && self.sampled.is_none()
            && self.trace_state.is_none()
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
        self
    }

    /// Sets whether the trace is sampled only if not already set.
    pub fn with_sampled(mut self, sampled: bool) -> Self {
        if self.context().and_then(|c| c.sampled).is_none() {
            let inner = self.ensure_inner();
            inner.context.sampled = Some(sampled);
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

    /// Returns a mutable reference to the inner events, allocating if necessary.
    fn events_mut(&mut self) -> &mut Vec<TraceEvent> {
        &mut self.ensure_inner().events
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
/// Fixed-length non-zero hexadecimal identifier.
pub struct HexId<const N: usize>([u8; N]);

impl<const N: usize> fmt::Debug for HexId<N> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "HexId({})", self.as_str())
    }
}

impl<const N: usize> HexId<N> {
    /// Creates a validated hexadecimal identifier.
    pub const fn new(value: [u8; N]) -> Result<Self, &'static str> {
        let mut i = 0;
        let mut all_zeros = true;

        while i < N {
            let b = value[i];
            if !b.is_ascii_hexdigit() {
                return Err("invalid hex id: contains non-hex characters");
            }
            if b != b'0' {
                all_zeros = false;
            }
            i += 1;
        }

        if all_zeros {
            return Err("invalid hex id: cannot be all zeros");
        }

        Ok(Self(value))
    }

    /// Creates an unchecked identifier.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `value` is a valid hexadecimal identifier.
    pub const unsafe fn new_unchecked(value: [u8; N]) -> Self {
        Self(value)
    }

    /// Creates a validated hexadecimal identifier from a byte slice.
    ///
    /// If the slice length is less than N, the identifier is padded with '0' bytes at the beginning.
    /// For example, if N=4 and the input is "ab", the result will be "00ab".
    pub const fn from_bytes(value: &[u8]) -> Result<Self, &'static str> {
        let len = value.len();
        if len > N {
            return Err("invalid hex id: input too long");
        }
        if len == 0 {
            return Err("invalid hex id: input is empty");
        }

        let mut bytes = [b'0'; N];
        let mut i = 0;
        let offset = N - len;
        let mut all_zeros = true;

        while i < len {
            let b = value[i];
            if !b.is_ascii_hexdigit() {
                return Err("invalid hex id: contains non-hex characters");
            }
            if b != b'0' {
                all_zeros = false;
            }
            bytes[offset + i] = b;
            i += 1;
        }

        if all_zeros {
            return Err("invalid hex id: cannot be all zeros");
        }

        Ok(Self(bytes))
    }

    /// Creates a validated hexadecimal identifier from a str.
    pub const fn from_str(value: &str) -> Result<Self, &'static str> {
        let slice = value.as_bytes();
        if slice.len() != N {
            return Err("invalid hex id: length mismatch");
        }

        let mut bytes = [0u8; N];
        let mut i = 0;
        let mut all_zeros = true;

        while i < N {
            let b = slice[i];
            if !b.is_ascii_hexdigit() {
                return Err("invalid hex id: contains non-hex characters");
            }
            if b != b'0' {
                all_zeros = false;
            }
            bytes[i] = b;
            i += 1;
        }

        if all_zeros {
            return Err("invalid hex id: cannot be all zeros");
        }

        Ok(Self(bytes))
    }

    /// Creates an unchecked identifier from a str.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `value` is a valid hexadecimal identifier.
    pub const unsafe fn from_str_unchecked(value: &str) -> Self {
        let slice = value.as_bytes();
        let mut bytes = [0u8; N];
        let mut i = 0;
        while i < N {
            bytes[i] = slice[i];
            i += 1;
        }
        Self(bytes)
    }

    /// Returns the owned inner bytes.
    pub const fn into_inner(self) -> [u8; N] {
        self.0
    }

    /// Returns the identifier as a string slice.
    pub const fn as_str(&self) -> &str {
        // SAFETY: HexId is always validated to be ASCII hex digits during construction.
        unsafe { core::str::from_utf8_unchecked(&self.0) }
    }

    /// Returns the identifier as an owned String.
    pub fn to_string(&self) -> String {
        self.as_str().to_string()
    }
}

impl<const N: usize> TryFrom<[u8; N]> for HexId<N> {
    type Error = &'static str;
    fn try_from(value: [u8; N]) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<'a, const N: usize> TryFrom<&'a str> for HexId<N> {
    type Error = &'static str;
    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Self::from_str(value)
    }
}

#[cfg(feature = "json")]
impl<const N: usize> serde::Serialize for HexId<N> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "json")]
impl<'de, const N: usize> serde::Deserialize<'de> for HexId<N> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct HexIdVisitor<const N: usize>;

        impl<'de, const N: usize> serde::de::Visitor<'de> for HexIdVisitor<N> {
            type Value = HexId<N>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "a hex string of length {}", N)
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                HexId::from_str(v).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(HexIdVisitor::<N>)
    }
}

impl<const N: usize> AsRef<str> for HexId<N> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<const N: usize> core::ops::Deref for HexId<N> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl<const N: usize> Display for HexId<N> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 16-byte trace id encoded as 32 lowercase hex chars.
pub type TraceId = HexId<32>;

#[cfg(feature = "otel")]
impl TryFrom<opentelemetry::TraceId> for TraceId {
    type Error = &'static str;
    fn try_from(value: opentelemetry::TraceId) -> Result<Self, Self::Error> {
        Self::from_bytes(value.to_bytes().as_ref())
    }
}

/// 8-byte span id encoded as 16 lowercase hex chars.
pub type SpanId = HexId<16>;

#[cfg(feature = "otel")]
impl TryFrom<opentelemetry::SpanId> for SpanId {
    type Error = &'static str;
    fn try_from(value: opentelemetry::SpanId) -> Result<Self, Self::Error> {
        Self::from_bytes(value.to_bytes().as_ref())
    }
}
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

    pub fn into_inner(self) -> StaticRefStr {
        self.0
    }
}

impl From<StaticRefStr> for TraceState {
    fn from(value: StaticRefStr) -> Self {
        Self(value)
    }
}

impl AsRef<str> for TraceState {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl core::ops::Deref for TraceState {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl Display for TraceState {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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
        self
    }

    /// Sets whether the trace is sampled only if not already set.
    pub fn with_trace_sampled(mut self, sampled: bool) -> Self {
        if self.trace().context().and_then(|c| c.sampled).is_none() {
            let inner = self.trace_mut().ensure_inner();
            inner.context.sampled = Some(sampled);
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

