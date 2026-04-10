//! Accessor methods for Report.
//!
//! This module contains all accessor and visitor methods for Report instances.
//! These methods provide read-only access to the report's data and support
//! iteration over various components like attachments, causes, and source errors.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::error::Error;
use core::fmt::{self, Display};

#[cfg(feature = "json")]
use super::types::DisplayCauseChain;
use super::types::{Attachment, AttachmentVisit, StackTrace};
use super::{
    CauseCollectOptions, CauseTraversalState, ContextMap, Report, ReportMetadata, ReportOptions,
    ReportSourceErrorIter, SeverityState, SourceErrorChain, SourceErrorEntry,
};

impl<E, State> Report<E, State>
where
    State: SeverityState,
{
    /// Returns a reference to the inner error.
    ///
    /// The inner error is the wrapped error type that this report contains.
    ///
    /// # Example
    ///
    /// ```rust
    /// use diagweave::prelude::Report;
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("my error")]
    /// struct MyError;
    ///
    /// let report = Report::new(MyError);
    /// let inner: &MyError = report.inner();
    /// ```
    pub fn inner(&self) -> &E {
        &self.inner
    }

    /// Consumes the report and returns the inner error.
    ///
    /// This method consumes the report and returns the wrapped error type,
    /// discarding all diagnostic information.
    ///
    /// # Example
    ///
    /// ```rust
    /// use diagweave::prelude::Report;
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("my error")]
    /// struct MyError;
    ///
    /// let report = Report::new(MyError);
    /// let inner: MyError = report.into_inner();
    /// ```
    pub fn into_inner(self) -> E {
        self.inner
    }

    /// Returns the attachments associated with the report.
    ///
    /// Attachments include notes and payloads that provide additional
    /// context about the error.
    ///
    /// # Example
    ///
    /// ```rust
    /// use diagweave::prelude::Report;
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("my error")]
    /// struct MyError;
    ///
    /// let report = Report::new(MyError)
    ///     .attach_note("Additional context");
    /// let attachments = report.attachments();
    /// assert!(!attachments.is_empty());
    /// ```
    pub fn attachments(&self) -> &[Attachment] {
        self.diagnostics().attachments()
    }

    /// Returns context key-value pairs associated with the report.
    ///
    /// Returns a reference to an empty [`ContextMap`] if no context has been set.
    ///
    /// # Example
    ///
    /// ```rust
    /// use diagweave::prelude::Report;
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("my error")]
    /// struct MyError;
    ///
    /// let report = Report::new(MyError)
    ///     .with_ctx("user_id", "12345");
    /// let context = report.context();
    /// assert!(context.contains_key("user_id"));
    /// ```
    pub fn context(&self) -> &ContextMap {
        self.diagnostics().context()
    }

    /// Returns system context associated with the report.
    ///
    /// Returns a reference to an empty [`ContextMap`] if no system context has been set.
    ///
    /// # Example
    ///
    /// ```rust
    /// use diagweave::prelude::Report;
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("my error")]
    /// struct MyError;
    ///
    /// let report = Report::new(MyError)
    ///     .with_system("hostname", "server-01");
    /// let system = report.system();
    /// assert!(system.contains_key("hostname"));
    /// ```
    pub fn system(&self) -> &ContextMap {
        self.diagnostics().system()
    }

    /// Visits attachments in insertion order without building intermediate allocations.
    ///
    /// This method provides a zero-allocation way to iterate over attachments
    /// by calling a visitor function for each attachment.
    ///
    /// # Example
    ///
    /// ```rust
    /// use diagweave::prelude::Report;
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("my error")]
    /// struct MyError;
    ///
    ///
    /// let report = Report::new(MyError)
    ///     .attach_note("Note 1")
    ///     .attach_note("Note 2");
    ///
    /// report.visit_attachments(|visit| {
    ///     match visit {
    ///         diagweave::report::AttachmentVisit::Note { message } => {
    ///             println!("Note: {}", message);
    ///         }
    ///         diagweave::report::AttachmentVisit::Payload { name, value, media_type } => {
    ///             println!("Payload: {} ({:?})", name, media_type);
    ///         }
    ///     }
    ///     Ok(())
    /// }).unwrap();
    /// ```
    pub fn visit_attachments<F>(&self, mut visit: F) -> Result<(), fmt::Error>
    where
        F: FnMut(AttachmentVisit<'_>) -> fmt::Result,
    {
        for attachment in self.diagnostics().attachments() {
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
    ///
    /// Display causes are human-readable messages that provide additional
    /// context about the error.
    ///
    /// # Example
    ///
    /// ```rust
    /// use diagweave::prelude::Report;
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("my error")]
    /// struct MyError;
    ///
    /// let report = Report::new(MyError)
    ///     .with_display_cause("Failed to connect");
    /// let causes = report.display_causes();
    /// assert!(!causes.is_empty());
    /// ```
    pub fn display_causes(&self) -> &[Arc<dyn Display + Send + Sync + 'static>] {
        self.diagnostics()
            .display_causes()
            .map(|v| v.items.as_slice())
            .unwrap_or(&[])
    }

    /// Returns the display-cause chain associated with the report, if any.
    #[cfg(feature = "json")]
    pub(crate) fn display_causes_chain(&self) -> Option<&DisplayCauseChain> {
        self.diagnostics().display_causes()
    }

    /// Returns source errors from the origin chain associated with the report.
    ///
    /// Origin source errors represent the chain of errors that caused
    /// the main error. This method returns an iterator over the entries.
    ///
    /// # Example
    ///
    /// ```rust
    /// use diagweave::prelude::Report;
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("inner error")]
    /// struct InnerError;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("outer error")]
    /// struct OuterError;
    ///
    /// impl From<InnerError> for OuterError {
    ///     fn from(e: InnerError) -> Self { OuterError }
    /// }
    ///
    /// let inner = Report::new(InnerError);
    /// let outer: Report<OuterError> = inner.map_err(|_| OuterError);
    /// let sources: Vec<_> = outer.origin_source_errors().collect();
    /// ```
    pub fn origin_source_errors(&self) -> impl Iterator<Item = SourceErrorEntry> + '_
    where
        E: Error + 'static,
    {
        self.origin_src_err_view(self.options().as_cause_options())
            .map(|chain| chain.iter_entries_origin().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
    }

    /// Returns source errors from the diagnostic chain associated with the report.
    ///
    /// Diagnostic source errors are separate from the origin chain and represent
    /// additional errors that are related to but not directly caused by the main error.
    ///
    /// # Example
    ///
    /// ```rust
    /// use diagweave::prelude::Report;
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("main error")]
    /// struct MainError;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("related error")]
    /// struct RelatedError;
    ///
    /// let report = Report::new(MainError)
    ///     .with_diag_src_err(RelatedError);
    /// let sources: Vec<_> = report.diag_source_errors().collect();
    /// ```
    pub fn diag_source_errors(&self) -> impl Iterator<Item = SourceErrorEntry> + '_
    where
        E: Error + 'static,
    {
        self.diagnostics()
            .diagnostic_source_errors()
            .map(SourceErrorChain::iter_entries)
            .into_iter()
            .flatten()
    }

    /// Returns the origin source-error chain associated with the report, if any.
    #[cfg(feature = "json")]
    pub(crate) fn origin_src_err_chain(&self) -> Option<&SourceErrorChain> {
        self.diagnostics().origin_source_errors()
    }

    /// Returns the diagnostic source-error chain associated with the report, if any.
    #[cfg(feature = "json")]
    pub(crate) fn diag_src_err_chain(&self) -> Option<&SourceErrorChain> {
        self.diagnostics().diagnostic_source_errors()
    }

    /// Returns the metadata associated with the report.
    ///
    /// Metadata contains error code, category, and retryable flag.
    ///
    /// # Example
    ///
    /// ```rust
    /// use diagweave::prelude::Report;
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("my error")]
    /// struct MyError;
    ///
    /// let report = Report::new(MyError)
    ///     .set_error_code("ERR-001");
    /// let metadata = report.metadata();
    /// ```
    pub fn metadata(&self) -> &ReportMetadata<State> {
        self.metadata_ref()
    }

    /// Returns the error code from report metadata, if present.
    ///
    /// # Example
    ///
    /// ```rust
    /// use diagweave::prelude::Report;
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("my error")]
    /// struct MyError;
    ///
    /// let report = Report::new(MyError)
    ///     .set_error_code("ERR-001");
    /// assert_eq!(report.error_code().unwrap().to_string(), "ERR-001".to_string());
    /// ```
    pub fn error_code(&self) -> Option<&super::ErrorCode> {
        self.metadata.error_code()
    }

    /// Returns the severity from report typestate.
    ///
    /// # Example
    ///
    /// ```rust
    /// use diagweave::prelude::{Report, Severity};
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("my error")]
    /// struct MyError;
    ///
    /// let report = Report::new(MyError)
    ///     .set_severity(Severity::Error);
    /// assert_eq!(report.severity(), Some(Severity::Error));
    /// ```
    pub fn severity(&self) -> Option<super::Severity> {
        self.metadata.severity()
    }

    /// Returns the severity state from report typestate.
    pub(crate) fn severity_state(&self) -> State {
        self.metadata.severity_state()
    }

    /// Returns the category from report metadata, if present.
    ///
    /// # Example
    ///
    /// ```rust
    /// use diagweave::prelude::Report;
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("my error")]
    /// struct MyError;
    ///
    /// let report = Report::new(MyError)
    ///     .set_category("payment");
    /// assert_eq!(report.category(), Some("payment"));
    /// ```
    pub fn category(&self) -> Option<&str> {
        self.metadata.category()
    }

    /// Returns whether the report is marked retryable, if present.
    ///
    /// # Example
    ///
    /// ```rust
    /// use diagweave::prelude::Report;
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("my error")]
    /// struct MyError;
    ///
    /// let report = Report::new(MyError)
    ///     .set_retryable(true);
    /// assert_eq!(report.retryable(), Some(true));
    /// ```
    pub fn retryable(&self) -> Option<bool> {
        self.metadata.retryable()
    }

    /// Returns the stack trace associated with the report, if any.
    ///
    /// # Example
    ///
    /// ```rust
    /// # #[cfg(feature = "std")]
    /// # {
    /// use diagweave::prelude::Report;
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("my error")]
    /// struct MyError;
    ///
    /// let report = Report::new(MyError)
    ///     .capture_stack_trace();
    /// let stack = report.stack_trace();
    /// assert!(stack.is_some());
    /// # }
    /// ```
    pub fn stack_trace(&self) -> Option<&StackTrace> {
        self.diagnostics().stack_trace()
    }

    /// Returns the current report options.
    ///
    /// Report options control behavior like source chain accumulation
    /// and cycle detection.
    ///
    /// # Example
    ///
    /// ```rust
    /// use diagweave::prelude::Report;
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("my error")]
    /// struct MyError;
    ///
    /// let report = Report::new(MyError);
    /// let options = report.options();
    /// ```
    pub fn options(&self) -> &ReportOptions {
        &self.options
    }

    /// Visits display causes using default collection options.
    ///
    /// This method iterates over all display causes and calls the provided
    /// visitor function for each one.
    ///
    /// # Example
    ///
    /// ```rust
    /// use diagweave::prelude::Report;
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("my error")]
    /// struct MyError;
    ///
    /// let report = Report::new(MyError)
    ///     .with_display_cause("Cause 1")
    ///     .with_display_cause("Cause 2");
    ///
    /// let state = report.visit_causes(|cause| {
    ///     println!("Cause: {}", cause);
    ///     Ok(())
    /// }).unwrap();
    /// ```
    pub fn visit_causes<F>(&self, visit: F) -> Result<CauseTraversalState, fmt::Error>
    where
        F: FnMut(&dyn Display) -> fmt::Result,
        E: Error + 'static,
    {
        self.visit_causes_ext(self.options().as_cause_options(), visit)
    }

    /// Visits display causes using custom collection options.
    ///
    /// This method allows customizing the traversal behavior with
    /// [`CauseCollectOptions`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use diagweave::prelude::Report;
    /// use diagweave::report::CauseCollectOptions;
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("my error")]
    /// struct MyError;
    ///
    /// let report = Report::new(MyError)
    ///     .with_display_cause("Cause 1")
    ///     .with_display_cause("Cause 2");
    ///
    /// let options = CauseCollectOptions::new().with_max_depth(1);
    /// let state = report.visit_causes_ext(options, |cause| {
    ///     println!("Cause: {}", cause);
    ///     Ok(())
    /// }).unwrap();
    /// assert!(state.truncated);
    /// ```
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
        let diag = self.diagnostics();
        let Some(display_causes) = diag.display_causes() else {
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
    ///
    /// This method iterates over all origin source errors and calls the
    /// provided visitor function for each one.
    ///
    /// # Example
    ///
    /// ```rust
    /// use diagweave::prelude::Report;
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("inner error")]
    /// struct InnerError;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("outer error")]
    /// struct OuterError;
    ///
    /// impl From<InnerError> for OuterError {
    ///     fn from(e: InnerError) -> Self { OuterError }
    /// }
    ///
    /// let inner = Report::new(InnerError);
    /// let outer: Report<OuterError> = inner.map_err(|_| OuterError);
    ///
    /// let state = outer.visit_origin_sources(|entry| {
    ///     println!("Source: {}", entry.message);
    ///     Ok(())
    /// }).unwrap();
    /// ```
    pub fn visit_origin_sources<F>(&self, visit: F) -> Result<CauseTraversalState, fmt::Error>
    where
        F: FnMut(SourceErrorEntry) -> fmt::Result,
        E: Error + 'static,
    {
        self.visit_origin_src_ext(self.options().as_cause_options(), visit)
    }

    /// Visits origin source errors using custom collection options.
    ///
    /// This method allows customizing the traversal behavior with
    /// [`CauseCollectOptions`].
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
    ///
    /// This method iterates over all diagnostic source errors and calls the
    /// provided visitor function for each one.
    pub fn visit_diag_sources<F>(&self, visit: F) -> Result<CauseTraversalState, fmt::Error>
    where
        F: FnMut(SourceErrorEntry) -> fmt::Result,
        E: Error + 'static,
    {
        self.visit_diag_srcs_ext(self.options().as_cause_options(), visit)
    }

    /// Visits diagnostic source errors using custom collection options.
    ///
    /// This method allows customizing the traversal behavior with
    /// [`CauseCollectOptions`].
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

    /// Returns an iterator over origin source errors with custom options.
    pub fn iter_origin_src_ext(&self, options: CauseCollectOptions) -> ReportSourceErrorIter<'_>
    where
        E: Error + 'static,
    {
        ReportSourceErrorIter::new_origin(self, options)
    }

    /// Returns an iterator over diagnostic source errors with custom options.
    pub fn iter_diag_srcs_ext(&self, options: CauseCollectOptions) -> ReportSourceErrorIter<'_>
    where
        E: Error + 'static,
    {
        ReportSourceErrorIter::new_diagnostic(self, options)
    }

    /// Iterates origin source errors using default collection options.
    ///
    /// This method returns an iterator over all origin source errors
    /// associated with the report.
    ///
    /// # Example
    ///
    /// ```rust
    /// use diagweave::prelude::Report;
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("inner error")]
    /// struct InnerError;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("outer error")]
    /// struct OuterError;
    ///
    /// impl From<InnerError> for OuterError {
    ///     fn from(e: InnerError) -> Self { OuterError }
    /// }
    ///
    /// let inner = Report::new(InnerError);
    /// let outer: Report<OuterError> = inner.map_err(|_| OuterError);
    /// for entry in outer.iter_origin_sources() {
    ///     println!("Source: {}", entry.message);
    /// }
    /// ```
    pub fn iter_origin_sources(&self) -> ReportSourceErrorIter<'_>
    where
        E: Error + 'static,
    {
        self.iter_origin_src_ext(self.options().as_cause_options())
    }

    /// Iterates diagnostic source errors using default collection options.
    ///
    /// This method returns an iterator over all diagnostic source errors
    /// associated with the report.
    ///
    /// # Example
    ///
    /// ```rust
    /// use diagweave::prelude::Report;
    /// use diagweave::Error;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("main error")]
    /// struct MainError;
    ///
    /// #[derive(Debug, Error)]
    /// #[display("related error")]
    /// struct RelatedError;
    ///
    /// let report = Report::new(MainError)
    ///     .with_diag_src_err(RelatedError);
    /// for entry in report.iter_diag_sources() {
    ///     println!("Related: {}", entry.message);
    /// }
    /// ```
    pub fn iter_diag_sources(&self) -> ReportSourceErrorIter<'_>
    where
        E: Error + 'static,
    {
        self.iter_diag_srcs_ext(self.options().as_cause_options())
    }
}
