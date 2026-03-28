use alloc::vec::Vec;
use ref_str::{RefStr, StaticRefStr};

use crate::report::{AttachmentValue, ReportTrace, TraceContext, TraceEvent, TraceEventAttribute};

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(bound(deserialize = "'de: 'a")))]
pub(super) struct TraceSectionValue<'a> {
    pub context: TraceContextValue<'a>,
    pub events: Vec<TraceEventValue<'a>>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(bound(deserialize = "'de: 'a")))]
pub(super) struct TraceContextValue<'a> {
    pub trace_id: Option<RefStr<'a>>,
    pub span_id: Option<RefStr<'a>>,
    pub parent_span_id: Option<RefStr<'a>>,
    pub sampled: Option<bool>,
    pub trace_state: Option<RefStr<'a>>,
    pub flags: Option<u8>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(bound(deserialize = "'de: 'a")))]
pub(super) struct TraceEventValue<'a> {
    pub name: RefStr<'a>,
    pub level: Option<StaticRefStr>,
    pub timestamp_unix_nano: Option<u64>,
    pub attributes: Vec<TraceAttributeValue<'a>>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(bound(deserialize = "'de: 'a")))]
pub(super) struct TraceAttributeValue<'a> {
    pub key: RefStr<'a>,
    pub value: AttachmentValue,
}

pub(super) fn build_trace_section_value(trace: &ReportTrace) -> TraceSectionValue<'_> {
    TraceSectionValue {
        context: build_trace_wire_context_value(&trace.context),
        events: trace
            .events
            .iter()
            .map(build_trace_wire_event_value)
            .collect(),
    }
}

fn build_trace_wire_context_value(context: &TraceContext) -> TraceContextValue<'_> {
    TraceContextValue {
        trace_id: context.trace_id.as_ref().map(|v| v.as_ref().into()),
        span_id: context.span_id.as_ref().map(|v| v.as_ref().into()),
        parent_span_id: context.parent_span_id.as_ref().map(|v| v.as_ref().into()),
        sampled: context.sampled,
        trace_state: context.trace_state.as_ref().map(|v| v.as_str().into()),
        flags: context.flags,
    }
}

fn build_trace_wire_event_value(event: &TraceEvent) -> TraceEventValue<'_> {
    TraceEventValue {
        name: event.name.clone().into(),
        level: event.level.map(trace_event_level_ref),
        timestamp_unix_nano: event.timestamp_unix_nano,
        attributes: event
            .attributes
            .iter()
            .map(build_trace_wire_attribute_value)
            .collect(),
    }
}

fn build_trace_wire_attribute_value(attr: &TraceEventAttribute) -> TraceAttributeValue<'_> {
    TraceAttributeValue {
        key: attr.key.clone().into(),
        value: attr.value.clone(),
    }
}

fn trace_event_level_ref(level: crate::report::TraceEventLevel) -> StaticRefStr {
    match level {
        crate::report::TraceEventLevel::Trace => "trace".into(),
        crate::report::TraceEventLevel::Debug => "debug".into(),
        crate::report::TraceEventLevel::Info => "info".into(),
        crate::report::TraceEventLevel::Warn => "warn".into(),
        crate::report::TraceEventLevel::Error => "error".into(),
    }
}
