//! Report module - core diagnostic report types and operations.
//!
//! This module provides the main [`Report`] type which wraps errors with rich
//! metadata and context. The module is organized into several submodules:
//!
//! - [`builder`] - Builder-style methods for constructing reports
//! - [`accessors`] - Accessor and visitor methods for reading report data
//! - [`global`] - Global context injection utilities
//! - [`transform`] - Error transformation methods like `map_err`
//! - [`types`] - Core type definitions
//! - [`ext`] - Extension traits for working with reports
//! - [`impls`] - Core trait implementations
//!
//! # Example
//!
//! ```rust
//! use diagweave::prelude::{Report, Severity};
//! use diagweave::Error;
//!
//! #[derive(Debug, Error)]
//! #[display("database connection failed")]
//! struct DatabaseError;
//!
//! let report = Report::new(DatabaseError)
//!     .set_severity(Severity::Error)
//!     .set_error_code("DB-001")
//!     .attach_note("Failed to connect to production database")
//!     .with_ctx("host", "db.example.com")
//!     .with_ctx("port", "5432");
//! ```

#[path = "report/accessors.rs"]
mod accessors;
#[path = "report/builder.rs"]
mod builder;
#[path = "report/ext.rs"]
mod ext;
#[path = "report/global.rs"]
mod global;
#[path = "report/impls.rs"]
mod impls;
#[cfg(feature = "trace")]
#[path = "report/trace.rs"]
mod trace;
#[path = "report/transform.rs"]
mod transform;
#[path = "report/types.rs"]
mod types;

use alloc::boxed::Box;
use core::error::Error;

pub use ext::{Diagnostic, InspectReportExt, ResultReportExt};
pub use types::{
    Attachment, AttachmentValue, CauseCollectOptions, CauseKind, ContextMap, ContextValue,
    DisplayCauseChain, ErrorCode, ErrorCodeIntError, GlobalErrorMeta, HasSeverity, MissingSeverity,
    ReportMetadata, ReportOptions, Severity, SeverityParseError, SeverityState, SourceErrorChain,
    SourceErrorEntry, SourceErrorItem, StackFrame, StackTrace, StackTraceFormat,
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

#[cfg(feature = "std")]
pub use global::RegisterGlobalContextError;
#[cfg(feature = "std")]
pub use global::register_global_injector;
#[cfg(feature = "std")]
pub use types::{GlobalConfig, SetGlobalConfigError, set_global_config};

use types::{ColdData, DiagnosticBag, append_source_chain, limit_depth_source_chain};

/// A high-level diagnostic report that wraps an error with rich metadata and context.
///
/// `Report` provides a comprehensive wrapper around error types, adding:
/// - **Attachments**: Notes and payloads for additional context
/// - **Context**: Key-value pairs for business and system context
/// - **Metadata**: Error code, category, and retryable flag
/// - **Severity**: Error severity level (via typestate pattern)
/// - **Stack traces**: Captured call stack information
/// - **Display causes**: Human-readable cause chain
/// - **Source errors**: Technical error chain for debugging
///
/// # Typestate Pattern
///
/// `Report` uses a typestate pattern for severity:
/// - `Report<E, MissingSeverity>` - Severity not yet set
/// - `Report<E, HasSeverity>` - Severity has been set
///
/// This ensures type safety when severity is required for certain operations.
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
/// // Create a report without severity
/// let report: Report<MyError, _> = Report::new(MyError);
///
/// // Set severity to get HasSeverity typestate
/// let report = report.set_severity(Severity::Error);
/// ```
pub struct Report<E, State: SeverityState = MissingSeverity> {
    inner: E,
    metadata: ReportMetadata<State>,
    cold: Option<Box<ColdData>>,
}

impl<E, State> Report<E, State>
where
    State: SeverityState,
{
    /// Returns a reference to the internal diagnostics bag.
    fn diagnostics(&self) -> Option<&DiagnosticBag> {
        self.cold.as_deref().map(|cold| &cold.bag)
    }

    /// Ensures cold data is allocated, creating it if necessary.
    fn ensure_cold(&mut self) -> &mut ColdData {
        self.cold
            .get_or_insert_with(|| Box::new(ColdData::default()))
    }

    /// Returns a mutable reference to the diagnostics bag.
    fn diagnostics_mut(&mut self) -> &mut DiagnosticBag {
        &mut self.ensure_cold().bag
    }

    /// Returns a reference to the metadata.
    fn metadata_ref(&self) -> &ReportMetadata<State> {
        &self.metadata
    }
}

impl<E, State> Report<E, State>
where
    E: Error + 'static,
    State: SeverityState,
{
    /// Builds a source error chain view based on stored errors and inner source.
    fn source_errors_view(
        &self,
        stored: Option<&SourceErrorChain>,
        include_inner_source: bool,
        options: CauseCollectOptions,
    ) -> Option<SourceErrorChain> {
        let mut snapshot = stored.cloned();

        if include_inner_source && let Some(source) = self.inner.source() {
            let source_chain = SourceErrorChain::from_source(source, options);
            match snapshot.as_mut() {
                Some(existing) => append_source_chain(existing, source_chain),
                None => snapshot = Some(source_chain),
            }
        }

        let mut snapshot = snapshot?;
        limit_depth_source_chain(&mut snapshot, options, 0);
        if !options.detect_cycle {
            snapshot.clear_cycle_flags();
        }
        Some(snapshot)
    }

    /// Returns the origin source error chain view for rendering.
    pub(crate) fn origin_src_err_view(
        &self,
        options: CauseCollectOptions,
    ) -> Option<SourceErrorChain> {
        self.source_errors_view(
            self.diagnostics()
                .and_then(|diag| diag.origin_source_errors.as_ref()),
            true,
            options,
        )
    }

    /// Returns the diagnostic source error chain view for rendering.
    pub(crate) fn diag_src_err_view(
        &self,
        options: CauseCollectOptions,
    ) -> Option<SourceErrorChain> {
        self.source_errors_view(
            self.diagnostics()
                .and_then(|diag| diag.diagnostic_source_errors.as_ref()),
            false,
            options,
        )
    }
}

impl<E> Report<E, MissingSeverity> {
    /// Sets the severity for the report, transitioning to `HasSeverity` typestate.
    ///
    /// This method consumes the report and returns a new one with the severity
    /// set. This is the primary way to transition from `MissingSeverity` to
    /// `HasSeverity`.
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
    /// ```
    pub fn set_severity(self, severity: Severity) -> Report<E, HasSeverity> {
        let Self {
            inner,
            metadata,
            cold,
        } = self;
        Report {
            inner,
            metadata: metadata.set_severity(severity),
            cold,
        }
    }
}

impl<E> Report<E, HasSeverity> {
    /// Replaces the severity with a new value.
    ///
    /// This method is available on reports that already have a severity set.
    /// It replaces the existing severity with a new one.
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
    ///     .set_severity(Severity::Warn);
    /// let report = report.replace_severity(Severity::Error); // Replace severity
    /// assert_eq!(report.severity(), Some(Severity::Error));
    /// ```
    pub fn replace_severity(self, severity: Severity) -> Report<E, HasSeverity> {
        let Self {
            inner,
            metadata,
            cold,
        } = self;
        Report {
            inner,
            metadata: metadata.replace_severity(severity),
            cold,
        }
    }

    /// Sets the severity to a new value (alias for `replace_severity`).
    ///
    /// This method is provided for API consistency, allowing `set_severity`
    /// to be called on both `MissingSeverity` and `HasSeverity` typestates.
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
    ///     .set_severity(Severity::Warn);
    /// let report = report.set_severity(Severity::Error); // Replace severity
    /// assert_eq!(report.severity(), Some(Severity::Error));
    /// ```
    pub fn set_severity(self, severity: Severity) -> Report<E, HasSeverity> {
        self.replace_severity(severity)
    }
}
