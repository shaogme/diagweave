#[path = "render/ir.rs"]
mod ir;
#[cfg(feature = "json")]
#[path = "render/json.rs"]
mod json;
#[cfg(all(feature = "json", feature = "otel"))]
#[path = "render/otel_snapshot.rs"]
mod otel_snapshot;
#[path = "render/pretty.rs"]
mod pretty;
#[path = "render/stack_filter.rs"]
mod stack_filter;

use alloc::string::{String, ToString};
use core::fmt::{self, Display, Formatter};

use crate::report::{HasSeverity, Report, SeverityState};

#[cfg(feature = "trace")]
pub(crate) use ir::build_ctx_and_attachments;
#[cfg(any(feature = "trace", feature = "otel"))]
pub(crate) use ir::build_error_value;
#[cfg(feature = "trace")]
pub(crate) use ir::build_trace_value;
pub use ir::{DiagnosticIr, DiagnosticIrError, DiagnosticIrMessage, DiagnosticIrMetadata};
#[cfg(any(feature = "trace", feature = "otel"))]
pub(crate) use ir::{
    build_diag_src_errs_val, build_display_causes, build_origin_src_errs_val,
    build_stack_trace_value,
};
pub use pretty::Pretty;
pub(crate) use stack_filter::filtered_frames;

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
impl<E, State> ReportRenderer<E, State> for Json
where
    E: core::error::Error + Display + 'static,
    State: SeverityState,
{
    fn render(&self, report: &Report<E, State>, f: &mut Formatter<'_>) -> fmt::Result {
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
    pub show_trace_event_details: bool,
    pub show_stack_trace_section: bool,
    pub show_context_section: bool,
    pub show_attachments_section: bool,
    pub show_cause_chains_section: bool,
    pub stack_trace_max_lines: usize,
    pub stack_trace_include_raw: bool,
    pub stack_trace_include_frames: bool,
    pub stack_trace_filter: StackTraceFilter,
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

/// Filter strategy for stack trace frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(rename_all = "snake_case"))]
pub enum StackTraceFilter {
    /// Show all frames without filtering.
    #[default]
    All,
    /// Filter out standard library and runtime frames.
    AppOnly,
    /// Filter out standard library, runtime, and internal frames.
    AppFocused,
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
            show_trace_event_details: true,
            show_stack_trace_section: true,
            show_context_section: true,
            show_attachments_section: true,
            show_cause_chains_section: true,
            stack_trace_max_lines: 24,
            stack_trace_include_raw: true,
            stack_trace_include_frames: true,
            stack_trace_filter: StackTraceFilter::default(),
            json_pretty: false,
        }
    }
}

impl ReportRenderOptions {
    /// Developer mode: show all detailed information including trace events and unfiltered stack traces.
    pub fn developer() -> Self {
        Self {
            show_trace_event_details: true,
            stack_trace_filter: StackTraceFilter::All,
            stack_trace_max_lines: 50,
            ..Self::default()
        }
    }

    /// Production incident mode: filter noise while keeping essential debugging info.
    pub fn production() -> Self {
        Self {
            show_trace_event_details: true,
            stack_trace_filter: StackTraceFilter::AppOnly,
            stack_trace_max_lines: 15,
            ..Self::default()
        }
    }

    /// Minimal mode: show only core information for quick scanning.
    pub fn minimal() -> Self {
        Self {
            show_trace_event_details: false,
            stack_trace_filter: StackTraceFilter::AppFocused,
            stack_trace_max_lines: 5,
            show_empty_sections: false,
            show_type_name: false,
            ..Self::default()
        }
    }
}

/// A trait for rendering a diagnostic report using a specific format.
pub trait ReportRenderer<E, State>
where
    State: SeverityState,
{
    fn render(&self, report: &Report<E, State>, f: &mut Formatter<'_>) -> fmt::Result;
}

/// A renderer that produces a compact display of the report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Compact {
    pub profile: CompactProfile,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "json", serde(rename_all = "snake_case"))]
pub enum CompactProfile {
    #[default]
    Summary,
    Full,
}

/// A report that has been paired with a renderer, implementing `Display`.
pub struct RenderedReport<'a, E, State, R>
where
    State: SeverityState,
{
    report: &'a Report<E, State>,
    renderer: R,
}

impl<E, State> Report<E, State>
where
    State: SeverityState,
{
    /// Returns a renderer for compact output.
    pub fn compact(&self) -> RenderedReport<'_, E, State, Compact> {
        self.render(Compact::summary())
    }

    /// Returns a renderer for summary compact output.
    pub fn compact_summary(&self) -> RenderedReport<'_, E, State, Compact> {
        self.render(Compact::summary())
    }

    /// Returns a renderer for full compact key-value output.
    pub fn compact_full(&self) -> RenderedReport<'_, E, State, Compact> {
        self.render(Compact::full())
    }

    /// Returns a renderer for pretty-printed output.
    pub fn pretty(&self) -> RenderedReport<'_, E, State, Pretty> {
        self.render(Pretty::default())
    }

    /// Returns a renderer for JSON output.
    #[cfg(feature = "json")]
    pub fn json(&self) -> RenderedReport<'_, E, State, Json> {
        self.render(Json::default())
    }

    /// Returns a renderer for the given renderer implementation.
    pub fn render<R>(&self, renderer: R) -> RenderedReport<'_, E, State, R> {
        RenderedReport {
            report: self,
            renderer,
        }
    }

    /// Returns a snapshot-ready compact summary string.
    pub fn snap_compact(&self) -> String
    where
        E: core::error::Error + Display + 'static,
    {
        self.render(Compact::summary()).to_string()
    }

    /// Returns a snapshot-ready pretty-printed string.
    pub fn snap_pretty(&self) -> String
    where
        E: core::error::Error + Display + 'static,
    {
        self.render(Pretty::default()).to_string()
    }

    /// Returns a snapshot-ready JSON string.
    #[cfg(feature = "json")]
    pub fn snap_json(&self) -> String
    where
        E: core::error::Error + Display + 'static,
    {
        self.render(Json::default()).to_string()
    }
}

impl<E> Report<E, HasSeverity> {
    /// Returns a snapshot-ready OTel envelope JSON string.
    /// Requires the report to carry an explicit severity.
    #[cfg(all(feature = "json", feature = "otel"))]
    pub fn snap_otel(&self) -> String
    where
        E: core::error::Error + Display + 'static,
    {
        let ir = self.to_diagnostic_ir();
        let mut otel = ir.to_otel_envelope_default();
        otel_snapshot::normalize_otel_envelope(&mut otel);
        serde_json::to_string(&otel).unwrap_or_default()
    }
}

impl<E, State> ReportRenderer<E, State> for Compact
where
    E: Display,
    State: SeverityState,
{
    fn render(&self, report: &Report<E, State>, f: &mut Formatter<'_>) -> fmt::Result {
        match self.profile {
            CompactProfile::Summary => write!(f, "{}", report.inner()),
            CompactProfile::Full => write!(f, "{report}"),
        }
    }
}

impl Compact {
    pub const fn summary() -> Self {
        Self {
            profile: CompactProfile::Summary,
        }
    }

    pub const fn full() -> Self {
        Self {
            profile: CompactProfile::Full,
        }
    }
}

impl<E, State, R> Display for RenderedReport<'_, E, State, R>
where
    State: SeverityState,
    R: ReportRenderer<E, State>,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.renderer.render(self.report, f)
    }
}
