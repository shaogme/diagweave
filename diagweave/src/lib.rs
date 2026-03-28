#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

#[path = "adapters.rs"]
mod adapters_impl;
#[path = "render.rs"]
mod render_impl;
#[path = "report.rs"]
mod report_impl;
#[cfg(feature = "trace")]
#[path = "trace.rs"]
mod trace_impl;

pub use diagweave_macros::{Error, set, union};
pub use ref_str::{RefStr, StaticRefStr};

#[cfg(doctest)]
#[doc = include_str!("../../README.md")]
pub struct ReadmeDoctests;

#[cfg(doctest)]
#[doc = include_str!("../../README_CN.md")]
pub struct ReadmeCnDoctests;

#[cfg(doctest)]
#[doc = include_str!("../../docs/ai/ai_docs.md")]
pub struct AiDoctests;

#[cfg(doctest)]
#[doc = include_str!("../../docs/ai/ai_docs_cn.md")]
pub struct AiCnDoctests;

#[cfg(any(feature = "trace", feature = "otel"))]
pub mod adapters {
    pub use crate::adapters_impl::*;
}

pub mod render {
    pub use crate::render_impl::{
        Compact, DiagnosticIr, DiagnosticIrError, DiagnosticIrMessage, DiagnosticIrMetadata,
        Pretty, PrettyIndent, RenderedReport, ReportRenderOptions, ReportRenderer,
    };
    #[cfg(feature = "json")]
    pub use crate::render_impl::{
        Json, REPORT_JSON_SCHEMA_DRAFT, REPORT_JSON_SCHEMA_VERSION, report_json_schema,
    };
}

pub mod report {
    pub use crate::report_impl::{
        Attachment, AttachmentValue, AttachmentVisit, CauseCollectOptions, CauseKind,
        CauseTraversalState, Diagnostic, DisplayCauseChain, ErrorCode, ErrorCodeIntError,
        GlobalContext, Report, ReportMetadata, ReportResultExt, ReportResultInspectExt,
        ReportSourceErrorIter, Severity, SourceErrorChain, SourceErrorEntry, SourceErrorItem,
        StackFrame, StackTrace, StackTraceFormat,
    };
    #[cfg(feature = "trace")]
    pub use crate::report_impl::{
        ParentSpanId, ReportTrace, SpanId, TraceContext, TraceEvent, TraceEventAttribute,
        TraceEventLevel, TraceId,
    };
    #[cfg(feature = "std")]
    pub use crate::report_impl::{RegisterGlobalContextError, register_global_injector};
}

#[cfg(feature = "trace")]
pub mod trace {
    #[cfg(feature = "tracing")]
    pub use crate::trace_impl::TracingExporter;
    pub use crate::trace_impl::TracingExporterTrait;
}

pub mod prelude {
    pub use crate::render::{Compact, Pretty, ReportRenderOptions, ReportRenderer};
    pub use crate::report::{
        AttachmentValue, Diagnostic, Report, ReportResultExt, ReportResultInspectExt, Severity,
        SourceErrorItem,
    };
    #[cfg(feature = "std")]
    pub use crate::report::{GlobalContext, register_global_injector};
    #[cfg(feature = "trace")]
    pub use crate::report::{
        ParentSpanId, SpanId, TraceEvent, TraceEventAttribute, TraceEventLevel, TraceId,
    };
    pub use crate::{Error, RefStr, StaticRefStr, set, union};
}
