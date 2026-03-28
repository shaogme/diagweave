#[path = "report/ext.rs"]
mod ext;
#[path = "report/impls.rs"]
mod impls;
#[cfg(feature = "trace")]
#[path = "report/trace.rs"]
mod trace;
#[path = "report/types.rs"]
mod types;

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::error::Error;
use core::fmt::{self, Display};
use ref_str::StaticRefStr;
#[cfg(feature = "std")]
use std::sync::OnceLock;

pub use ext::{Diagnostic, ReportResultExt, ReportResultInspectExt};
pub use types::{
    Attachment, AttachmentValue, CauseCollectOptions, CauseKind, DisplayCauseChain, ErrorCode,
    ErrorCodeIntError, ReportMetadata, Severity, SourceErrorChain, SourceErrorEntry,
    SourceErrorItem, StackFrame, StackTrace, StackTraceFormat,
};
pub use types::{AttachmentVisit, CauseTraversalState, GlobalContext, ReportSourceErrorIter};

#[cfg(feature = "trace")]
pub use trace::{
    ParentSpanId, ReportTrace, SpanId, TraceContext, TraceEvent, TraceEventAttribute,
    TraceEventLevel, TraceId,
};

use types::{ColdData, DiagnosticBag, EMPTY_REPORT_METADATA};

/// A high-level diagnostic report that wraps an error with rich metadata and context.
pub struct Report<E> {
    inner: E,
    cold: Option<Box<ColdData>>,
}

impl<E> Report<E> {
    /// Creates a new report.
    pub fn new(inner: E) -> Self {
        #[cfg(feature = "std")]
        let mut report = Self { inner, cold: None };
        #[cfg(not(feature = "std"))]
        let report = Self { inner, cold: None };
        #[cfg(feature = "std")]
        report.apply_global_context();
        report
    }

    /// Returns a reference to the inner error.
    pub fn inner(&self) -> &E {
        &self.inner
    }

    /// Consumes the report and returns the inner error.
    pub fn into_inner(self) -> E {
        self.inner
    }

    /// Returns the attachments associated with the report.
    pub fn attachments(&self) -> &[Attachment] {
        match self.diagnostics() {
            Some(diag) => &diag.attachments,
            None => &[],
        }
    }

    /// Visits attachments in insertion order without building intermediate allocations.
    pub fn visit_attachments<F>(&self, mut visit: F) -> Result<(), fmt::Error>
    where
        F: FnMut(AttachmentVisit<'_>) -> fmt::Result,
    {
        let Some(diag) = self.diagnostics() else {
            return Ok(());
        };
        for attachment in &diag.attachments {
            match attachment {
                Attachment::Context { key, value } => {
                    visit(AttachmentVisit::Context { key, value })?;
                }
                Attachment::Note { message } => {
                    visit(AttachmentVisit::Note {
                        message: message.as_ref(),
                    })?;
                }
                Attachment::Payload {
                    name,
                    value,
                    media_type,
                } => {
                    visit(AttachmentVisit::Payload {
                        name,
                        value,
                        media_type: media_type.as_ref(),
                    })?;
                }
            }
        }
        Ok(())
    }

    /// Returns the display causes associated with the report.
    pub fn display_causes(&self) -> &[Arc<dyn Display + 'static>] {
        match self.diagnostics() {
            Some(diag) => diag
                .display_causes
                .as_ref()
                .map(|v| v.items.as_slice())
                .unwrap_or(&[]),
            None => &[],
        }
    }

    /// Returns the display-cause chain associated with the report, if any.
    #[cfg(feature = "json")]
    pub(crate) fn display_causes_chain(&self) -> Option<&DisplayCauseChain> {
        self.diagnostics()
            .and_then(|diag| diag.display_causes.as_ref())
    }

    /// Returns the source errors associated with the report.
    pub fn source_errors(&self) -> impl Iterator<Item = SourceErrorEntry> + '_
    where
        E: Error + 'static,
    {
        self.diagnostics()
            .and_then(|diag| diag.source_errors.as_ref())
            .map(SourceErrorChain::iter_entries)
            .into_iter()
            .flatten()
    }

    /// Returns the source-error chain associated with the report, if any.
    #[cfg(feature = "json")]
    pub(crate) fn source_errors_chain(&self) -> Option<&SourceErrorChain> {
        self.diagnostics()
            .and_then(|diag| diag.source_errors.as_ref())
    }

    /// Returns the metadata associated with the report.
    pub fn metadata(&self) -> &ReportMetadata {
        self.cold
            .as_ref()
            .map(|cold| &cold.metadata)
            .unwrap_or(&EMPTY_REPORT_METADATA)
    }

    /// Returns the error code from report metadata, if present.
    pub fn error_code(&self) -> Option<&ErrorCode> {
        self.metadata().error_code.as_ref()
    }

    /// Returns the severity from report metadata, if present.
    pub fn severity(&self) -> Option<Severity> {
        self.metadata().severity
    }

    /// Returns the category from report metadata, if present.
    pub fn category(&self) -> Option<&str> {
        self.metadata().category.as_deref()
    }

    /// Returns whether the report is marked retryable, if present.
    pub fn retryable(&self) -> Option<bool> {
        self.metadata().retryable
    }

    /// Returns the stack trace associated with the report, if any.
    pub fn stack_trace(&self) -> Option<&StackTrace> {
        self.diagnostics()
            .and_then(|diag| diag.stack_trace.as_ref())
    }

    fn diagnostics(&self) -> Option<&DiagnosticBag> {
        self.cold.as_ref().map(|cold| &cold.diagnostics)
    }

    fn ensure_cold(&mut self) -> &mut ColdData {
        self.cold
            .get_or_insert_with(|| Box::new(ColdData::default()))
            .as_mut()
    }

    fn diagnostics_mut(&mut self) -> &mut DiagnosticBag {
        &mut self.ensure_cold().diagnostics
    }

    #[cfg(feature = "std")]
    fn apply_global_context(&mut self) {
        let Some(injector) = global_context_injector().get() else {
            return;
        };
        let injected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(injector));
        let Some(global) = injected.unwrap_or_default() else {
            return;
        };

        #[cfg(feature = "trace")]
        let GlobalContext {
            context,
            trace_id,
            span_id,
            parent_span_id,
        } = global;
        #[cfg(not(feature = "trace"))]
        let GlobalContext { context } = global;

        let has_context = !context.is_empty();
        #[cfg(feature = "trace")]
        let has_trace = trace_id.is_some() || span_id.is_some() || parent_span_id.is_some();
        #[cfg(not(feature = "trace"))]
        let has_trace = false;

        if !has_context && !has_trace {
            return;
        }

        let diag = self.diagnostics_mut();
        for (key, value) in context {
            diag.attachments.push(Attachment::context(key, value));
        }
        #[cfg(feature = "trace")]
        if has_trace {
            let trace = diag.trace.get_or_insert_with(ReportTrace::default);
            if trace.context.trace_id.is_none() {
                trace.context.trace_id = trace_id;
            }
            if trace.context.span_id.is_none() {
                trace.context.span_id = span_id;
            }
            if trace.context.parent_span_id.is_none() {
                trace.context.parent_span_id = parent_span_id;
            }
        }
    }

    /// Attaches a context key-value pair to the report.
    pub fn attach(
        mut self,
        key: impl Into<StaticRefStr>,
        value: impl Into<AttachmentValue>,
    ) -> Self {
        self.diagnostics_mut()
            .attachments
            .push(Attachment::context(key, value));
        self
    }

    /// Attaches a printable note to the report.
    pub fn attach_printable(mut self, message: impl Display + 'static) -> Self {
        self.diagnostics_mut()
            .attachments
            .push(Attachment::note(message));
        self
    }

    /// Attaches a payload with an optional media type to the report.
    pub fn attach_payload(
        mut self,
        name: impl Into<StaticRefStr>,
        value: impl Into<AttachmentValue>,
        media_type: Option<impl Into<StaticRefStr>>,
    ) -> Self {
        self.diagnostics_mut().attachments.push(Attachment::payload(
            name,
            value,
            media_type.map(|m| m.into()),
        ));
        self
    }

    /// Adds context to the report (alias for `attach`).
    pub fn with_context(
        self,
        key: impl Into<StaticRefStr>,
        value: impl Into<AttachmentValue>,
    ) -> Self {
        self.attach(key, value)
    }

    /// Adds a note to the report (alias for `attach_printable`).
    pub fn with_note(self, message: impl Display + 'static) -> Self {
        self.attach_printable(message)
    }

    /// Adds a payload to the report (alias for `attach_payload`).
    pub fn with_payload(
        self,
        name: impl Into<StaticRefStr>,
        value: impl Into<AttachmentValue>,
        media_type: Option<impl Into<StaticRefStr>>,
    ) -> Self {
        self.attach_payload(name, value, media_type)
    }

    /// Sets the metadata for the report.
    pub fn with_metadata(mut self, metadata: ReportMetadata) -> Self {
        self.ensure_cold().metadata = metadata;
        self
    }

    /// Sets the error code for the report.
    pub fn with_error_code(mut self, error_code: impl Into<ErrorCode>) -> Self {
        self.ensure_cold().metadata.error_code = Some(error_code.into());
        self
    }

    /// Sets the severity for the report.
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.ensure_cold().metadata.severity = Some(severity);
        self
    }

    /// Sets the category for the report.
    pub fn with_category(mut self, category: impl Into<StaticRefStr>) -> Self {
        self.ensure_cold().metadata.category = Some(category.into());
        self
    }

    /// Sets whether the error is retryable.
    pub fn with_retryable(mut self, retryable: bool) -> Self {
        self.ensure_cold().metadata.retryable = Some(retryable);
        self
    }

    /// Sets the stack trace for the report.
    pub fn with_stack_trace(mut self, stack_trace: StackTrace) -> Self {
        self.diagnostics_mut().stack_trace = Some(stack_trace);
        self
    }

    /// Clears the stack trace from the report.
    pub fn clear_stack_trace(mut self) -> Self {
        self.diagnostics_mut().stack_trace = None;
        self
    }

    /// Captures the stack trace for the report if not already present.
    #[cfg(feature = "std")]
    pub fn capture_stack_trace(mut self) -> Self {
        if self.stack_trace().is_none() {
            self.diagnostics_mut().stack_trace = Some(StackTrace::capture_raw());
        }
        self
    }

    /// Forcefully captures the stack trace for the report.
    #[cfg(feature = "std")]
    pub fn force_capture_stack(mut self) -> Self {
        self.diagnostics_mut().stack_trace = Some(StackTrace::capture_raw());
        self
    }

    /// Adds a display cause to the report.
    pub fn with_display_cause(mut self, cause: impl Display + 'static) -> Self {
        self.diagnostics_mut()
            .display_causes
            .get_or_insert_with(DisplayCauseChain::default)
            .items
            .push(Arc::new(cause) as Arc<dyn Display + 'static>);
        self
    }

    /// Replaces the display-cause chain for the report.
    pub fn with_display_cause_chain(mut self, display_causes: DisplayCauseChain) -> Self {
        self.diagnostics_mut().display_causes = Some(display_causes);
        self
    }

    /// Adds multiple display causes to the report.
    pub fn with_display_causes<I, T>(mut self, causes: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Display + 'static,
    {
        self.diagnostics_mut()
            .display_causes
            .get_or_insert_with(DisplayCauseChain::default)
            .items
            .extend(
                causes
                    .into_iter()
                    .map(|cause| Arc::new(cause) as Arc<dyn Display + 'static>),
            );
        self
    }

    /// Adds an error source to the report's error chain.
    pub fn with_source_error(mut self, err: impl Error + 'static) -> Self {
        self.diagnostics_mut()
            .source_errors
            .get_or_insert_with(SourceErrorChain::default)
            .append(SourceErrorChain::from_error(err));
        self
    }

    /// Replaces the source-error chain for the report.
    pub fn with_source_error_chain(mut self, source_errors: SourceErrorChain) -> Self {
        self.diagnostics_mut().source_errors = Some(source_errors);
        self
    }

    /// Wraps the report into another error type.
    pub fn wrap<Outer>(self, outer: Outer) -> Report<Outer>
    where
        Self: Error + 'static,
        E: Error + 'static,
    {
        let source_errors = match self
            .diagnostics()
            .and_then(|diag| diag.source_errors.as_ref())
        {
            Some(source_errors) => Some(source_errors.clone()),
            None => self.source_errors_snapshot(CauseCollectOptions {
                max_depth: usize::MAX,
                detect_cycle: true,
            }),
        };
        let Report { inner, cold } = self;
        let source_report = Report { inner, cold };
        let source_state = source_errors
            .as_ref()
            .map(SourceErrorChain::state)
            .unwrap_or_default();
        let source_errors = SourceErrorChain {
            items: vec![
                SourceErrorItem::new(source_report).with_source(source_errors.map(Arc::new)),
            ]
            .into(),
            truncated: source_state.truncated,
            cycle_detected: source_state.cycle_detected,
        };
        Report {
            inner: outer,
            cold: Some(Box::new(ColdData {
                metadata: ReportMetadata::default(),
                diagnostics: DiagnosticBag {
                    #[cfg(feature = "trace")]
                    trace: None,
                    stack_trace: None,
                    attachments: Vec::new(),
                    display_causes: None,
                    source_errors: Some(source_errors),
                },
            })),
        }
    }

    /// Wraps the report using a mapping function for the inner error.
    pub fn wrap_with<Outer>(self, map: impl FnOnce(E) -> Outer) -> Report<Outer> {
        let Self { inner, cold } = self;
        let outer = map(inner);
        Report { inner: outer, cold }
    }

    /// Visits display causes using default collection options.
    pub fn visit_causes<F>(&self, visit: F) -> Result<CauseTraversalState, fmt::Error>
    where
        F: FnMut(&dyn Display) -> fmt::Result,
        E: Error + 'static,
    {
        self.visit_causes_ext(CauseCollectOptions::default(), visit)
    }

    /// Visits display causes using custom collection options.
    pub fn visit_causes_ext<F>(
        &self,
        options: CauseCollectOptions,
        mut visit: F,
    ) -> Result<CauseTraversalState, fmt::Error>
    where
        F: FnMut(&dyn Display) -> fmt::Result,
        E: Error + 'static,
    {
        let mut state = CauseTraversalState::default();
        let Some(diag) = self.diagnostics() else {
            return Ok(state);
        };
        let Some(display_causes) = diag.display_causes.as_ref() else {
            return Ok(state);
        };
        state.truncated |= display_causes.truncated;
        state.cycle_detected |= display_causes.cycle_detected;
        for (depth, cause) in display_causes.items.iter().enumerate() {
            if depth >= options.max_depth {
                state.truncated = true;
                break;
            }
            visit(cause.as_ref())?;
        }

        Ok(state)
    }

    /// Visits source errors using default collection options.
    pub fn visit_sources<F>(&self, visit: F) -> Result<CauseTraversalState, fmt::Error>
    where
        F: FnMut(SourceErrorEntry) -> fmt::Result,
        E: Error + 'static,
    {
        self.visit_sources_ext(CauseCollectOptions::default(), visit)
    }

    /// Visits source errors using custom collection options.
    pub fn visit_sources_ext<F>(
        &self,
        options: CauseCollectOptions,
        mut visit: F,
    ) -> Result<CauseTraversalState, fmt::Error>
    where
        F: FnMut(SourceErrorEntry) -> fmt::Result,
        E: Error + 'static,
    {
        let mut iter = self.iter_sources_ext(options);
        for err in iter.by_ref() {
            visit(err)?;
        }
        Ok(iter.state())
    }
}

/// Context injector type alias for global context providers.
#[cfg(feature = "std")]
pub(crate) type ContextInjector = dyn Fn() -> Option<GlobalContext> + Send + Sync + 'static;

#[cfg(feature = "std")]
pub(crate) fn global_context_injector() -> &'static OnceLock<Box<ContextInjector>> {
    static INJECTOR: OnceLock<Box<ContextInjector>> = OnceLock::new();
    &INJECTOR
}

/// Error returned when global context registration fails.
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisterGlobalContextError;

/// Registers a global context injector that will be invoked for every new report.
#[cfg(feature = "std")]
pub fn register_global_injector(
    injector: impl Fn() -> Option<GlobalContext> + Send + Sync + 'static,
) -> Result<(), RegisterGlobalContextError> {
    global_context_injector()
        .set(Box::new(injector))
        .map_err(|_| RegisterGlobalContextError)
}

impl<E> Report<E>
where
    E: Error + 'static,
{
    /// Iterates source errors using default collection options.
    pub fn iter_sources(&self) -> ReportSourceErrorIter<'_> {
        self.iter_sources_ext(CauseCollectOptions::default())
    }

    /// Iterates source errors using custom collection options.
    pub fn iter_sources_ext(&self, options: CauseCollectOptions) -> ReportSourceErrorIter<'_> {
        ReportSourceErrorIter::new(self, options)
    }
}

impl<E> Report<E>
where
    E: Error + 'static,
{
    pub(crate) fn source_errors_snapshot(
        &self,
        options: CauseCollectOptions,
    ) -> Option<SourceErrorChain> {
        let stored = self
            .diagnostics()
            .and_then(|diag| diag.source_errors.as_ref());
        let inner_source = self.inner.source();

        let mut snapshot = match (stored, inner_source) {
            (None, None) => None,
            (Some(chain), None) => Some(chain.clone()),
            (None, Some(source)) => Some(SourceErrorChain::from_source(source, options)),
            (Some(chain), Some(source)) => {
                let mut snapshot = chain.clone();
                snapshot.append(SourceErrorChain::from_source(source, options));
                Some(snapshot)
            }
        }?;
        snapshot.limit_depth(options, 0);
        if !options.detect_cycle {
            snapshot.clear_cycle_flags();
        }
        Some(snapshot)
    }
}
