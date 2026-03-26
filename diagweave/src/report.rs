#[path = "report/ext.rs"]
mod ext;
#[path = "report/impls.rs"]
mod impls;
#[path = "report/types.rs"]
mod types;

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::error::Error;
use core::fmt::{self, Display};
#[cfg(feature = "std")]
use std::panic::{AssertUnwindSafe, catch_unwind};
#[cfg(feature = "std")]
use std::sync::OnceLock;

pub use ext::{Diagnostic, ReportResultExt, ReportResultInspectExt};
pub use types::{
    Attachment, AttachmentValue, CauseCollectOptions, CauseKind, DisplayCauseChain, ErrorCode,
    ErrorCodeIntError, ReportMetadata, Severity, SourceError, SourceErrorChain, StackFrame,
    StackTrace, StackTraceFormat,
};
#[cfg(feature = "trace")]
pub use types::{ReportTrace, TraceContext, TraceEvent, TraceEventAttribute, TraceEventLevel};

/// A high-level diagnostic report that wraps an error with rich metadata and context.
pub struct Report<E> {
    inner: E,
    cold: Option<Box<ColdData>>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CauseTraversalState {
    pub truncated: bool,
    pub cycle_detected: bool,
}

enum SourceErrorIterStage {
    Attached,
    Inner,
    Done,
}

/// Iterator over source errors with depth/cycle control.
pub struct ReportSourceErrorIter<'a> {
    source_errors: core::slice::Iter<'a, Box<dyn Error + 'static>>,
    root_source: Option<&'a (dyn Error + 'static)>,
    current: Option<&'a (dyn Error + 'static)>,
    stage: SourceErrorIterStage,
    depth: usize,
    options: CauseCollectOptions,
    seen: SeenErrorAddrs,
    state: CauseTraversalState,
}

impl<'a> ReportSourceErrorIter<'a> {
    /// Returns traversal state observed so far.
    pub fn state(&self) -> CauseTraversalState {
        self.state
    }
}

impl<'a> Iterator for ReportSourceErrorIter<'a> {
    type Item = &'a (dyn Error + 'static);

    fn next(&mut self) -> Option<Self::Item> {
        if self.state.truncated || self.state.cycle_detected {
            self.stage = SourceErrorIterStage::Done;
            return None;
        }

        loop {
            let err = match self.current.take() {
                Some(err) => err,
                None => match self.stage {
                    SourceErrorIterStage::Attached => {
                        if let Some(err) = self.source_errors.next() {
                            err.as_ref()
                        } else {
                            self.stage = SourceErrorIterStage::Inner;
                            continue;
                        }
                    }
                    SourceErrorIterStage::Inner => {
                        let Some(err) = self.root_source.take() else {
                            self.stage = SourceErrorIterStage::Done;
                            return None;
                        };
                        err
                    }
                    SourceErrorIterStage::Done => return None,
                },
            };

            if self.depth >= self.options.max_depth {
                self.state.truncated = true;
                self.stage = SourceErrorIterStage::Done;
                return None;
            }
            if self.options.detect_cycle {
                let ptr = (err as *const dyn Error) as *const ();
                let addr = ptr as usize;
                if !self.seen.insert(addr) {
                    self.state.cycle_detected = true;
                    self.stage = SourceErrorIterStage::Done;
                    return None;
                }
            }
            self.current = err.source();
            self.depth += 1;
            return Some(err);
        }
    }
}

#[derive(Default)]
struct ColdData {
    metadata: ReportMetadata,
    diagnostics: DiagnosticBag,
}

#[derive(Default)]
struct DiagnosticBag {
    #[cfg(feature = "trace")]
    trace: ReportTrace,
    attachments: Vec<Attachment>,
    display_causes: Vec<Box<dyn Display + 'static>>,
    source_errors: Vec<Box<dyn Error + 'static>>,
}

const EMPTY_REPORT_METADATA: ReportMetadata = ReportMetadata {
    error_code: None,
    severity: None,
    category: None,
    retryable: None,
    stack_trace: None,
    display_causes: None,
    source_errors: None,
};

/// Global context information that can be injected into reports.
#[derive(Debug, Clone, Default)]
pub struct GlobalContext {
    /// Context key-value pairs.
    pub context: Vec<(Cow<'static, str>, AttachmentValue)>,
    /// Global trace ID if available.
    #[cfg(feature = "trace")]
    pub trace_id: Option<Cow<'static, str>>,
    /// Global span ID if available.
    #[cfg(feature = "trace")]
    pub span_id: Option<Cow<'static, str>>,
    /// Global parent span ID if available.
    #[cfg(feature = "trace")]
    pub parent_span_id: Option<Cow<'static, str>>,
}

/// Context injector type alias for global context providers.
#[cfg(feature = "std")]
type ContextInjector = dyn Fn() -> Option<GlobalContext> + Send + Sync + 'static;

#[cfg(feature = "std")]
fn global_context_injector() -> &'static OnceLock<Box<ContextInjector>> {
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

    /// Returns the display causes associated with the report.
    pub fn display_causes(&self) -> &[Box<dyn Display + 'static>] {
        match self.diagnostics() {
            Some(diag) => &diag.display_causes,
            None => &[],
        }
    }

    /// Returns the source errors associated with the report.
    pub fn source_errors(&self) -> &[Box<dyn Error + 'static>] {
        match self.diagnostics() {
            Some(diag) => &diag.source_errors,
            None => &[],
        }
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
        self.metadata().stack_trace.as_ref()
    }

    /// Returns the trace information associated with the report, if any.
    #[cfg(feature = "trace")]
    pub fn trace(&self) -> Option<&ReportTrace> {
        self.diagnostics().map(|diag| &diag.trace)
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
        let injected = catch_unwind(AssertUnwindSafe(injector));
        let Some(global) = injected.unwrap_or_default() else {
            return;
        };
        for (key, value) in global.context {
            self.diagnostics_mut()
                .attachments
                .push(Attachment::context(key, value));
        }
        #[cfg(feature = "trace")]
        {
            let trace = &mut self.diagnostics_mut().trace.context;
            if trace.trace_id.is_none() {
                trace.trace_id = global.trace_id;
            }
            if trace.span_id.is_none() {
                trace.span_id = global.span_id;
            }
            if trace.parent_span_id.is_none() {
                trace.parent_span_id = global.parent_span_id;
            }
        }
    }

    /// Attaches a context key-value pair to the report.
    pub fn attach(
        mut self,
        key: impl Into<Cow<'static, str>>,
        value: impl Into<AttachmentValue>,
    ) -> Self {
        self.diagnostics_mut()
            .attachments
            .push(Attachment::context(key, value));
        self
    }

    /// Attaches a printable note to the report.
    pub fn attach_printable(mut self, message: impl Display) -> Self {
        self.diagnostics_mut().attachments.push(Attachment::Note {
            message: Cow::Owned(message.to_string()),
        });
        self
    }

    /// Attaches a payload with an optional media type to the report.
    pub fn attach_payload(
        mut self,
        name: impl Into<Cow<'static, str>>,
        value: impl Into<AttachmentValue>,
        media_type: Option<impl Into<Cow<'static, str>>>,
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
        key: impl Into<Cow<'static, str>>,
        value: impl Into<AttachmentValue>,
    ) -> Self {
        self.attach(key, value)
    }

    /// Adds a note to the report (alias for `attach_printable`).
    pub fn with_note(self, message: impl Display) -> Self {
        self.attach_printable(message)
    }

    /// Adds a payload to the report (alias for `attach_payload`).
    pub fn with_payload(
        self,
        name: impl Into<Cow<'static, str>>,
        value: impl Into<AttachmentValue>,
        media_type: Option<impl Into<Cow<'static, str>>>,
    ) -> Self {
        self.attach_payload(name, value, media_type)
    }

    /// Sets the metadata for the report.
    pub fn with_metadata(mut self, metadata: ReportMetadata) -> Self {
        self.ensure_cold().metadata = metadata;
        self
    }

    /// Sets the trace information for the report.
    #[cfg(feature = "trace")]
    pub fn with_trace(mut self, trace: ReportTrace) -> Self {
        self.diagnostics_mut().trace = trace;
        self
    }

    /// Sets the trace and span IDs for the report.
    #[cfg(feature = "trace")]
    pub fn with_trace_ids(
        mut self,
        trace_id: impl Into<Cow<'static, str>>,
        span_id: impl Into<Cow<'static, str>>,
    ) -> Self {
        let trace = &mut self.diagnostics_mut().trace;
        trace.context.trace_id = Some(trace_id.into());
        trace.context.span_id = Some(span_id.into());
        self
    }

    /// Sets the parent span ID for the report.
    #[cfg(feature = "trace")]
    pub fn with_parent_span_id(mut self, parent_span_id: impl Into<Cow<'static, str>>) -> Self {
        self.diagnostics_mut().trace.context.parent_span_id = Some(parent_span_id.into());
        self
    }

    /// Sets whether the trace is sampled.
    #[cfg(feature = "trace")]
    pub fn with_trace_sampled(mut self, sampled: bool) -> Self {
        self.diagnostics_mut().trace.context.sampled = Some(sampled);
        self
    }

    /// Sets the trace state.
    #[cfg(feature = "trace")]
    pub fn with_trace_state(mut self, trace_state: impl Into<Cow<'static, str>>) -> Self {
        self.diagnostics_mut().trace.context.trace_state = Some(trace_state.into());
        self
    }

    /// Sets the trace flags.
    #[cfg(feature = "trace")]
    pub fn with_trace_flags(mut self, flags: u32) -> Self {
        self.diagnostics_mut().trace.context.flags = Some(flags);
        self
    }

    /// Adds a trace event to the report.
    #[cfg(feature = "trace")]
    pub fn with_trace_event(mut self, event: TraceEvent) -> Self {
        self.diagnostics_mut().trace.events.push(event);
        self
    }

    /// Pushes a trace event with the specified name.
    #[cfg(feature = "trace")]
    pub fn push_trace_event(mut self, name: impl Into<Cow<'static, str>>) -> Self {
        self.diagnostics_mut().trace.events.push(TraceEvent {
            name: name.into(),
            ..TraceEvent::default()
        });
        self
    }

    /// Pushes a trace event with detailed information.
    #[cfg(feature = "trace")]
    pub fn push_trace_event_ext(
        mut self,
        name: impl Into<Cow<'static, str>>,
        level: Option<TraceEventLevel>,
        timestamp_unix_nano: Option<u64>,
        attributes: impl IntoIterator<Item = TraceEventAttribute>,
    ) -> Self {
        self.diagnostics_mut().trace.events.push(TraceEvent {
            name: name.into(),
            level,
            timestamp_unix_nano,
            attributes: attributes.into_iter().collect(),
        });
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
    pub fn with_category(mut self, category: impl Into<Cow<'static, str>>) -> Self {
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
        self.ensure_cold().metadata.stack_trace = Some(stack_trace);
        self
    }

    /// Clears the stack trace from the report.
    pub fn clear_stack_trace(mut self) -> Self {
        self.ensure_cold().metadata.stack_trace = None;
        self
    }

    /// Captures the stack trace for the report if not already present.
    #[cfg(feature = "std")]
    pub fn capture_stack_trace(mut self) -> Self {
        if self.metadata().stack_trace.is_none() {
            self.ensure_cold().metadata.stack_trace = Some(StackTrace::capture_raw());
        }
        self
    }

    /// Forcefully captures the stack trace for the report.
    #[cfg(feature = "std")]
    pub fn force_capture_stack(mut self) -> Self {
        self.ensure_cold().metadata.stack_trace = Some(StackTrace::capture_raw());
        self
    }

    /// Adds a display cause to the report.
    pub fn with_display_cause(mut self, cause: impl Display + 'static) -> Self {
        self.diagnostics_mut().display_causes.push(Box::new(cause));
        self
    }

    /// Adds multiple display causes to the report.
    pub fn with_display_causes<I, T>(mut self, causes: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Display + 'static,
    {
        self.diagnostics_mut().display_causes.extend(
            causes
                .into_iter()
                .map(|cause| Box::new(cause) as Box<dyn Display + 'static>),
        );
        self
    }

    /// Adds an error source to the report's error chain.
    pub fn with_source_error(mut self, err: impl Error + 'static) -> Self {
        self.diagnostics_mut().source_errors.push(Box::new(err));
        self
    }

    /// Wraps the report into another error type.
    pub fn wrap<Outer>(self, outer: Outer) -> Report<Outer>
    where
        Self: Error + 'static,
    {
        let source_errors = alloc::vec![Box::new(self) as Box<dyn Error + 'static>];
        Report {
            inner: outer,
            cold: Some(Box::new(ColdData {
                metadata: ReportMetadata::default(),
                diagnostics: DiagnosticBag {
                    #[cfg(feature = "trace")]
                    trace: ReportTrace::default(),
                    attachments: Vec::new(),
                    display_causes: Vec::new(),
                    source_errors,
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
    pub fn visit_display_causes<F>(&self, visit: F) -> Result<CauseTraversalState, fmt::Error>
    where
        F: FnMut(&dyn Display) -> fmt::Result,
        E: Error + 'static,
    {
        self.visit_display_causes_with(CauseCollectOptions::default(), visit)
    }

    /// Visits display causes using custom collection options.
    pub fn visit_display_causes_with<F>(
        &self,
        options: CauseCollectOptions,
        visit: F,
    ) -> Result<CauseTraversalState, fmt::Error>
    where
        F: FnMut(&dyn Display) -> fmt::Result,
        E: Error + 'static,
    {
        self.for_each_display_cause_with(options, visit)
    }

    /// Visits source errors using default collection options.
    pub fn visit_source_errors<F>(&self, visit: F) -> Result<CauseTraversalState, fmt::Error>
    where
        F: FnMut(&dyn Error) -> fmt::Result,
        E: Error + 'static,
    {
        self.visit_source_errors_with(CauseCollectOptions::default(), visit)
    }

    /// Visits source errors using custom collection options.
    pub fn visit_source_errors_with<F>(
        &self,
        options: CauseCollectOptions,
        visit: F,
    ) -> Result<CauseTraversalState, fmt::Error>
    where
        F: FnMut(&dyn Error) -> fmt::Result,
        E: Error + 'static,
    {
        self.for_each_source_error_with(options, visit)
    }
}

impl<E> Report<E>
where
    E: Error + 'static,
{
    /// Iterates source errors using default collection options.
    pub fn iter_source_errors(&self) -> ReportSourceErrorIter<'_> {
        self.iter_source_errors_with(CauseCollectOptions::default())
    }

    /// Iterates source errors using custom collection options.
    pub fn iter_source_errors_with(
        &self,
        options: CauseCollectOptions,
    ) -> ReportSourceErrorIter<'_> {
        let source_errors = self
            .diagnostics()
            .map(|diag| diag.source_errors.as_slice())
            .unwrap_or(&[]);

        ReportSourceErrorIter {
            source_errors: source_errors.iter(),
            root_source: self.inner.source(),
            current: None,
            stage: SourceErrorIterStage::Attached,
            depth: 0,
            options,
            seen: SeenErrorAddrs::new(),
            state: CauseTraversalState::default(),
        }
    }

    pub fn for_each_display_cause_with<F>(
        &self,
        options: CauseCollectOptions,
        mut visit: F,
    ) -> Result<CauseTraversalState, fmt::Error>
    where
        F: FnMut(&dyn Display) -> fmt::Result,
    {
        let mut state = CauseTraversalState::default();
        let mut depth = 0usize;
        let Some(diag) = self.diagnostics() else {
            return Ok(state);
        };

        for cause in &diag.display_causes {
            if depth >= options.max_depth {
                state.truncated = true;
                break;
            }
            visit(cause.as_ref())?;
            depth += 1;
        }

        Ok(state)
    }

    pub fn for_each_source_error_with<F>(
        &self,
        options: CauseCollectOptions,
        mut visit: F,
    ) -> Result<CauseTraversalState, fmt::Error>
    where
        F: FnMut(&dyn Error) -> fmt::Result,
    {
        let mut iter = self.iter_source_errors_with(options);
        for err in iter.by_ref() {
            visit(err)?;
        }
        Ok(iter.state())
    }
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
            return true;
        }
        self.spill.push(addr);
        true
    }

    fn contains(&self, addr: usize) -> bool {
        if self.inline[..self.len].contains(&addr) {
            return true;
        }
        self.spill.contains(&addr)
    }
}
