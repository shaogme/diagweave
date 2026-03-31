#[cfg(feature = "json")]
use alloc::vec::Vec;
use ref_str::StaticRefStr;

use super::{AttachmentValue, ErrorCode};
#[cfg(feature = "trace")]
use crate::report::{ParentSpanId, SpanId, TraceFlags, TraceId, TraceState};
use crate::utils::FastMap;

pub type ContextValue = AttachmentValue;

#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct ContextMap(FastMap<StaticRefStr, ContextValue>);

impl ContextMap {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn insert(&mut self, key: impl Into<StaticRefStr>, value: ContextValue) {
        self.0.insert(key.into(), value);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&StaticRefStr, &ContextValue)> {
        self.0.iter()
    }

    pub fn sorted_entries(&self) -> Vec<(&StaticRefStr, &ContextValue)> {
        self.0.sorted_entries()
    }
}

impl<'a> IntoIterator for &'a ContextMap {
    type Item = (&'a StaticRefStr, &'a ContextValue);
    type IntoIter = <&'a FastMap<StaticRefStr, ContextValue> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        (&self.0).into_iter()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct SystemContext {
    pub service: ContextMap,
    pub deployment: ContextMap,
    pub runtime: ContextMap,
    pub request: ContextMap,
}

impl SystemContext {
    pub fn is_empty(&self) -> bool {
        self.service.is_empty()
            && self.deployment.is_empty()
            && self.runtime.is_empty()
            && self.request.is_empty()
    }

    pub fn len(&self) -> usize {
        self.service.len() + self.deployment.len() + self.runtime.len() + self.request.len()
    }

    pub fn insert(&mut self, key: impl Into<StaticRefStr>, value: ContextValue) {
        self.runtime.insert(key, value);
    }

    pub fn sections(&self) -> [(&'static str, &ContextMap); 4] {
        [
            ("service", &self.service),
            ("deployment", &self.deployment),
            ("runtime", &self.runtime),
            ("request", &self.request),
        ]
    }
}

#[cfg(feature = "json")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JsonContext {
    pub entries: Vec<JsonContextEntry>,
}

#[cfg(feature = "json")]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JsonContextEntry {
    pub key: StaticRefStr,
    pub value: ContextValue,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct GlobalErrorMeta {
    pub error_code: Option<ErrorCode>,
    pub category: Option<StaticRefStr>,
    pub retryable: Option<bool>,
}

impl GlobalErrorMeta {
    pub fn is_empty(&self) -> bool {
        self.error_code.is_none() && self.category.is_none() && self.retryable.is_none()
    }
}

#[cfg(feature = "trace")]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlobalTraceContext {
    pub trace_id: Option<TraceId>,
    pub span_id: Option<SpanId>,
    pub parent_span_id: Option<ParentSpanId>,
    pub sampled: Option<bool>,
    pub trace_state: Option<TraceState>,
    pub flags: Option<TraceFlags>,
}

#[cfg(feature = "trace")]
impl GlobalTraceContext {
    pub fn is_empty(&self) -> bool {
        self.trace_id.is_none()
            && self.span_id.is_none()
            && self.parent_span_id.is_none()
            && self.sampled.is_none()
            && self.trace_state.is_none()
            && self.flags.is_none()
    }
}
