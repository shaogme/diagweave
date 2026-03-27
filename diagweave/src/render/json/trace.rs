use alloc::borrow::Cow;
use alloc::string::ToString;
use alloc::vec::Vec;

use crate::report::{AttachmentValue, ReportTrace, TraceContext, TraceEvent, TraceEventAttribute};

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub(super) struct TraceSectionValue {
    pub context: TraceContextValue,
    pub events: Vec<TraceEventValue>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub(super) struct TraceContextValue {
    pub trace_id: Option<Cow<'static, str>>,
    pub span_id: Option<Cow<'static, str>>,
    pub parent_span_id: Option<Cow<'static, str>>,
    pub sampled: Option<bool>,
    pub trace_state: Option<Cow<'static, str>>,
    pub flags: Option<u8>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub(super) struct TraceEventValue {
    pub name: Cow<'static, str>,
    pub level: Option<Cow<'static, str>>,
    pub timestamp_unix_nano: Option<u64>,
    pub attributes: Vec<TraceAttributeValue>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub(super) struct TraceAttributeValue {
    pub key: Cow<'static, str>,
    pub value: AttachmentValue,
}

pub(super) fn build_trace_section_value(trace: &ReportTrace) -> TraceSectionValue {
    TraceSectionValue {
        context: build_trace_wire_context_value(&trace.context),
        events: trace
            .events
            .iter()
            .map(build_trace_wire_event_value)
            .collect(),
    }
}

fn build_trace_wire_context_value(context: &TraceContext) -> TraceContextValue {
    TraceContextValue {
        trace_id: context.trace_id.as_ref().map(|v| v.as_cow()),
        span_id: context.span_id.as_ref().map(|v| v.as_cow()),
        parent_span_id: context.parent_span_id.as_ref().map(|v| v.as_cow()),
        sampled: context.sampled,
        trace_state: context
            .trace_state
            .as_ref()
            .map(|v| Cow::Owned(v.to_string())),
        flags: context.flags,
    }
}

fn build_trace_wire_event_value(event: &TraceEvent) -> TraceEventValue {
    TraceEventValue {
        name: Cow::Owned(event.name.to_string()),
        level: event.level.map(Cow::from),
        timestamp_unix_nano: event.timestamp_unix_nano,
        attributes: event
            .attributes
            .iter()
            .map(build_trace_wire_attribute_value)
            .collect(),
    }
}

fn build_trace_wire_attribute_value(attr: &TraceEventAttribute) -> TraceAttributeValue {
    TraceAttributeValue {
        key: Cow::Owned(attr.key.to_string()),
        value: attr.value.clone(),
    }
}
