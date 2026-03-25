use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;
use core::error::Error;
use core::fmt::{self, Display, Formatter};

#[cfg(feature = "trace")]
use crate::report::ReportTrace;
use crate::report::{
    AttachmentValue, CauseChain, CauseEntry, CauseKind, CauseStore, Report, Severity, StackFrame,
    StackTrace, StackTraceFormat,
};

use super::{DiagnosticIr, DiagnosticIrAttachment, ReportRenderOptions, ReportRenderer};

/// A renderer that outputs the diagnostic report in JSON format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Json {
    pub options: ReportRenderOptions,
}

pub const REPORT_JSON_SCHEMA_VERSION: &str = "v0.1.0";

pub const REPORT_JSON_SCHEMA_DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";

/// Returns the JSON schema for the diagnostic report.
pub fn report_json_schema() -> &'static str {
    include_str!("../../schemas/report-v0.1.0.schema.json")
}

/// JSON representation of an error.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ReportJsonError {
    /// The formatted error message.
    pub message: String,
    /// The type name of the error.
    pub r#type: String,
}

/// JSON representation of error metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ReportJsonMetadata {
    /// An optional error code.
    pub error_code: Option<String>,
    /// The severity of the error.
    pub severity: Option<Severity>,
    /// The category of the error.
    pub category: Option<String>,
    /// Whether the error is retryable.
    pub retryable: Option<bool>,
    /// The stack trace if available.
    pub stack_trace: Option<ReportJsonStackTrace>,
    /// The cause chain if available.
    pub causes: Option<ReportJsonCauseChain>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportJsonCauseKind {
    #[default]
    Error,
    Event,
}

/// JSON representation of a cause node.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ReportJsonCauseNode {
    /// The kind of the cause.
    pub kind: ReportJsonCauseKind,
    /// The formatted cause message.
    pub message: String,
}

/// JSON representation of a cause chain.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ReportJsonCauseChain {
    /// The items in the cause chain.
    pub items: Vec<ReportJsonCauseNode>,
    /// Whether the cause chain was truncated.
    pub truncated: bool,
    /// Whether a cycle was detected in the cause chain.
    pub cycle_detected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportJsonStackTraceFormat {
    #[default]
    Native,
    Raw,
}

/// JSON representation of a stack frame.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ReportJsonStackFrame {
    /// The symbol name.
    pub symbol: Option<String>,
    /// The module path.
    pub module_path: Option<String>,
    /// The file name.
    pub file: Option<String>,
    /// The line number.
    pub line: Option<u32>,
    /// The column number.
    pub column: Option<u32>,
}

/// JSON representation of a stack trace.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ReportJsonStackTrace {
    /// The format of the stack trace.
    pub format: ReportJsonStackTraceFormat,
    /// The stack frames.
    pub frames: Vec<ReportJsonStackFrame>,
    /// The raw stack trace string if available.
    pub raw: Option<String>,
}

/// JSON representation of a context item.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct ReportJsonContext {
    /// The key of the context item.
    pub key: String,
    /// The value of the context item.
    pub value: AttachmentValue,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReportJsonAttachment {
    Note {
        message: String,
    },
    Payload {
        name: String,
        value: AttachmentValue,
        media_type: Option<String>,
    },
}

/// JSON representation of a diagnostic report document.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReportJsonDocument {
    /// The version of the schema used.
    pub schema_version: String,
    /// Basic error information.
    pub error: ReportJsonError,
    /// Metadata about the error.
    pub metadata: ReportJsonMetadata,
    /// Trace information if enabled.
    #[cfg(feature = "trace")]
    pub trace: ReportTrace,
    /// Context key-value pairs.
    pub context: Vec<ReportJsonContext>,
    /// Attachments associated with the report.
    pub attachments: Vec<ReportJsonAttachment>,
}

impl Default for ReportJsonDocument {
    fn default() -> Self {
        Self {
            schema_version: REPORT_JSON_SCHEMA_VERSION.to_owned(),
            error: ReportJsonError::default(),
            metadata: ReportJsonMetadata::default(),
            #[cfg(feature = "trace")]
            trace: ReportTrace::default(),
            context: Vec::new(),
            attachments: Vec::new(),
        }
    }
}

impl Json {
    /// Creates a new JSON renderer with specific options.
    pub fn new(options: ReportRenderOptions) -> Self {
        Self { options }
    }
}

impl From<StackTraceFormat> for ReportJsonStackTraceFormat {
    fn from(value: StackTraceFormat) -> Self {
        match value {
            StackTraceFormat::Native => Self::Native,
            StackTraceFormat::Raw => Self::Raw,
        }
    }
}

impl From<StackFrame> for ReportJsonStackFrame {
    fn from(value: StackFrame) -> Self {
        Self {
            symbol: value.symbol,
            module_path: value.module_path,
            file: value.file,
            line: value.line,
            column: value.column,
        }
    }
}

impl From<StackTrace> for ReportJsonStackTrace {
    fn from(value: StackTrace) -> Self {
        Self {
            format: value.format.into(),
            frames: value
                .frames
                .into_iter()
                .map(ReportJsonStackFrame::from)
                .collect(),
            raw: value.raw,
        }
    }
}

impl From<CauseKind> for ReportJsonCauseKind {
    fn from(value: CauseKind) -> Self {
        match value {
            CauseKind::Error => Self::Error,
            CauseKind::Event => Self::Event,
        }
    }
}

impl From<CauseEntry> for ReportJsonCauseNode {
    fn from(value: CauseEntry) -> Self {
        Self {
            kind: value.kind.into(),
            message: value.message,
        }
    }
}

impl From<CauseChain> for ReportJsonCauseChain {
    fn from(value: CauseChain) -> Self {
        Self {
            items: value
                .items
                .into_iter()
                .map(ReportJsonCauseNode::from)
                .collect(),
            truncated: value.truncated,
            cycle_detected: value.cycle_detected,
        }
    }
}

impl From<DiagnosticIr> for ReportJsonDocument {
    fn from(value: DiagnosticIr) -> Self {
        Self {
            schema_version: REPORT_JSON_SCHEMA_VERSION.to_owned(),
            error: ReportJsonError {
                message: value.error.message,
                r#type: value.error.r#type,
            },
            metadata: ReportJsonMetadata {
                error_code: value.metadata.error_code,
                severity: value.metadata.severity,
                category: value.metadata.category,
                retryable: value.metadata.retryable,
                stack_trace: value.metadata.stack_trace.map(ReportJsonStackTrace::from),
                causes: value.metadata.causes.map(ReportJsonCauseChain::from),
            },
            #[cfg(feature = "trace")]
            trace: value.trace,
            context: value
                .context
                .into_iter()
                .map(|item| ReportJsonContext {
                    key: item.key,
                    value: item.value,
                })
                .collect(),
            attachments: value
                .attachments
                .into_iter()
                .map(|item| match item {
                    DiagnosticIrAttachment::Note { message } => {
                        ReportJsonAttachment::Note { message }
                    }
                    DiagnosticIrAttachment::Payload {
                        name,
                        value,
                        media_type,
                    } => ReportJsonAttachment::Payload {
                        name,
                        value,
                        media_type,
                    },
                })
                .collect(),
        }
    }
}

impl<E, C> ReportRenderer<E, C> for Json
where
    E: Error + Display + 'static,
    C: CauseStore,
{
    fn render(&self, report: &Report<E, C>, f: &mut Formatter<'_>) -> fmt::Result {
        render_json(report, self.options, f)
    }
}

fn render_json<E, C>(
    report: &Report<E, C>,
    options: ReportRenderOptions,
    f: &mut Formatter<'_>,
) -> fmt::Result
where
    E: Error + Display + 'static,
    C: CauseStore,
{
    let node: ReportJsonDocument = report.to_diagnostic_ir(options).into();
    let encoded = if options.json_pretty {
        serde_json::to_string_pretty(&node)
    } else {
        serde_json::to_string(&node)
    };
    match encoded {
        Ok(payload) => write!(f, "{payload}"),
        Err(_) => write!(f, "{{\"error\":\"json serialization failed\"}}"),
    }
}
