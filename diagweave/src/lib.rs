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

#[cfg(doctest)]
#[doc = include_str!("../../README.md")]
pub struct ReadmeDoctests;

#[cfg(doctest)]
#[doc = include_str!("../../README_CN.md")]
pub struct ReadmeCnDoctests;

pub mod adapters {
    pub use crate::adapters_impl::*;
}

pub mod render {
    pub use crate::render_impl::{
        Compact, DiagnosticIr, DiagnosticIrAttachment, DiagnosticIrContext, DiagnosticIrError,
        DiagnosticIrMetadata, Pretty, PrettyIndent, RenderedReport, ReportRenderOptions,
        ReportRenderer,
    };
    #[cfg(feature = "json")]
    pub use crate::render_impl::{
        Json, REPORT_JSON_SCHEMA_DRAFT, REPORT_JSON_SCHEMA_VERSION, ReportJsonAttachment,
        ReportJsonContext, ReportJsonDisplayCauseChain, ReportJsonDocument, ReportJsonError,
        ReportJsonMetadata, ReportJsonSourceError, ReportJsonSourceErrorChain,
        ReportJsonStackFrame, ReportJsonStackTrace, ReportJsonStackTraceFormat, report_json_schema,
    };
}

pub mod report {
    pub use crate::report_impl::{
        Attachment, AttachmentValue, CauseCollectOptions, CauseCollection, CauseKind, Diagnostic,
        DisplayCauseChain, GlobalContext, Report, ReportMetadata, ReportResultExt, Severity,
        SourceError, SourceErrorChain, StackFrame, StackTrace, StackTraceFormat,
    };
    #[cfg(feature = "std")]
    pub use crate::report_impl::{RegisterGlobalContextError, register_global_injector};
    #[cfg(feature = "trace")]
    pub use crate::report_impl::{
        ReportTrace, TraceContext, TraceEvent, TraceEventAttribute, TraceEventLevel,
    };
}

#[cfg(feature = "trace")]
pub mod trace {
    #[cfg(feature = "tracing")]
    pub use crate::trace_impl::TracingExporter;
    pub use crate::trace_impl::TracingExporterTrait;
}

pub mod prelude {
    pub use crate::render::{Compact, Pretty, ReportRenderOptions, ReportRenderer};
    pub use crate::report::{AttachmentValue, Diagnostic, Report, ReportResultExt, Severity};
    #[cfg(feature = "std")]
    pub use crate::report::{GlobalContext, register_global_injector};
    #[cfg(feature = "trace")]
    pub use crate::report::{TraceEvent, TraceEventAttribute, TraceEventLevel};
    pub use crate::{Error, set, union};
}
