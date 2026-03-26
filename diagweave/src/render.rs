#[cfg(feature = "json")]
#[path = "render/json.rs"]
mod json;
#[path = "render/pretty.rs"]
mod pretty;

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::any;
use core::error::Error;
use core::fmt::{self, Display, Formatter};

#[cfg(feature = "trace")]
use crate::report::ReportTrace;
use crate::report::{
    Attachment, AttachmentValue, CauseChain, CauseCollectOptions, CauseCollection, CauseEntry,
    CauseKind, CauseStore, DefaultCauseStore, Report, ReportMetadata, Severity, StackTrace,
};

pub use pretty::Pretty;

#[cfg(feature = "json")]
pub use json::{
    Json, REPORT_JSON_SCHEMA_DRAFT, REPORT_JSON_SCHEMA_VERSION, ReportJsonAttachment,
    ReportJsonCauseChain, ReportJsonCauseKind, ReportJsonCauseNode, ReportJsonContext,
    ReportJsonDocument, ReportJsonError, ReportJsonMetadata, ReportJsonStackFrame,
    ReportJsonStackTrace, ReportJsonStackTraceFormat, report_json_schema,
};

/// Options for rendering a diagnostic report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportRenderOptions {
    /// Maximum depth for traversing cause chains.
    pub max_source_depth: usize,
    /// Whether to detect and stop on cycles in cause chains.
    pub detect_source_cycle: bool,
    /// Indentation style for pretty rendering.
    pub pretty_indent: PrettyIndent,
    /// Whether to show the type name of the error.
    pub show_type_name: bool,
    /// Whether to show sections that are empty.
    pub show_empty_sections: bool,
    /// Whether to show the governance section.
    pub show_governance_section: bool,
    /// Whether to show the trace section.
    pub show_trace_section: bool,
    /// Whether to show the stack trace section.
    pub show_stack_trace_section: bool,
    /// Whether to show the context section.
    pub show_context_section: bool,
    /// Whether to show the attachments section.
    pub show_attachments_section: bool,
    /// Whether to show the causes section.
    pub show_causes_section: bool,
    /// Maximum number of lines to show for a raw stack trace.
    pub stack_trace_max_lines: usize,
    /// Whether to include the raw stack trace if frames are missing.
    pub stack_trace_include_raw: bool,
    /// Whether to include individual stack frames.
    pub stack_trace_include_frames: bool,
    /// Whether to use pretty-printing for JSON output.
    pub json_pretty: bool,
}

/// Indentation style for pretty rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrettyIndent {
    /// Use a fixed number of spaces.
    Spaces(u8),
    /// Use a tab character.
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
            show_causes_section: true,
            stack_trace_max_lines: 24,
            stack_trace_include_raw: true,
            stack_trace_include_frames: true,
            json_pretty: false,
        }
    }
}

/// A trait for rendering a diagnostic report using a specific format.
pub trait ReportRenderer<E, C = DefaultCauseStore>
where
    C: CauseStore,
{
    /// Renders the report to the given formatter.
    fn render(&self, report: &Report<E, C>, f: &mut Formatter<'_>) -> fmt::Result;
}

/// Error information for building diagnostic IR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticIrError {
    /// The formatted error message.
    pub message: String,
    /// The type name of the error.
    pub r#type: String,
}

/// Metadata for building diagnostic IR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticIrMetadata {
    /// An optional error code.
    pub error_code: Option<String>,
    /// The severity of the error.
    pub severity: Option<Severity>,
    /// The category of the error.
    pub category: Option<String>,
    /// Whether the error is retryable.
    pub retryable: Option<bool>,
    /// The stack trace if available.
    pub stack_trace: Option<StackTrace>,
    /// The display cause chain if available.
    pub display_causes: Option<CauseChain>,
    /// The error source chain if available.
    pub error_sources: Option<CauseChain>,
}

/// A context item in the diagnostic IR.
#[derive(Debug, Clone, PartialEq)]
pub struct DiagnosticIrContext {
    /// The key of the context item.
    pub key: String,
    /// The value of the context item.
    pub value: AttachmentValue,
}

/// An attachment in the diagnostic IR.
#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosticIrAttachment {
    /// A simple text note.
    Note {
        /// The message of the note.
        message: String,
    },
    /// A named payload with an optional media type.
    Payload {
        /// The name of the payload.
        name: String,
        /// The value of the payload.
        value: AttachmentValue,
        /// The optional media type of the payload.
        media_type: Option<String>,
    },
}

/// An intermediate representation of a diagnostic report, suitable for rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct DiagnosticIr {
    /// Basic error information.
    pub error: DiagnosticIrError,
    /// Metadata about the error.
    pub metadata: DiagnosticIrMetadata,
    /// Trace information if enabled.
    #[cfg(feature = "trace")]
    pub trace: ReportTrace,
    /// Context key-value pairs.
    pub context: Vec<DiagnosticIrContext>,
    /// Attachments associated with the report.
    pub attachments: Vec<DiagnosticIrAttachment>,
}

/// A compact renderer that uses the default `Display` implementation of the error.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Compact;

impl<E, C> Report<E, C>
where
    C: CauseStore,
{
    /// Wraps the report for compact rendering.
    pub fn compact(&self) -> RenderedReport<'_, E, C, Compact> {
        self.render(Compact)
    }

    /// Wraps the report for pretty rendering.
    pub fn pretty(&self) -> RenderedReport<'_, E, C, Pretty> {
        self.render(Pretty::default())
    }

    /// Wraps the report for JSON rendering.
    #[cfg(feature = "json")]
    pub fn json(&self) -> RenderedReport<'_, E, C, Json> {
        self.render(Json::default())
    }

    /// Wraps the report with a specific renderer.
    pub fn render<R>(&self, renderer: R) -> RenderedReport<'_, E, C, R> {
        RenderedReport {
            report: self,
            renderer,
        }
    }
}

impl From<&ReportMetadata> for DiagnosticIrMetadata {
    fn from(value: &ReportMetadata) -> Self {
        Self {
            error_code: value.error_code.clone(),
            severity: value.severity,
            category: value.category.clone(),
            retryable: value.retryable,
            stack_trace: value.stack_trace.clone(),
            display_causes: value.display_causes.clone(),
            error_sources: value.error_sources.clone(),
        }
    }
}

impl<E, C> Report<E, C>
where
    E: Error + Display + 'static,
    C: CauseStore,
{
    /// Converts the report to a diagnostic intermediate representation.
    pub fn to_diagnostic_ir(&self, options: ReportRenderOptions) -> DiagnosticIr {
        let display_cause_state = self.collect_display_causes(CauseCollectOptions {
            max_depth: options.max_source_depth,
            detect_cycle: options.detect_source_cycle,
        });
        let display_causes = cause_collection_to_chain(&display_cause_state);
        let error_source_state = self.collect_error_sources(CauseCollectOptions {
            max_depth: options.max_source_depth,
            detect_cycle: options.detect_source_cycle,
        });
        let error_sources = cause_collection_to_chain_error_only(&error_source_state);

        let mut context = Vec::new();
        let mut attachments = Vec::new();
        for item in self.attachments() {
            match item {
                Attachment::Context { key, value } => context.push(DiagnosticIrContext {
                    key: key.clone(),
                    value: value.clone(),
                }),
                Attachment::Note { message } => attachments.push(DiagnosticIrAttachment::Note {
                    message: message.clone(),
                }),
                Attachment::Payload {
                    name,
                    value,
                    media_type,
                } => attachments.push(DiagnosticIrAttachment::Payload {
                    name: name.clone(),
                    value: value.clone(),
                    media_type: media_type.clone(),
                }),
            }
        }

        let mut metadata = DiagnosticIrMetadata::from(self.metadata());
        metadata.display_causes = display_causes;
        metadata.error_sources = error_sources;

        DiagnosticIr {
            error: DiagnosticIrError {
                message: format!("{}", self.inner()),
                r#type: any::type_name::<E>().to_owned(),
            },
            metadata,
            #[cfg(feature = "trace")]
            trace: self.trace().cloned().unwrap_or_default(),
            context,
            attachments,
        }
    }
}

fn cause_collection_to_chain(cause_state: &CauseCollection) -> Option<CauseChain> {
    if cause_state.messages.is_empty() && !cause_state.truncated && !cause_state.cycle_detected {
        return None;
    }

    let items = cause_state
        .messages
        .iter()
        .map(|message| {
            if let Some(event_message) = message.strip_prefix("event: ") {
                CauseEntry {
                    kind: CauseKind::Event,
                    message: event_message.to_owned(),
                }
            } else {
                CauseEntry {
                    kind: CauseKind::Error,
                    message: message.clone(),
                }
            }
        })
        .collect();

    Some(CauseChain {
        items,
        truncated: cause_state.truncated,
        cycle_detected: cause_state.cycle_detected,
    })
}

fn cause_collection_to_chain_error_only(cause_state: &CauseCollection) -> Option<CauseChain> {
    if cause_state.messages.is_empty() && !cause_state.truncated && !cause_state.cycle_detected {
        return None;
    }

    let items = cause_state
        .messages
        .iter()
        .map(|message| CauseEntry {
            kind: CauseKind::Error,
            message: message.clone(),
        })
        .collect();

    Some(CauseChain {
        items,
        truncated: cause_state.truncated,
        cycle_detected: cause_state.cycle_detected,
    })
}

/// A report that has been paired with a renderer, implementing `Display`.
pub struct RenderedReport<'a, E, C, R>
where
    C: CauseStore,
{
    report: &'a Report<E, C>,
    renderer: R,
}

impl<E, C> ReportRenderer<E, C> for Compact
where
    E: Display,
    C: CauseStore,
{
    fn render(&self, report: &Report<E, C>, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{report}")
    }
}

impl<E, C, R> Display for RenderedReport<'_, E, C, R>
where
    C: CauseStore,
    R: ReportRenderer<E, C>,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.renderer.render(self.report, f)
    }
}
