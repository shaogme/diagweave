//! Error transformation methods for Report.
//!
//! This module contains the `map_err` method and related functionality
//! for transforming the inner error type while preserving diagnostic data.

use alloc::boxed::Box;
use core::error::Error;

use super::types::build_origin_source_chain;
use super::{ColdData, DiagnosticBag, Report, SeverityState, SourceErrorChain};

impl<E, State> Report<E, State>
where
    State: SeverityState,
{
    /// Maps the inner error type while preserving all diagnostic data.
    ///
    /// When source chain accumulation is enabled via [`ReportOptions::accumulate_source_chain`],
    /// this method also accumulates the origin source error chain.
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
    ///     fn from(e: InnerError) -> Self {
    ///         OuterError
    ///     }
    /// }
    ///
    /// let inner_error = InnerError;
    /// let inner_report = Report::new(inner_error);
    ///
    /// // Transform error type while preserving diagnostics
    /// let report: Report<OuterError> = inner_report.map_err(|e| OuterError::from(e));
    ///
    /// // Control source chain accumulation per-report
    /// let _report = report.set_accumulate_source_chain(false); // Disable accumulation
    /// ```
    ///
    /// # Preserved Data
    ///
    /// The following diagnostic data is preserved during transformation:
    /// - Attachments (notes and payloads)
    /// - Context (business and system)
    /// - Display causes
    /// - Stack traces
    /// - Metadata (error code, category, retryable)
    /// - Diagnostic source errors
    /// - Trace context (when `trace` feature is enabled)
    ///
    /// # Performance
    ///
    /// When source chain accumulation is disabled (the default), this method
    /// performs minimal work - it simply transforms the error type and moves
    /// the diagnostic data. When accumulation is enabled, there is additional
    /// overhead for building and preserving the source chain.
    ///
    /// # Type Constraints
    ///
    /// Both the input error type `E` and output error type `Outer` must implement:
    /// - `Error` - The core error trait
    /// - `Send + Sync` - Required for thread safety
    /// - `'static` - Required for storing in the report
    pub fn map_err<Outer>(self, map: impl FnOnce(E) -> Outer) -> Report<Outer, State>
    where
        E: Error + Send + Sync + 'static,
        Outer: Error + Send + Sync + 'static,
    {
        let Self {
            inner,
            metadata,
            report,
            #[cfg(feature = "trace")]
            trace,
            cold,
        } = self;

        // Check if source chain accumulation is enabled for this report
        if report.resolve_accumulate_source_chain() {
            // Build origin source chain with the old inner as the new root
            let origin_source_errors = build_origin_source_chain(&inner, cold.as_deref());

            // Now create the outer error
            let outer = map(inner);

            // Build new cold data with the origin source chain
            let new_cold = Self::build_cold_with_origin_chain(cold, origin_source_errors);

            Report {
                inner: outer,
                metadata,
                report,
                #[cfg(feature = "trace")]
                trace,
                cold: new_cold,
            }
        } else {
            // Simple transformation without source chain accumulation
            let outer = map(inner);
            Report {
                inner: outer,
                metadata,
                report,
                #[cfg(feature = "trace")]
                trace,
                cold,
            }
        }
    }

    /// Builds cold data with origin source chain for map_err operations.
    ///
    /// This function handles the cold data construction logic:
    /// - If original cold exists, preserve all diagnostic data and update origin_source_errors
    /// - If no original cold, create new cold data with just the origin source chain
    ///
    /// # Parameters
    ///
    /// - `cold`: The original cold data, if any
    /// - `origin_source_errors`: The source chain to attach to the new report
    ///
    /// # Returns
    ///
    /// Returns `Some(Box<ColdData>)` with the constructed cold data.
    fn build_cold_with_origin_chain(
        cold: Option<Box<ColdData>>,
        origin_source_errors: SourceErrorChain,
    ) -> Option<Box<ColdData>> {
        match cold {
            Some(c) => {
                let c = *c;
                Some(Box::new(ColdData {
                    bag: DiagnosticBag {
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
                let mut cold_data = ColdData::default();
                cold_data.bag.origin_source_errors = Some(origin_source_errors);
                Some(Box::new(cold_data))
            }
        }
    }
}

impl<E> Report<E, super::HasSeverity> {
    /// Sets the severity only if not already set.
    ///
    /// This allows conditional chaining without type-state changes when severity
    /// is already set. Use `set_severity()` to force a new severity value.
    ///
    /// Since this implementation is for `Report<E, HasSeverity>`, the severity
    /// is already present, so this method simply returns `self` unchanged.
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
    ///
    /// // This is a no-op since severity is already set
    /// let report = report.with_severity(Severity::Warn);
    /// assert_eq!(report.severity(), Some(Severity::Error));
    /// ```
    pub fn with_severity(self, _severity: super::Severity) -> Self {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{MissingSeverity, Severity};

    #[derive(Debug)]
    struct TestError;

    impl core::fmt::Display for TestError {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("test error")
        }
    }

    impl Error for TestError {}

    #[derive(Debug)]
    struct OuterError;

    impl core::fmt::Display for OuterError {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("outer error")
        }
    }

    impl Error for OuterError {}

    impl From<TestError> for OuterError {
        fn from(_: TestError) -> Self {
            OuterError
        }
    }

    #[test]
    fn test_map_err_preserves_severity() {
        let report: Report<TestError, MissingSeverity> = Report::new(TestError);
        let mapped: Report<OuterError, MissingSeverity> = report.map_err(|_| OuterError);
        assert!(mapped.severity().is_none());
    }

    #[test]
    fn test_map_err_with_severity() {
        let report = Report::new(TestError).set_severity(Severity::Error);
        let mapped = report.map_err(|_| OuterError);
        assert_eq!(mapped.severity(), Some(Severity::Error));
    }
}
