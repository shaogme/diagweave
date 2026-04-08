#[path = "report/ext.rs"]
mod ext;
#[path = "report/impls.rs"]
mod impls;
#[path = "report/source_view.rs"]
mod source_view;
#[cfg(feature = "trace")]
#[path = "report/trace.rs"]
mod trace;
#[path = "report/types.rs"]
mod types;

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::error::Error;
use core::fmt::{self, Display};
use ref_str::StaticRefStr;
#[cfg(feature = "std")]
use std::sync::OnceLock;

pub use ext::{Diagnostic, InspectReportExt, ResultReportExt};
pub use types::{
    Attachment, AttachmentValue, CauseCollectOptions, CauseKind, ContextMap, ContextValue,
    DisplayCauseChain, ErrorCode, ErrorCodeIntError, GlobalErrorMeta, HasSeverity, MissingSeverity,
    ReportMetadata, Severity, SeverityParseError, SeverityState, SourceErrorChain,
    SourceErrorEntry, SourceErrorItem, StackFrame, StackTrace, StackTraceFormat, SystemContext,
};
pub use types::{AttachmentVisit, CauseTraversalState, GlobalContext, ReportSourceErrorIter};
#[cfg(feature = "json")]
pub use types::{JsonContext, JsonContextEntry};

#[cfg(feature = "trace")]
pub use trace::{
    ParentSpanId, ReportTrace, SpanId, TraceContext, TraceEvent, TraceEventAttribute,
    TraceEventLevel, TraceFlags, TraceId, TraceState,
};
#[cfg(feature = "trace")]
pub use types::GlobalTraceContext;

use types::{DiagnosticBag, append_source_chain, limit_depth_source_chain};

/// A high-level diagnostic report that wraps an error with rich metadata and context.
pub struct Report<E, State = MissingSeverity> {
    inner: E,
    metadata: ReportMetadata<State>,
    cold: Option<Box<DiagnosticBag>>,
}

impl<E> Report<E, MissingSeverity> {
    /// Creates a new report.
    pub fn new(inner: E) -> Self {
        #[cfg(feature = "std")]
        let mut report = Self {
            inner,
            metadata: ReportMetadata::default(),
            cold: None,
        };
        #[cfg(not(feature = "std"))]
        let report = Self {
            inner,
            metadata: ReportMetadata::default(),
            cold: None,
        };
        #[cfg(feature = "std")]
        report.apply_global_context();
        report
    }
}

impl<E, State> Report<E, State>
where
    State: SeverityState,
{
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

    /// Returns context key-value pairs associated with the report.
    ///
    /// Returns a reference to an empty [`ContextMap`] if no context has been set.
    pub fn context(&self) -> &ContextMap {
        match self.diagnostics() {
            Some(diag) => &diag.context,
            None => ContextMap::default_ref(),
        }
    }

    /// Returns structured system fields associated with the report.
    ///
    /// Returns a reference to an empty [`SystemContext`] if no system context has been set.
    pub fn system(&self) -> &SystemContext {
        match self.diagnostics() {
            Some(diag) => &diag.system,
            None => SystemContext::default_ref(),
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
    pub fn display_causes(&self) -> &[Arc<dyn Display + Send + Sync + 'static>] {
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

    /// Returns source errors from the origin chain associated with the report.
    pub fn origin_source_errors(&self) -> impl Iterator<Item = SourceErrorEntry> + '_
    where
        E: Error + 'static,
    {
        self.origin_src_err_view(CauseCollectOptions::default())
            .map(|chain| chain.iter_entries_origin().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
    }

    /// Returns source errors from the diagnostic chain associated with the report.
    pub fn diag_source_errors(&self) -> impl Iterator<Item = SourceErrorEntry> + '_
    where
        E: Error + 'static,
    {
        self.diagnostics()
            .and_then(|diag| diag.diagnostic_source_errors.as_ref())
            .map(SourceErrorChain::iter_entries)
            .into_iter()
            .flatten()
    }

    /// Returns the origin source-error chain associated with the report, if any.
    #[cfg(feature = "json")]
    pub(crate) fn origin_src_err_chain(&self) -> Option<&SourceErrorChain> {
        self.diagnostics()
            .and_then(|diag| diag.origin_source_errors.as_ref())
    }

    /// Returns the diagnostic source-error chain associated with the report, if any.
    #[cfg(feature = "json")]
    pub(crate) fn diag_src_err_chain(&self) -> Option<&SourceErrorChain> {
        self.diagnostics()
            .and_then(|diag| diag.diagnostic_source_errors.as_ref())
    }

    /// Returns the metadata associated with the report.
    pub fn metadata(&self) -> &ReportMetadata<State> {
        &self.metadata
    }

    /// Returns the error code from report metadata, if present.
    pub fn error_code(&self) -> Option<&ErrorCode> {
        self.metadata().error_code()
    }

    /// Returns the severity from report metadata, if present.
    pub fn severity(&self) -> Option<Severity> {
        self.metadata().severity()
    }

    /// Returns the category from report metadata, if present.
    pub fn category(&self) -> Option<&str> {
        self.metadata().category()
    }

    /// Returns whether the report is marked retryable, if present.
    pub fn retryable(&self) -> Option<bool> {
        self.metadata().retryable()
    }

    /// Returns the stack trace associated with the report, if any.
    pub fn stack_trace(&self) -> Option<&StackTrace> {
        self.diagnostics()
            .and_then(|diag| diag.stack_trace.as_ref())
    }

    fn diagnostics(&self) -> Option<&DiagnosticBag> {
        self.cold.as_deref()
    }

    fn ensure_cold(&mut self) -> &mut DiagnosticBag {
        self.cold
            .get_or_insert_with(|| Box::new(DiagnosticBag::default()))
            .as_mut()
    }

    fn diagnostics_mut(&mut self) -> &mut DiagnosticBag {
        self.ensure_cold()
    }

    #[cfg(feature = "std")]
    fn apply_global_context(&mut self)
    where
        State: Clone,
    {
        let Some(injector) = global_context_injector().get() else {
            return;
        };
        let injected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(injector));
        let Some(global) = injected.unwrap_or_default() else {
            return;
        };

        let GlobalContext {
            #[cfg(feature = "trace")]
            trace,
            error,
            system,
            context,
        } = global;

        if let Some(error) = error {
            if let Some(error_code) = error.error_code {
                self.metadata = self.metadata.clone().with_error_code(error_code);
            }
            if let Some(category) = error.category {
                self.metadata = self.metadata.clone().with_category(category);
            }
            if let Some(retryable) = error.retryable {
                self.metadata = self.metadata.clone().with_retryable(retryable);
            }
        }

        if !system.is_empty() {
            let diag = self.diagnostics_mut();
            diag.system = system;
        }
        if !context.is_empty() {
            let diag = self.diagnostics_mut();
            for (key, value) in &context {
                diag.context.insert(key.clone(), value.clone());
            }
        }

        #[cfg(feature = "trace")]
        if let Some(trace) = trace {
            let report_trace = self
                .diagnostics_mut()
                .trace
                .get_or_insert_with(ReportTrace::default);
            report_trace.context.trace_id = trace.trace_id;
            report_trace.context.span_id = trace.span_id;
            report_trace.context.parent_span_id = trace.parent_span_id;
            report_trace.context.sampled = trace.sampled;
            report_trace.context.trace_state = trace.trace_state;
            report_trace.context.flags = trace.flags;
        }
    }

    /// Adds a business context key-value pair to the report.
    pub fn with_ctx(
        mut self,
        key: impl Into<StaticRefStr>,
        value: impl Into<ContextValue>,
    ) -> Self {
        self.diagnostics_mut().context.insert(key, value.into());
        self
    }

    /// Adds a system context key-value pair to the report.
    pub fn with_system(
        mut self,
        key: impl Into<StaticRefStr>,
        value: impl Into<ContextValue>,
    ) -> Self {
        self.diagnostics_mut().system.insert(key, value.into());
        self
    }

    /// Replaces the structured system context for the report.
    pub fn with_system_context(mut self, system: SystemContext) -> Self {
        self.diagnostics_mut().system = system;
        self
    }

    /// Attaches a printable note to the report.
    pub fn attach_printable(mut self, message: impl Display + Send + Sync + 'static) -> Self {
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

    /// Adds a note to the report (alias for `attach_printable`).
    pub fn attach_note(self, message: impl Display + Send + Sync + 'static) -> Self {
        self.attach_printable(message)
    }

    /// Sets the metadata for the report.
    pub fn with_metadata<NewState>(self, metadata: ReportMetadata<NewState>) -> Report<E, NewState>
    where
        NewState: SeverityState,
    {
        let Self { inner, cold, .. } = self;
        Report {
            inner,
            metadata,
            cold,
        }
    }

    /// Sets the error code for the report.
    pub fn with_error_code(mut self, error_code: impl Into<ErrorCode>) -> Self {
        self.metadata = self.metadata.with_error_code(error_code);
        self
    }

    /// Sets the severity for the report.
    pub fn with_severity(self, severity: Severity) -> Report<E, HasSeverity> {
        let Self {
            inner,
            metadata,
            cold,
        } = self;
        Report {
            inner,
            metadata: metadata.with_severity(severity),
            cold,
        }
    }

    /// Sets the category for the report.
    pub fn with_category(mut self, category: impl Into<StaticRefStr>) -> Self {
        self.metadata = self.metadata.with_category(category);
        self
    }

    /// Sets whether the error is retryable.
    pub fn with_retryable(mut self, retryable: bool) -> Self {
        self.metadata = self.metadata.with_retryable(retryable);
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
    pub fn with_display_cause(mut self, cause: impl Display + Send + Sync + 'static) -> Self {
        self.diagnostics_mut()
            .display_causes
            .get_or_insert_with(DisplayCauseChain::default)
            .items
            .push(Arc::new(cause) as Arc<dyn Display + Send + Sync + 'static>);
        self
    }

    /// Replaces the display-cause chain for the report.
    pub fn set_display_causes(mut self, display_causes: DisplayCauseChain) -> Self {
        self.diagnostics_mut().display_causes = Some(display_causes);
        self
    }

    /// Adds multiple display causes to the report.
    pub fn with_display_causes<I, T>(mut self, causes: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Display + Send + Sync + 'static,
    {
        self.diagnostics_mut()
            .display_causes
            .get_or_insert_with(DisplayCauseChain::default)
            .items
            .extend(
                causes
                    .into_iter()
                    .map(|cause| Arc::new(cause) as Arc<dyn Display + Send + Sync + 'static>),
            );
        self
    }

    /// Adds an error source to the report's diagnostic source chain.
    pub fn with_diag_src_err(mut self, err: impl Error + Send + Sync + 'static) -> Self {
        let existing = self
            .diagnostics_mut()
            .diagnostic_source_errors
            .get_or_insert_with(SourceErrorChain::default);
        append_source_chain(existing, SourceErrorChain::from_error(err));
        self
    }

    /// Replaces the diagnostic source-error chain for the report.
    pub fn set_diag_src_errs(mut self, source_errors: SourceErrorChain) -> Self {
        self.diagnostics_mut().diagnostic_source_errors = Some(source_errors);
        self
    }

    /// Wraps the report into another error type, creating a diagnostic boundary.
    pub fn boundary<Outer>(self, outer: Outer) -> Report<Outer, MissingSeverity>
    where
        Self: Error + Send + Sync + 'static,
        E: Error + Send + Sync + 'static,
    {
        let origin_source_errors = match self
            .diagnostics()
            .and_then(|diag| diag.origin_source_errors.as_ref())
        {
            Some(source_errors) => Some(source_errors.clone()),
            None => self.origin_src_err_view(CauseCollectOptions {
                max_depth: usize::MAX,
                detect_cycle: true,
            }),
        };
        let Report {
            inner,
            metadata,
            cold,
        } = self;
        let source_report = Report {
            inner,
            metadata,
            cold,
        };
        let source_state = origin_source_errors
            .as_ref()
            .map(SourceErrorChain::state)
            .unwrap_or_default();
        let origin_source_errors =
            SourceErrorChain::from_root_source(source_report, origin_source_errors, source_state);
        Report {
            inner: outer,
            metadata: ReportMetadata::default(),
            cold: Some(Box::new(DiagnosticBag {
                #[cfg(feature = "trace")]
                trace: None,
                stack_trace: None,
                context: ContextMap::default(),
                system: SystemContext::default(),
                attachments: Vec::new(),
                display_causes: None,
                origin_source_errors: Some(origin_source_errors),
                diagnostic_source_errors: None,
            })),
        }
    }

    /// Maps the inner error type while preserving all diagnostic data.
    pub fn map_err<Outer>(self, map: impl FnOnce(E) -> Outer) -> Report<Outer, State> {
        let Self {
            inner,
            metadata,
            cold,
        } = self;
        let outer = map(inner);
        Report {
            inner: outer,
            metadata,
            cold,
        }
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

    /// Visits origin source errors using default collection options.
    pub fn visit_origin_sources<F>(&self, visit: F) -> Result<CauseTraversalState, fmt::Error>
    where
        F: FnMut(SourceErrorEntry) -> fmt::Result,
        E: Error + 'static,
    {
        self.visit_origin_src_ext(CauseCollectOptions::default(), visit)
    }

    /// Visits origin source errors using custom collection options.
    pub fn visit_origin_src_ext<F>(
        &self,
        options: CauseCollectOptions,
        mut visit: F,
    ) -> Result<CauseTraversalState, fmt::Error>
    where
        F: FnMut(SourceErrorEntry) -> fmt::Result,
        E: Error + 'static,
    {
        let mut iter = self.iter_origin_src_ext(options);
        for err in iter.by_ref() {
            visit(err)?;
        }
        Ok(iter.state())
    }

    /// Visits diagnostic source errors using default collection options.
    pub fn visit_diag_sources<F>(&self, visit: F) -> Result<CauseTraversalState, fmt::Error>
    where
        F: FnMut(SourceErrorEntry) -> fmt::Result,
        E: Error + 'static,
    {
        self.visit_diag_srcs_ext(CauseCollectOptions::default(), visit)
    }

    /// Visits diagnostic source errors using custom collection options.
    pub fn visit_diag_srcs_ext<F>(
        &self,
        options: CauseCollectOptions,
        mut visit: F,
    ) -> Result<CauseTraversalState, fmt::Error>
    where
        F: FnMut(SourceErrorEntry) -> fmt::Result,
        E: Error + 'static,
    {
        let mut iter = self.iter_diag_srcs_ext(options);
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
