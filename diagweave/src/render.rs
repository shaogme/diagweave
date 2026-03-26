#[cfg(feature = "json")]
#[path = "render/json.rs"]
mod json;
#[path = "render/pretty.rs"]
mod pretty;

use alloc::borrow::Cow;
use alloc::format;
use alloc::vec::Vec;
use core::any;
use core::error::Error;
use core::fmt::{self, Display, Formatter};

#[cfg(feature = "trace")]
use crate::report::ReportTrace;
use crate::report::{
    Attachment, AttachmentValue, CauseCollectOptions, ErrorCode, Report, Severity, StackTrace,
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
pub struct DiagnosticIrMetadata<'a> {
    pub error_code: Option<&'a ErrorCode>,
    pub severity: Option<Severity>,
    pub category: Option<&'a Cow<'static, str>>,
    pub retryable: Option<bool>,
    pub stack_trace: Option<&'a StackTrace>,
    pub display_causes: Option<DiagnosticIrDisplayCauseChain<'a>>,
    pub source_errors: Option<DiagnosticIrSourceErrorChain<'a>>,
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

pub(crate) struct AttachmentPayloadRef<'a> {
    pub name: &'a Cow<'static, str>,
    pub value: &'a AttachmentValue,
    pub media_type: Option<&'a Cow<'static, str>>,
}

pub(crate) struct AttachmentDispatch<'a> {
    pub contexts: Vec<(&'a Cow<'static, str>, &'a AttachmentValue)>,
    pub notes: Vec<&'a Cow<'static, str>>,
    pub payloads: Vec<AttachmentPayloadRef<'a>>,
}

pub(crate) fn dispatch_attachments(attachments: &[Attachment]) -> AttachmentDispatch<'_> {
    let mut contexts = Vec::new();
    let mut notes = Vec::new();
    let mut payloads = Vec::new();

    for item in attachments {
        match item {
            Attachment::Context { key, value } => contexts.push((key, value)),
            Attachment::Note { message } => notes.push(message),
            Attachment::Payload {
                name,
                value,
                media_type,
            } => payloads.push(AttachmentPayloadRef {
                name,
                value,
                media_type: media_type.as_ref(),
            }),
        }
    }

    AttachmentDispatch {
        contexts,
        notes,
        payloads,
    }
}

/// Context items borrowed from the report attachments.
pub struct DiagnosticIrContexts<'a> {
    attachments: &'a [Attachment],
    count: usize,
}

impl<'a> DiagnosticIrContexts<'a> {
    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn iter(&self) -> DiagnosticIrContextIter<'a> {
        DiagnosticIrContextIter {
            attachments: self.attachments.iter(),
        }
    }
}

impl<'a> IntoIterator for DiagnosticIrContexts<'a> {
    type Item = DiagnosticIrContext<'a>;
    type IntoIter = DiagnosticIrContextIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a DiagnosticIrContexts<'a> {
    type Item = DiagnosticIrContext<'a>;
    type IntoIter = DiagnosticIrContextIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterator over context items.
pub struct DiagnosticIrContextIter<'a> {
    attachments: core::slice::Iter<'a, Attachment>,
}

impl<'a> Iterator for DiagnosticIrContextIter<'a> {
    type Item = DiagnosticIrContext<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        for item in self.attachments.by_ref() {
            if let Attachment::Context { key, value } = item {
                return Some(DiagnosticIrContext {
                    key: Cow::Borrowed(key),
                    value,
                });
            }
        }
        None
    }
}

/// Attachment items borrowed from the report attachments.
pub struct DiagnosticIrAttachments<'a> {
    attachments: &'a [Attachment],
    count: usize,
}

impl<'a> DiagnosticIrAttachments<'a> {
    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn iter(&self) -> DiagnosticIrAttachmentIter<'a> {
        DiagnosticIrAttachmentIter {
            attachments: self.attachments.iter(),
        }
    }
}

impl<'a> IntoIterator for DiagnosticIrAttachments<'a> {
    type Item = DiagnosticIrAttachment<'a>;
    type IntoIter = DiagnosticIrAttachmentIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a DiagnosticIrAttachments<'a> {
    type Item = DiagnosticIrAttachment<'a>;
    type IntoIter = DiagnosticIrAttachmentIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterator over attachment items.
pub struct DiagnosticIrAttachmentIter<'a> {
    attachments: core::slice::Iter<'a, Attachment>,
}

impl<'a> Iterator for DiagnosticIrAttachmentIter<'a> {
    type Item = DiagnosticIrAttachment<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        for item in self.attachments.by_ref() {
            match item {
                Attachment::Context { .. } => {}
                Attachment::Note { message } => {
                    return Some(DiagnosticIrAttachment::Note { message });
                }
                Attachment::Payload {
                    name,
                    value,
                    media_type,
                } => {
                    return Some(DiagnosticIrAttachment::Payload {
                        name,
                        value,
                        media_type: media_type.as_ref(),
                    });
                }
            }
        }
        None
    }
}

/// Display cause items borrowed from the report.
pub struct DiagnosticIrDisplayCauseItems<'a> {
    causes: &'a [Box<dyn Display + 'static>],
    count: usize,
}

impl<'a> DiagnosticIrDisplayCauseItems<'a> {
    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn iter(&self) -> DiagnosticIrDisplayCauseIter<'a> {
        DiagnosticIrDisplayCauseIter {
            causes: self.causes.iter().take(self.count),
        }
    }
}

impl<'a> IntoIterator for DiagnosticIrDisplayCauseItems<'a> {
    type Item = &'a dyn Display;
    type IntoIter = DiagnosticIrDisplayCauseIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a DiagnosticIrDisplayCauseItems<'a> {
    type Item = &'a dyn Display;
    type IntoIter = DiagnosticIrDisplayCauseIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterator over display causes.
pub struct DiagnosticIrDisplayCauseIter<'a> {
    causes: core::iter::Take<core::slice::Iter<'a, Box<dyn Display + 'static>>>,
}

impl<'a> Iterator for DiagnosticIrDisplayCauseIter<'a> {
    type Item = &'a dyn Display;

    fn next(&mut self) -> Option<Self::Item> {
        self.causes.next().map(|cause| cause.as_ref())
    }
}

/// A borrowed display cause chain.
pub struct DiagnosticIrDisplayCauseChain<'a> {
    pub items: DiagnosticIrDisplayCauseItems<'a>,
    pub truncated: bool,
    pub cycle_detected: bool,
}

/// Source error items borrowed from the report.
pub struct DiagnosticIrSourceErrorItems<'a> {
    source_errors: &'a [Box<dyn Error + 'static>],
    root_source: Option<&'a (dyn Error + 'static)>,
    count: usize,
    max_depth: usize,
    detect_cycle: bool,
}

impl<'a> DiagnosticIrSourceErrorItems<'a> {
    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn iter(&self) -> DiagnosticIrSourceErrorIter<'a> {
        DiagnosticIrSourceErrorIter {
            source_errors: self.source_errors.iter().take(self.source_errors.len()),
            root_source: self.root_source,
            stage: SourceErrorStage::Attached,
            depth: 0,
            remaining: self.count,
            max_depth: self.max_depth,
            detect_cycle: self.detect_cycle,
            seen: SeenErrorAddrs::new(),
        }
    }
}

impl<'a> IntoIterator for DiagnosticIrSourceErrorItems<'a> {
    type Item = &'a (dyn Error + 'static);
    type IntoIter = DiagnosticIrSourceErrorIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a DiagnosticIrSourceErrorItems<'a> {
    type Item = &'a (dyn Error + 'static);
    type IntoIter = DiagnosticIrSourceErrorIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

enum SourceErrorStage {
    Attached,
    Inner,
    Done,
}

/// Iterator over source errors.
pub struct DiagnosticIrSourceErrorIter<'a> {
    source_errors: core::iter::Take<core::slice::Iter<'a, Box<dyn Error + 'static>>>,
    root_source: Option<&'a (dyn Error + 'static)>,
    stage: SourceErrorStage,
    depth: usize,
    remaining: usize,
    max_depth: usize,
    detect_cycle: bool,
    seen: SeenErrorAddrs,
}

impl<'a> Iterator for DiagnosticIrSourceErrorIter<'a> {
    type Item = &'a (dyn Error + 'static);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.remaining == 0 {
                self.stage = SourceErrorStage::Done;
                return None;
            }
            match self.stage {
                SourceErrorStage::Attached => {
                    if let Some(err) = self.source_errors.next() {
                        self.remaining -= 1;
                        return Some(err.as_ref());
                    }
                    self.stage = SourceErrorStage::Inner;
                }
                SourceErrorStage::Inner => {
                    let Some(current) = self.root_source else {
                        self.stage = SourceErrorStage::Done;
                        return None;
                    };

                    if self.depth >= self.max_depth {
                        self.stage = SourceErrorStage::Done;
                        return None;
                    }
                    if self.detect_cycle {
                        let ptr = (current as *const dyn Error) as *const ();
                        let addr = ptr as usize;
                        if !self.seen.insert(addr) {
                            self.stage = SourceErrorStage::Done;
                            return None;
                        }
                    }
                    self.depth += 1;
                    self.remaining -= 1;
                    self.root_source = current.source();
                    return Some(current);
                }
                SourceErrorStage::Done => return None,
            }
        }
    }
}

/// A borrowed source error chain.
pub struct DiagnosticIrSourceErrorChain<'a> {
    pub items: DiagnosticIrSourceErrorItems<'a>,
    pub truncated: bool,
    pub cycle_detected: bool,
}

/// A platform-agnostic intermediate representation of a diagnostic report.
pub struct DiagnosticIr<'a> {
    #[cfg(feature = "json")]
    pub schema_version: Cow<'static, str>,
    pub error: DiagnosticIrError<'a>,
    pub metadata: DiagnosticIrMetadata<'a>,
    #[cfg(feature = "trace")]
    pub trace: Option<&'a ReportTrace>,
    pub context: DiagnosticIrContexts<'a>,
    pub attachments: DiagnosticIrAttachments<'a>,
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
        let attachments = self.attachments();
        let (context_count, attachment_count) = count_attachments(attachments);

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
            context: DiagnosticIrContexts {
                attachments,
                count: context_count,
            },
            attachments: DiagnosticIrAttachments {
                attachments,
                count: attachment_count,
            },
        }
    }
}

fn count_attachments(report_attachments: &[Attachment]) -> (usize, usize) {
    let mut context = 0usize;
    let mut attachments = 0usize;
    for item in report_attachments {
        match item {
            Attachment::Context { .. } => context += 1,
            Attachment::Note { .. } | Attachment::Payload { .. } => attachments += 1,
        }
    }
    (context, attachments)
}

fn build_display_causes<E>(
    report: &Report<E>,
    options: CauseCollectOptions,
) -> Option<DiagnosticIrDisplayCauseChain<'_>>
where
    E: Error + Display + 'static,
{
    let items = report.display_causes();
    let count = items.len().min(options.max_depth);
    if count == 0 && items.is_empty() {
        return None;
    }

    Some(DiagnosticIrDisplayCauseChain {
        items: DiagnosticIrDisplayCauseItems {
            causes: items,
            count,
        },
        truncated: items.len() > options.max_depth,
        cycle_detected: false,
    })
}

fn build_source_errors<E>(
    report: &Report<E>,
    options: CauseCollectOptions,
) -> Option<DiagnosticIrSourceErrorChain<'_>>
where
    E: Error + Display + 'static,
{
    let source_errors = report.source_errors();
    let root_source = report.inner().source();
    let (count, truncated, cycle_detected) = count_source_errors(report, options);

    if count == 0 && !truncated && !cycle_detected {
        return None;
    }

    Some(DiagnosticIrSourceErrorChain {
        items: DiagnosticIrSourceErrorItems {
            source_errors,
            root_source,
            count,
            max_depth: options.max_depth,
            detect_cycle: options.detect_cycle,
        },
        truncated,
        cycle_detected,
    })
}

fn count_source_errors(
    report: &Report<impl Error + Display + 'static>,
    options: CauseCollectOptions,
) -> (usize, bool, bool) {
    let mut iter = report.iter_source_errors_with(options);
    let mut count = 0usize;
    for _ in iter.by_ref() {
        count += 1;
    }
    let state = iter.state();
    (count, state.truncated, state.cycle_detected)
}

struct SeenErrorAddrs {
    inline: [usize; 8],
    len: usize,
    spill: Vec<usize>,
}

impl SeenErrorAddrs {
    fn new() -> Self {
        Self {
            inline: [0usize; 8],
            len: 0,
            spill: Vec::new(),
        }
    }

    fn insert(&mut self, addr: usize) -> bool {
        if self.contains(addr) {
            return false;
        }
        if self.len < self.inline.len() {
            self.inline[self.len] = addr;
            self.len += 1;
        } else {
            self.spill.push(addr);
        }
        true
    }

    fn contains(&self, addr: usize) -> bool {
        self.inline[..self.len].contains(&addr) || self.spill.contains(&addr)
    }
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
