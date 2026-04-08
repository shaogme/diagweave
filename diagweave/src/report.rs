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
    ReportMetadata, ReportOptions, Severity, SeverityParseError, SeverityState, SourceErrorChain,
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

use types::{ColdData, DiagnosticBag, append_source_chain, limit_depth_source_chain};

/// A high-level diagnostic report that wraps an error with rich metadata and context.
pub struct Report<E, State = MissingSeverity> {
    inner: E,
    severity: State,
    cold: Option<Box<ColdData>>,
    options: ReportOptions,
}

impl<E> Report<E, MissingSeverity> {
    /// Creates a new report.
    pub fn new(inner: E) -> Self {
        #[cfg(feature = "std")]
        let mut report = Self {
            inner,
            severity: MissingSeverity,
            cold: None,
            options: ReportOptions::default(),
        };
        #[cfg(not(feature = "std"))]
        let report = Self {
            inner,
            severity: MissingSeverity,
            cold: None,
            options: ReportOptions::default(),
        };
        #[cfg(feature = "std")]
        report.apply_global_context();
        report
    }

    /// Sets the severity for the report.
    ///
    /// This is an alias for `set_severity()` for convenience when starting
    /// from a `Report<E, MissingSeverity>`.
    pub fn with_severity(self, severity: Severity) -> Report<E, HasSeverity> {
        self.set_severity(severity)
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
    pub fn metadata(&self) -> &ReportMetadata {
        match self.cold.as_deref() {
            Some(cold) => &cold.metadata,
            None => ReportMetadata::default_ref(),
        }
    }

    /// Returns the error code from report metadata, if present.
    pub fn error_code(&self) -> Option<&ErrorCode> {
        self.cold.as_deref().and_then(|c| c.metadata.error_code())
    }

    /// Returns the severity from report typestate.
    pub fn severity(&self) -> Option<Severity> {
        self.severity.severity()
    }

    /// Returns the severity state from report typestate.
    pub(crate) fn severity_state(&self) -> State {
        self.severity
    }

    /// Returns the category from report metadata, if present.
    pub fn category(&self) -> Option<&str> {
        self.cold.as_deref().and_then(|c| c.metadata.category())
    }

    /// Returns whether the report is marked retryable, if present.
    pub fn retryable(&self) -> Option<bool> {
        self.cold.as_deref().and_then(|c| c.metadata.retryable())
    }

    /// Returns the stack trace associated with the report, if any.
    pub fn stack_trace(&self) -> Option<&StackTrace> {
        self.diagnostics()
            .and_then(|diag| diag.stack_trace.as_ref())
    }

    fn diagnostics(&self) -> Option<&DiagnosticBag> {
        self.cold.as_deref().map(|cold| &cold.bag)
    }

    fn ensure_cold(&mut self) -> &mut ColdData {
        self.cold
            .get_or_insert_with(|| Box::new(ColdData::default()))
    }

    fn diagnostics_mut(&mut self) -> &mut DiagnosticBag {
        &mut self.ensure_cold().bag
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

        let GlobalContext {
            #[cfg(feature = "trace")]
            trace,
            error,
            system,
            context,
        } = global;

        if let Some(error) = error {
            let cold = self.ensure_cold();
            if let Some(error_code) = error.error_code {
                cold.metadata = cold.metadata.clone().with_error_code(error_code);
            }
            if let Some(category) = error.category {
                cold.metadata = cold.metadata.clone().with_category(category);
            }
            if let Some(retryable) = error.retryable {
                cold.metadata = cold.metadata.clone().with_retryable(retryable);
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
    pub fn with_metadata(mut self, metadata: ReportMetadata) -> Self {
        if let Some(cold) = self.cold.as_mut() {
            cold.metadata = metadata;
        } else {
            self.cold = Some(Box::new(ColdData {
                metadata,
                bag: DiagnosticBag::default(),
            }));
        }
        self
    }

    /// Sets the error code for the report, replacing any existing value.
    pub fn set_error_code(mut self, error_code: impl Into<ErrorCode>) -> Self {
        let cold = self.ensure_cold();
        cold.metadata = cold.metadata.clone().set_error_code(error_code);
        self
    }

    /// Sets the error code only if not already set.
    pub fn with_error_code(mut self, error_code: impl Into<ErrorCode>) -> Self {
        let cold = self.ensure_cold();
        cold.metadata = cold.metadata.clone().with_error_code(error_code);
        self
    }

    /// Sets the severity for the report, replacing any existing value.
    pub fn set_severity(self, severity: Severity) -> Report<E, HasSeverity> {
        let Self {
            inner,
            severity: _,
            cold,
            options,
        } = self;
        Report {
            inner,
            severity: HasSeverity::new(severity),
            cold,
            options,
        }
    }

    /// Sets the category for the report, replacing any existing value.
    pub fn set_category(mut self, category: impl Into<StaticRefStr>) -> Self {
        let cold = self.ensure_cold();
        cold.metadata = cold.metadata.clone().set_category(category);
        self
    }

    /// Sets the category only if not already set.
    pub fn with_category(mut self, category: impl Into<StaticRefStr>) -> Self {
        let cold = self.ensure_cold();
        cold.metadata = cold.metadata.clone().with_category(category);
        self
    }

    /// Sets whether the error is retryable, replacing any existing value.
    pub fn set_retryable(mut self, retryable: bool) -> Self {
        let cold = self.ensure_cold();
        cold.metadata = cold.metadata.clone().set_retryable(retryable);
        self
    }

    /// Sets whether the error is retryable only if not already set.
    pub fn with_retryable(mut self, retryable: bool) -> Self {
        let cold = self.ensure_cold();
        cold.metadata = cold.metadata.clone().with_retryable(retryable);
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

    /// Sets the report options for this report.
    ///
    /// This replaces any existing options with the provided ones.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use diagweave::{Report, ReportOptions};
    ///
    /// // Disable source chain accumulation for this specific report
    /// let report = report.set_options(ReportOptions::new(false));
    /// ```
    pub fn set_options(mut self, options: ReportOptions) -> Self {
        self.options = options;
        self
    }

    /// Sets whether source chains should be accumulated during `map_err()`.
    ///
    /// This is a convenience method for setting the `accumulate_source_chain` option.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use diagweave::Report;
    ///
    /// // Enable source chain accumulation for this specific report
    /// let report = report.set_accumulate_source_chain(true);
    /// ```
    pub fn set_accumulate_source_chain(mut self, accumulate: bool) -> Self {
        self.options = ReportOptions::new(accumulate);
        self
    }

    /// Returns the current report options.
    pub fn options(&self) -> &ReportOptions {
        &self.options
    }

    /// Maps the inner error type while preserving all diagnostic data.
    ///
    /// When source chain accumulation is enabled via [`ReportOptions::accumulate_source_chain`],
    /// this method also accumulates the origin source error chain, similar to the old `boundary()` behavior.
    ///
    /// # Source Chain Accumulation
    ///
    /// If `accumulate_source_chain` is `true`:
    /// - The current report's origin source chain (if any) is preserved
    /// - The old inner error is added as a source of the new outer error
    /// - The resulting chain reflects: `outer -> old_inner -> ...old sources`
    ///
    /// If `accumulate_source_chain` is `false`:
    /// - Only the error type is transformed
    /// - No source chain manipulation occurs
    ///
    /// # Example
    ///
    /// ```ignore
    /// use diagweave::{Report, ReportOptions};
    ///
    /// // Transform error type while preserving diagnostics
    /// let report: Report<OuterError> = inner_report.map_err(|e| OuterError::from(e));
    ///
    /// // Control source chain accumulation per-report
    /// let report = report.set_accumulate_source_chain(false); // Disable accumulation
    /// ```
    pub fn map_err<Outer>(self, map: impl FnOnce(E) -> Outer) -> Report<Outer, State>
    where
        E: Error + Send + Sync + 'static,
        Outer: Error + Send + Sync + 'static,
    {
        let Self {
            inner,
            severity,
            cold,
            options,
        } = self;

        // Check if source chain accumulation is enabled for this report
        if options.accumulate_source_chain {
            // Build origin source chain with the old inner as the new root
            // We use a borrowed representation (StringError) to avoid ownership issues

            // First, compute the existing source chain from inner.source() or stored chain
            let existing_source_chain = cold
                .as_ref()
                .and_then(|c| c.bag.origin_source_errors.clone())
                .or_else(|| {
                    inner.source().map(|source| {
                        SourceErrorChain::from_source(
                            source,
                            CauseCollectOptions {
                                max_depth: usize::MAX,
                                detect_cycle: true,
                            },
                        )
                    })
                });

            // Get type name for inner before moving it
            let inner_type_name: Option<StaticRefStr> = Some(core::any::type_name::<E>().into());

            // Create origin chain with old inner's info as root (using borrowed representation)
            let origin_source_errors = SourceErrorChain::from_borrowed_error(
                &inner,
                inner_type_name,
                existing_source_chain,
                CauseTraversalState::default(),
            );

            // Now create the outer error
            let outer = map(inner);

            // Extract diagnostic data from the original cold
            let new_cold = match cold {
                Some(c) => {
                    let c = *c;
                    Some(Box::new(ColdData {
                        metadata: c.metadata,
                        bag: DiagnosticBag {
                            #[cfg(feature = "trace")]
                            trace: c.bag.trace,
                            stack_trace: c.bag.stack_trace,
                            context: c.bag.context,
                            system: c.bag.system,
                            attachments: c.bag.attachments,
                            display_causes: c.bag.display_causes,
                            origin_source_errors: Some(origin_source_errors),
                            diagnostic_source_errors: c.bag.diagnostic_source_errors,
                        },
                    }))
                }
                None => {
                    let mut cold_data = ColdData::new(ReportMetadata::default());
                    cold_data.bag.origin_source_errors = Some(origin_source_errors);
                    Some(Box::new(cold_data))
                }
            };

            Report {
                inner: outer,
                severity,
                cold: new_cold,
                options,
            }
        } else {
            // Simple transformation without source chain accumulation
            let outer = map(inner);
            Report {
                inner: outer,
                severity,
                cold,
                options,
            }
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

impl<E> Report<E, HasSeverity> {
    /// Sets the severity only if not already set (returns self unchanged since severity is already present).
    ///
    /// This allows conditional chaining without type-state changes when severity
    /// is already set. Use `set_severity()` to force a new severity value.
    pub fn with_severity(self, _severity: Severity) -> Self {
        self
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
