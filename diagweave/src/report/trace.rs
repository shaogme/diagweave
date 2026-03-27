use alloc::borrow::Cow;
use alloc::vec::Vec;
use core::fmt::{self, Display, Formatter};

use super::{Report, types::AttachmentValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(rename_all = "snake_case"))]
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

impl From<TraceEventLevel> for Cow<'static, str> {
    fn from(value: TraceEventLevel) -> Self {
        match value {
            TraceEventLevel::Trace => "trace".into(),
            TraceEventLevel::Debug => "debug".into(),
            TraceEventLevel::Info => "info".into(),
            TraceEventLevel::Warn => "warn".into(),
            TraceEventLevel::Error => "error".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct TraceEventAttribute {
    pub key: Cow<'static, str>,
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

#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct TraceEvent {
    pub name: Cow<'static, str>,
    pub level: Option<TraceEventLevel>,
    pub timestamp_unix_nano: Option<u64>,
    pub attributes: Vec<TraceEventAttribute>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct TraceContext {
    pub trace_id: Option<TraceId>,
    pub span_id: Option<SpanId>,
    pub parent_span_id: Option<ParentSpanId>,
    pub sampled: Option<bool>,
    pub trace_state: Option<Cow<'static, str>>,
    pub flags: Option<u8>,
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
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
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
pub struct HexId<const N: usize>(Cow<'static, str>);

impl<const N: usize> HexId<N> {
    pub fn new(value: impl Into<Cow<'static, str>>) -> Result<Self, ()> {
        let value = value.into();
        if Self::is_valid(value.as_ref()) {
            Ok(Self(value))
        } else {
            Err(())
        }
    }

    pub unsafe fn new_unchecked(value: impl Into<Cow<'static, str>>) -> Self {
        Self(value.into())
    }

    pub fn is_valid(value: &str) -> bool {
        if value.len() != N {
            return false;
        }
        if value.bytes().all(|b| b == b'0') {
            return false;
        }
        value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F'))
    }

    pub fn as_cow(&self) -> Cow<'static, str> {
        self.0.clone()
    }
}

impl<const N: usize> AsRef<str> for HexId<N> {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl<const N: usize> Display for HexId<N> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.as_ref())
    }
}

#[cfg(all(feature = "trace", feature = "json"))]
impl<const N: usize> serde::Serialize for HexId<N> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.0.as_ref())
    }
}

#[cfg(all(feature = "trace", feature = "json"))]
impl<'de, const N: usize> serde::Deserialize<'de> for HexId<N> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <Cow<'de, str> as serde::Deserialize<'de>>::deserialize(deserializer)?;
        let value: Cow<'static, str> = value.into_owned().into();
        if Self::is_valid(value.as_ref()) {
            Ok(Self(value))
        } else {
            Err(serde::de::Error::custom("invalid hex id"))
        }
    }
}

pub type TraceId = HexId<32>;
pub type SpanId = HexId<16>;
pub type ParentSpanId = HexId<16>;

impl<E> Report<E> {
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
        sync_trace_flags_with_sampled(&mut trace.context);
        self
    }

    /// Sets the trace state.
    pub fn with_trace_state(mut self, trace_state: impl Into<Cow<'static, str>>) -> Self {
        self.trace_mut().context.trace_state = Some(trace_state.into());
        self
    }

    /// Sets the trace flags.
    pub fn with_trace_flags(mut self, flags: u8) -> Self {
        let trace = self.trace_mut();
        trace.context.flags = Some(flags);
        sync_trace_sampled_with_flags(&mut trace.context);
        self
    }

    /// Adds a trace event to the report.
    pub fn with_trace_event(mut self, event: TraceEvent) -> Self {
        self.trace_mut().events.push(event);
        self
    }

    /// Pushes a trace event with the specified name.
    pub fn push_trace_event(mut self, name: impl Into<Cow<'static, str>>) -> Self {
        self.trace_mut().events.push(TraceEvent {
            name: name.into(),
            ..TraceEvent::default()
        });
        self
    }

    /// Pushes a trace event with detailed information.
    pub fn push_trace_event_ext(
        mut self,
        name: impl Into<Cow<'static, str>>,
        level: Option<TraceEventLevel>,
        timestamp_unix_nano: Option<u64>,
        attributes: impl IntoIterator<Item = TraceEventAttribute>,
    ) -> Self {
        self.trace_mut().events.push(TraceEvent {
            name: name.into(),
            level,
            timestamp_unix_nano,
            attributes: attributes.into_iter().collect(),
        });
        self
    }

    fn trace_mut(&mut self) -> &mut ReportTrace {
        let diag = self.diagnostics_mut();
        if diag.trace.is_none() {
            diag.trace = Some(ReportTrace::default());
        }
        diag.trace.as_mut().expect("trace just initialized")
    }
}

#[cfg(feature = "trace")]
fn sync_trace_flags_with_sampled(context: &mut TraceContext) {
    let Some(sampled) = context.sampled else {
        return;
    };
    match context.flags.as_mut() {
        Some(flags) => {
            if sampled {
                *flags |= 1;
            } else {
                *flags &= !1;
            }
        }
        None => {
            context.flags = Some(if sampled { 1 } else { 0 });
        }
    }
}

#[cfg(feature = "trace")]
fn sync_trace_sampled_with_flags(context: &mut TraceContext) {
    let Some(flags) = context.flags else {
        return;
    };
    context.sampled = Some((flags & 1) == 1);
}
