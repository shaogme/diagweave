#[path = "render/ir.rs"]
mod ir;
#[cfg(feature = "json")]
#[path = "render/json.rs"]
mod json;
#[path = "render/pretty.rs"]
mod pretty;

use core::fmt::{self, Display, Formatter};

use crate::report::Report;

#[cfg(feature = "trace")]
pub(crate) use ir::build_ctx_and_attachments;
#[cfg(any(feature = "trace", feature = "otel"))]
pub(crate) use ir::build_error_value;
#[cfg(feature = "trace")]
pub(crate) use ir::build_trace_value;
pub use ir::{DiagnosticIr, DiagnosticIrError, DiagnosticIrMessage, DiagnosticIrMetadata};
#[cfg(any(feature = "trace", feature = "otel"))]
pub(crate) use ir::{
    build_diagnostic_source_errors_value, build_display_causes, build_origin_source_errors_value,
    build_stack_trace_value,
};
pub use pretty::Pretty;

#[cfg(feature = "json")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// JSON renderer for diagnostic reports.
pub struct Json {
    pub options: ReportRenderOptions,
}

#[cfg(feature = "json")]
impl Json {
    /// Creates a new JSON renderer with specific options.
    pub fn new(options: ReportRenderOptions) -> Self {
        Self { options }
    }
}

#[cfg(feature = "json")]
impl<E> ReportRenderer<E> for Json
where
    E: core::error::Error + Display + 'static,
{
    fn render(&self, report: &Report<E>, f: &mut Formatter<'_>) -> fmt::Result {
        json::write_json_report(report, self.options, f)
    }
}

#[cfg(feature = "json")]
pub const REPORT_JSON_SCHEMA_VERSION: &str = "v0.1.0";
#[cfg(feature = "json")]
pub const REPORT_JSON_SCHEMA_DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";
#[cfg(feature = "json")]
/// Returns the JSON schema for rendered reports.
pub fn report_json_schema() -> &'static str {
    include_str!("../schemas/report-v0.1.0.schema.json")
}

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

/// A renderer that produces a compact display of the report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Compact;

/// A report that has been paired with a renderer, implementing `Display`.
pub struct RenderedReport<'a, E, R> {
    report: &'a Report<E>,
    renderer: R,
}

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
