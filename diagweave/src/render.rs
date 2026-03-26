#[cfg(feature = "json")]
#[path = "render/json.rs"]
mod json;
#[path = "render/pretty.rs"]
mod pretty;

use alloc::borrow::Cow;
use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::any;
use core::error::Error;
use core::fmt::{self, Display, Formatter};

#[cfg(feature = "trace")]
use crate::report::ReportTrace;
use crate::report::{
    Attachment, AttachmentValue, CauseCollectOptions, CauseCollection, DisplayCauseChain,
    ErrorCode, Report, Severity, SourceError, SourceErrorChain, StackTrace,
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

/// Metadata information in the Diagnostic Intermediate Representation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct DiagnosticIrMetadata<'a> {
    pub error_code: Option<&'a ErrorCode>,
    pub severity: Option<Severity>,
    pub category: Option<&'a Cow<'static, str>>,
    pub retryable: Option<bool>,
    pub stack_trace: Option<&'a StackTrace>,
    pub display_causes: Option<DisplayCauseChain>,
    pub source_errors: Option<SourceErrorChain>,
}

/// Context item in the Diagnostic Intermediate Representation.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct DiagnosticIrContext<'a> {
    pub key: Cow<'a, str>,
    pub value: &'a AttachmentValue,
}

/// Attachment in the Diagnostic Intermediate Representation.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
#[cfg_attr(feature = "json", serde(tag = "kind", rename_all = "snake_case"))]
pub enum DiagnosticIrAttachment<'a> {
    Note {
        message: &'a Cow<'static, str>,
    },
    Payload {
        name: &'a Cow<'static, str>,
        value: &'a AttachmentValue,
        media_type: Option<&'a Cow<'static, str>>,
    },
}

/// A platform-agnostic intermediate representation of a diagnostic report.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct DiagnosticIr<'a> {
    #[cfg(feature = "json")]
    pub schema_version: Cow<'static, str>,
    pub error: DiagnosticIrError<'a>,
    pub metadata: DiagnosticIrMetadata<'a>,
    #[cfg(feature = "trace")]
    pub trace: Option<&'a ReportTrace>,
    pub context: Vec<DiagnosticIrContext<'a>>,
    pub attachments: Vec<DiagnosticIrAttachment<'a>>,
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
        let (context, attachments) = split_attachments(self.attachments());
        let metadata = self.metadata();

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
                display_causes: to_display_causes(&self.display_causes_with(collect_opts)),
                source_errors: to_source_errors(&self.source_errors_with(collect_opts)),
            },
            #[cfg(feature = "trace")]
            trace: self.trace(),
            context,
            attachments,
        }
    }
}

fn split_attachments(
    report_attachments: &[Attachment],
) -> (
    Vec<DiagnosticIrContext<'_>>,
    Vec<DiagnosticIrAttachment<'_>>,
) {
    let mut context = Vec::new();
    let mut attachments = Vec::new();
    for item in report_attachments {
        match item {
            Attachment::Context { key, value } => context.push(DiagnosticIrContext {
                key: Cow::Borrowed(key),
                value,
            }),
            Attachment::Note { message } => {
                attachments.push(DiagnosticIrAttachment::Note { message });
            }
            Attachment::Payload {
                name,
                value,
                media_type,
            } => attachments.push(DiagnosticIrAttachment::Payload {
                name,
                value,
                media_type: media_type.as_ref(),
            }),
        }
    }
    (context, attachments)
}

fn to_display_causes(collection: &CauseCollection) -> Option<DisplayCauseChain> {
    if collection.messages.is_empty() && !collection.truncated && !collection.cycle_detected {
        return None;
    }

    let items = collection
        .messages
        .iter()
        .map(|message| {
            message
                .strip_prefix("event: ")
                .unwrap_or(message)
                .to_string()
        })
        .collect();

    Some(DisplayCauseChain {
        items,
        truncated: collection.truncated,
        cycle_detected: collection.cycle_detected,
    })
}

fn to_source_errors(collection: &CauseCollection) -> Option<SourceErrorChain> {
    if collection.messages.is_empty() && !collection.truncated && !collection.cycle_detected {
        return None;
    }

    let items = collection
        .messages
        .iter()
        .map(|message| SourceError {
            message: message.clone(),
        })
        .collect();

    Some(SourceErrorChain {
        items,
        truncated: collection.truncated,
        cycle_detected: collection.cycle_detected,
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
