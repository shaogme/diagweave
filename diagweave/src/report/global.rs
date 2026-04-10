//! Global context injection utilities for Report.
//!
//! This module provides the infrastructure for injecting global context
//! into newly created reports, enabling automatic population of metadata
//! and contextual information from application-wide configuration.

#[cfg(feature = "std")]
use std::sync::OnceLock;

use alloc::boxed::Box;

#[cfg(feature = "trace")]
use super::trace::ReportTrace;
use super::types::GlobalContext;
use super::{Report, ReportMetadata, ReportOptions, SeverityState};

/// Context injector type alias for global context providers.
///
/// This function type is used to inject global context into new reports.
/// The function should return `Some(GlobalContext)` if there is context
/// to inject, or `None` if no global context is available.
#[cfg(feature = "std")]
pub(crate) type ContextInjector = dyn Fn() -> Option<GlobalContext> + Send + Sync + 'static;

/// Returns the global context injector singleton.
///
/// The injector is stored in a static `OnceLock` and can be set once
/// using [`register_global_injector`].
#[cfg(feature = "std")]
pub(crate) fn global_context_injector() -> &'static OnceLock<Box<ContextInjector>> {
    static INJECTOR: OnceLock<Box<ContextInjector>> = OnceLock::new();
    &INJECTOR
}

/// Error returned when global context registration fails.
///
/// This error is returned when attempting to register a global context
/// injector after one has already been registered. Only one injector
/// can be registered per process.
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisterGlobalContextError;

/// Registers a global context injector that will be invoked for every new report.
///
/// The injector function is called during `Report::new()` when the `std` feature
/// is enabled. It allows automatic injection of:
/// - Error metadata (error code, category, retryable)
/// - System context (hostname, service name, etc.)
/// - Business context (user ID, request ID, etc.)
/// - Trace context (when `trace` feature is enabled)
///
/// # Errors
///
/// Returns `RegisterGlobalContextError` if an injector has already been registered.
/// Only one injector can be registered per process lifetime.
///
/// # Thread Safety
///
/// The injector is stored in a thread-safe `OnceLock` and can be safely
/// called from multiple threads. The injector function itself must be
/// `Send + Sync + 'static`.
///
/// # Example
///
/// ```rust
/// use diagweave::report::{register_global_injector, GlobalContext, GlobalErrorMeta};
///
/// // Register a global injector that adds error code to all reports
/// let result = register_global_injector(|| {
///     let mut ctx = GlobalContext::default();
///     ctx.error = Some(GlobalErrorMeta {
///         error_code: Some("ERR-001".into()),
///         category: Some("system".into()),
///         retryable: Some(true),
///     });
///     Some(ctx)
/// });
///
/// // Note: This will fail if an injector is already registered
/// // assert!(result.is_ok());
/// ```
///
/// # Integration with Tracing
///
/// When the `trace` feature is enabled, the injector can also provide
/// trace context for distributed tracing:
///
/// ```rust
/// # #[cfg(feature = "trace")]
/// # {
/// use diagweave::report::{register_global_injector, GlobalContext, GlobalTraceContext};
///
/// let _ = register_global_injector(|| {
///     let mut ctx = GlobalContext::default();
///     ctx.trace = Some(GlobalTraceContext {
///         trace_id: Some("4bf92f3577b34da6a3ce929d0e0e4736".try_into().unwrap()),
///         span_id: Some("00f067aa0ba902b7".try_into().unwrap()),
///         parent_span_id: None,
///         sampled: Some(true),
///         trace_state: None,
///         flags: None,
///     });
///     Some(ctx)
/// });
/// # }
/// ```
#[cfg(feature = "std")]
pub fn register_global_injector(
    injector: impl Fn() -> Option<GlobalContext> + Send + Sync + 'static,
) -> Result<(), RegisterGlobalContextError> {
    global_context_injector()
        .set(Box::new(injector))
        .map_err(|_| RegisterGlobalContextError)
}

impl<E, State> Report<E, State>
where
    State: SeverityState,
{
    /// Applies global context to the report during construction.
    ///
    /// This method is called automatically during `Report::new()` when
    /// the `std` feature is enabled. It invokes the registered global
    /// context injector (if any) and populates the report with the
    /// returned context.
    ///
    /// # Lazy Allocation
    ///
    /// This method uses lazy allocation optimization - it only allocates
    /// `ColdData` if the global context actually contains data to inject.
    /// If the injector returns `None` or an empty context, no allocation
    /// occurs.
    ///
    /// # Panic Safety
    ///
    /// The method uses `catch_unwind` to handle panics in the injector
    /// function. If the injector panics, it is treated as if it returned
    /// `None`, and the report is created without global context.
    ///
    /// # Implementation Details
    ///
    /// The method performs the following steps:
    /// 1. Check if a global injector is registered
    /// 2. Call the injector with panic protection
    /// 3. Check if any data needs to be injected
    /// 4. Allocate `ColdData` only if necessary
    /// 5. Populate the report with global context
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
        } = &global;

        // Check if we actually need to allocate ColdData
        // This is the lazy initialization optimization - only allocate when there's real data
        let needs_allocation = {
            let has_error_meta = error
                .as_ref()
                .map(|e| e.error_code.is_some() || e.category.is_some() || e.retryable.is_some())
                .unwrap_or(false);
            let has_system = !system.is_empty();
            let has_context = !context.is_empty();

            #[cfg(feature = "trace")]
            let has_trace = trace.is_some();
            #[cfg(not(feature = "trace"))]
            let has_trace = false;

            has_error_meta || has_system || has_context || has_trace
        };

        if !needs_allocation {
            return;
        }

        // Handle error metadata
        if let Some(error) = global.error {
            if let Some(error_code) = error.error_code {
                self.metadata.with_error_code_mut(error_code);
            }
            if let Some(category) = error.category {
                self.metadata.with_category_mut(category);
            }
            if let Some(retryable) = error.retryable {
                self.metadata.with_retryable_mut(retryable);
            }
        }

        // Handle system and context
        if !global.system.is_empty() {
            *self.bag.system_mut() = global.system;
        }
        if !global.context.is_empty() {
            for (key, value) in &global.context {
                self.bag.insert_context(key.clone(), value.clone());
            }
        }

        #[cfg(feature = "trace")]
        if let Some(global_trace) = global.trace {
            let trace = core::mem::take(&mut self.trace);
            self.trace = trace
                .set_trace_id_opt(global_trace.trace_id)
                .set_span_id_opt(global_trace.span_id)
                .set_parent_span_id_opt(global_trace.parent_span_id)
                .set_sampled_opt(global_trace.sampled)
                .set_trace_state_opt(global_trace.trace_state)
                .set_flags_opt(global_trace.flags);
        }
    }
}

impl<E> Report<E, crate::report::MissingSeverity> {
    /// Creates a new report with global context applied.
    ///
    /// This constructor creates a new report wrapping the provided error.
    /// When the `std` feature is enabled, it also applies any registered
    /// global context to the report.
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
    /// ```
    ///
    /// # Global Context
    ///
    /// When using the `std` feature, `Report::new()` automatically invokes
    /// any registered global context injector. This allows automatic
    /// population of:
    /// - Error metadata from application configuration
    /// - System context from environment
    /// - Trace context from distributed tracing systems
    ///
    /// See [`register_global_injector`] for how to register a global context provider.
    pub fn new(inner: E) -> Self {
        #[cfg(feature = "std")]
        let mut report = Self {
            inner,
            metadata: ReportMetadata::new(),
            options: ReportOptions::new(),
            #[cfg(feature = "trace")]
            trace: ReportTrace::default(),
            bag: super::DiagnosticBag::new(),
        };
        #[cfg(not(feature = "std"))]
        let report = Self {
            inner,
            metadata: ReportMetadata::new(),
            options: ReportOptions::new(),
            #[cfg(feature = "trace")]
            trace: ReportTrace::default(),
            bag: super::DiagnosticBag::new(),
        };
        #[cfg(feature = "std")]
        report.apply_global_context();
        report
    }

    /// Sets the severity for the report.
    ///
    /// This is an alias for `set_severity()` for convenience when starting
    /// from a `Report<E, MissingSeverity>`. It consumes the report and
    /// returns a new report with the severity typestate changed to
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
    ///     .with_severity(Severity::Error);
    /// ```
    pub fn with_severity(self, severity: super::Severity) -> Report<E, super::HasSeverity> {
        let Self {
            inner,
            metadata,
            options,
            #[cfg(feature = "trace")]
            trace,
            bag,
        } = self;
        Report {
            inner,
            metadata: metadata.set_severity(severity),
            options,
            #[cfg(feature = "trace")]
            trace,
            bag,
        }
    }
}
