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

#[derive(Debug, Default, Clone, PartialEq)]
/// Trace payload attached to a report.
pub struct ReportTrace {
    pub context: TraceContext,
    pub events: Vec<TraceEvent>,
}

impl ReportTrace {
    /// Returns true if the report trace is empty (no context and no events).
    pub fn is_empty(&self) -> bool {
        self.context.is_empty() && self.events.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Fixed-length non-zero hexadecimal identifier.
pub struct HexId<const N: usize>(StaticRefStr);

impl<const N: usize> HexId<N> {
    /// Creates a validated hexadecimal identifier.
    pub fn new(value: impl Into<StaticRefStr>) -> Result<Self, ()> {
        let value = value.into();
        if Self::is_valid(value.as_str()) {
            Ok(Self(value))
        } else {
            Err(())
        }
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
    pub fn trace(&self) -> Option<&ReportTrace> {
        self.diagnostics().and_then(|diag| diag.trace.as_ref())
    }

    /// Sets the trace information for the report.
    pub fn with_trace(mut self, trace: ReportTrace) -> Self {
        self.diagnostics_mut().trace = Some(trace);
        self
    }

    /// Sets the trace and span IDs for the report.
    pub fn with_trace_ids(mut self, trace_id: TraceId, span_id: SpanId) -> Self {
        let trace = self.trace_mut();
        trace.context.trace_id = Some(trace_id);
        trace.context.span_id = Some(span_id);
        self
    }

    /// Sets the parent span ID for the report.
    pub fn with_parent_span_id(mut self, parent_span_id: ParentSpanId) -> Self {
        self.trace_mut().context.parent_span_id = Some(parent_span_id);
        self
    }

    /// Sets whether the trace is sampled.
    pub fn with_trace_sampled(mut self, sampled: bool) -> Self {
        let trace = self.trace_mut();
        trace.context.sampled = Some(sampled);
        sync_flags_with_sampled(&mut trace.context);
        self
    }

    /// Sets the trace state.
    pub fn with_trace_state(mut self, trace_state: impl Into<StaticRefStr>) -> Self {
        self.trace_mut().context.trace_state = Some(TraceState::from(trace_state.into()));
        self
    }

    /// Sets the trace flags.
    pub fn with_trace_flags(mut self, flags: impl Into<TraceFlags>) -> Self {
        let trace = self.trace_mut();
        trace.context.flags = Some(flags.into());
        sync_sampled_with_flags(&mut trace.context);
        self
    }

    /// Adds a trace event to the report.
    pub fn with_trace_event(mut self, event: TraceEvent) -> Self {
        self.trace_mut().events.push(event);
        self
    }

    /// Pushes a trace event with the specified name.
    pub fn push_trace_event(mut self, name: impl Into<StaticRefStr>) -> Self {
        self.trace_mut().events.push(TraceEvent {
            name: name.into(),
            ..TraceEvent::default()
        });
        self
    }

    /// Pushes a trace event with detailed information.
    pub fn push_trace_event_ext(
        mut self,
        name: impl Into<StaticRefStr>,
        level: Option<TraceEventLevel>,
        timestamp_unix_nano: Option<u64>,
        attributes: impl IntoIterator<Item = TraceEventAttribute>,
    ) -> Self {
        self.trace_mut().events.push(TraceEvent {
            name: name.into(),
            level,
            timestamp_unix_nano,
            attributes: attributes.into_iter().collect::<Vec<_>>(),
        });
        self
    }

    fn trace_mut(&mut self) -> &mut ReportTrace {
        let diag = self.diagnostics_mut();
        diag.trace.get_or_insert_with(ReportTrace::default)
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
