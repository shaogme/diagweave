#[cfg(feature = "json")]
#[path = "render/json.rs"]
mod json;
#[path = "render/pretty.rs"]
mod pretty;

use alloc::borrow::Cow;
use alloc::format;
use core::any;
use core::error::Error;
use core::fmt::{self, Display, Formatter};

#[cfg(feature = "trace")]
use crate::report::ReportTrace;
use crate::report::{
    AttachmentVisit, CauseCollectOptions, ErrorCode, Report, Severity, StackTrace,
};

pub use pretty::Pretty;

#[cfg(feature = "json")]
pub use json::{Json, REPORT_JSON_SCHEMA_DRAFT, REPORT_JSON_SCHEMA_VERSION, report_json_schema};

/// Options for rendering a diagnostic report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
pub struct ReportRenderOptions {
    pub max_source_depth: usize,
    pub detect_source_cycle: bool,
    pub pretty_indent: PrettyIndent,
    pub show_type_name: bool,
    pub show_empty_sections: bool,
    pub show_governance_section: bool,
    pub show_trace_section: bool,
    pub show_stack_trace_section: bool,
    pub show_context_section: bool,
    pub show_attachments_section: bool,
    pub show_cause_chains_section: bool,
    pub stack_trace_max_lines: usize,
    pub stack_trace_include_raw: bool,
    pub stack_trace_include_frames: bool,
    pub json_pretty: bool,
}

/// Indentation style for pretty rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(rename_all = "snake_case"))]
pub enum PrettyIndent {
    Spaces(u8),
    Tab,
}

impl Default for ReportRenderOptions {
    fn default() -> Self {
        Self {
            max_source_depth: 16,
            detect_source_cycle: true,
            pretty_indent: PrettyIndent::Spaces(2),
            show_type_name: true,
            show_empty_sections: true,
            show_governance_section: true,
            show_trace_section: true,
            show_stack_trace_section: true,
            show_context_section: true,
            show_attachments_section: true,
            show_cause_chains_section: true,
            stack_trace_max_lines: 24,
            stack_trace_include_raw: true,
            stack_trace_include_frames: true,
            json_pretty: false,
        }
    }
}

/// A trait for rendering a diagnostic report using a specific format.
pub trait ReportRenderer<E> {
    fn render(&self, report: &Report<E>, f: &mut Formatter<'_>) -> fmt::Result;
}

/// Error information in the Diagnostic Intermediate Representation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct DiagnosticIrError<'a> {
    pub message: Cow<'a, str>,
    pub r#type: Cow<'a, str>,
}

/// Cause chain summary information in the Diagnostic Intermediate Representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct DiagnosticIrCauseChainSummary {
    pub count: usize,
    pub truncated: bool,
    pub cycle_detected: bool,
}

/// Metadata information in the Diagnostic Intermediate Representation.
pub struct DiagnosticIrMetadata<'a> {
    pub error_code: Option<&'a ErrorCode>,
    pub severity: Option<Severity>,
    pub category: Option<&'a Cow<'static, str>>,
    pub retryable: Option<bool>,
    pub stack_trace: Option<&'a StackTrace>,
    pub display_causes: Option<DiagnosticIrCauseChainSummary>,
    pub source_errors: Option<DiagnosticIrCauseChainSummary>,
}

/// A platform-agnostic intermediate representation of a diagnostic report.
pub struct DiagnosticIr<'a> {
    #[cfg(feature = "json")]
    pub schema_version: Cow<'static, str>,
    pub error: DiagnosticIrError<'a>,
    pub metadata: DiagnosticIrMetadata<'a>,
    #[cfg(feature = "trace")]
    pub trace: Option<&'a ReportTrace>,
    pub context_count: usize,
    pub attachment_count: usize,
}

/// A renderer that produces a compact display of the report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Compact;

impl<E> Report<E> {
    /// Returns a renderer for compact output.
    pub fn compact(&self) -> RenderedReport<'_, E, Compact> {
        self.render(Compact)
    }

    /// Returns a renderer for pretty-printed output.
    pub fn pretty(&self) -> RenderedReport<'_, E, Pretty> {
        self.render(Pretty::default())
    }

    /// Returns a renderer for JSON output.
    #[cfg(feature = "json")]
    pub fn json(&self) -> RenderedReport<'_, E, Json> {
        self.render(Json::default())
    }

    /// Returns a renderer for the given renderer implementation.
    pub fn render<R>(&self, renderer: R) -> RenderedReport<'_, E, R> {
        RenderedReport {
            report: self,
            renderer,
        }
    }
}

impl<E> Report<E>
where
    E: Error + Display + 'static,
{
    /// Converts the report to a platform-agnostic intermediate representation.
    pub fn to_diagnostic_ir(&self, options: ReportRenderOptions) -> DiagnosticIr<'_> {
        let collect_opts = CauseCollectOptions {
            max_depth: options.max_source_depth,
            detect_cycle: options.detect_source_cycle,
        };
        let metadata = self.metadata();
        let (context_count, attachment_count) = count_attachments(self);

        DiagnosticIr {
            #[cfg(feature = "json")]
            schema_version: Cow::Borrowed(REPORT_JSON_SCHEMA_VERSION),
            error: DiagnosticIrError {
                message: format!("{}", self.inner()).into(),
                r#type: Cow::Borrowed(any::type_name::<E>()),
            },
            metadata: DiagnosticIrMetadata {
                error_code: metadata.error_code.as_ref(),
                severity: metadata.severity,
                category: metadata.category.as_ref(),
                retryable: metadata.retryable,
                stack_trace: metadata.stack_trace.as_ref(),
                display_causes: build_display_causes(self, collect_opts),
                source_errors: build_source_errors(self, collect_opts),
            },
            #[cfg(feature = "trace")]
            trace: self.trace(),
            context_count,
            attachment_count,
        }
    }
}

fn count_attachments(report: &Report<impl Error + 'static>) -> (usize, usize) {
    let mut context = 0usize;
    let mut attachments = 0usize;
    match report.visit_attachments(|item| {
        match item {
            AttachmentVisit::Context { .. } => context += 1,
            AttachmentVisit::Note { .. } | AttachmentVisit::Payload { .. } => attachments += 1,
        }
        Ok(())
    }) {
        Ok(()) => (context, attachments),
        Err(_) => (0, 0),
    }
}

fn build_display_causes<E>(
    report: &Report<E>,
    options: CauseCollectOptions,
) -> Option<DiagnosticIrCauseChainSummary>
where
    E: Error + Display + 'static,
{
    let mut count = 0usize;
    let state = match report.visit_causes_ext(options, |_| {
        count += 1;
        Ok(())
    }) {
        Ok(state) => state,
        Err(_) => return None,
    };

    if count == 0 && !state.truncated && !state.cycle_detected {
        return None;
    }

    Some(DiagnosticIrCauseChainSummary {
        count,
        truncated: state.truncated,
        cycle_detected: state.cycle_detected,
    })
}

fn build_source_errors<E>(
    report: &Report<E>,
    options: CauseCollectOptions,
) -> Option<DiagnosticIrCauseChainSummary>
where
    E: Error + Display + 'static,
{
    let mut count = 0usize;
    let state = match report.visit_sources_ext(options, |_| {
        count += 1;
        Ok(())
    }) {
        Ok(state) => state,
        Err(_) => return None,
    };

    if count == 0 && !state.truncated && !state.cycle_detected {
        return None;
    }

    Some(DiagnosticIrCauseChainSummary {
        count,
        truncated: state.truncated,
        cycle_detected: state.cycle_detected,
    })
}

/// A report that has been paired with a renderer, implementing `Display`.
pub struct RenderedReport<'a, E, R> {
    report: &'a Report<E>,
    renderer: R,
}

impl<E> ReportRenderer<E> for Compact
where
    E: Display,
{
    fn render(&self, report: &Report<E>, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{report}")
    }
}

impl<E, R> Display for RenderedReport<'_, E, R>
where
    R: ReportRenderer<E>,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.renderer.render(self.report, f)
    }
}
