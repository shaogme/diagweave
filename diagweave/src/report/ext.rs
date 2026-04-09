use core::fmt::Display;

use super::{
    Attachment, ErrorCode, MissingSeverity, Report, ReportMetadata, Severity, SeverityState,
};

/// A trait for types that can be converted into a diagnostic result.
pub trait Diagnostic {
    /// The success value type.
    type Value;
    /// The error type.
    type Error;

    fn to_report(self) -> Result<Self::Value, Report<Self::Error>>;

    /// Convenience: perform a transformation on the error path in a single step.
    ///
    /// This is a generic variant that allows transforming both the error type
    /// and the state type. When only adding metadata (context, notes, etc.),
    /// no explicit type annotations are needed. When transforming the error
    /// type (e.g., via `map_err`), the return type must be annotated.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // No type annotation needed when only adding metadata
    /// fail_auth().diag(|r| {
    ///     r.with_ctx("request_id", 77u64)
    ///         .with_error_code("AUTH.INVALID_TOKEN")
    /// })
    ///
    /// // Type annotation needed when transforming error type
    /// let err: Result<(), Report<ApiError>> = fail_auth().diag(|r| {
    ///     r.map_err(|_| ApiError::Unauthorized)
    /// });
    /// ```
    fn diag<E2, State2>(
        self,
        f: impl FnOnce(Report<Self::Error>) -> Report<E2, State2>,
    ) -> Result<Self::Value, Report<E2, State2>>
    where
        Self: Sized,
        State2: SeverityState,
    {
        self.to_report().map_err(f)
    }

    fn to_report_note(
        self,
        message: impl Display + Send + Sync + 'static,
    ) -> Result<Self::Value, Report<Self::Error>>
    where
        Self: Sized,
    {
        self.to_report()
            .and_then_report(|report| report.attach_note(message))
    }
}

impl<T, E> Diagnostic for Result<T, E> {
    type Value = T;
    type Error = E;

    fn to_report(self) -> Result<Self::Value, Report<Self::Error>> {
        self.map_err(Report::new)
    }
}

/// Extension trait for `Result<T, Report<E, State>>` to apply diagnostic transformations
/// only on the error path, without duplicating every `Report` method.
///
/// # Example
///
/// ```ignore
/// db_operation()
///     .diag(|r| {
///         r.with_ctx("user_id", user_id)
///             .attach_note("failing over")
///             .capture_stack_trace()
///     })
///     .map_err(|db_err| AppError::from(db_err))?;
/// ```
pub trait ResultReportExt<T, E, State = MissingSeverity>
where
    State: SeverityState,
{
    /// Applies a transformation to the inner `Report` only on the error path.
    ///
    /// The closure receives an owned `Report` and must return an owned `Report`
    /// closure is never invoked.
    fn and_then_report<NewE, NewState>(
        self,
        f: impl FnOnce(Report<E, State>) -> Report<NewE, NewState>,
    ) -> Result<T, Report<NewE, NewState>>
    where
        NewState: SeverityState;
}

impl<T, E, State> ResultReportExt<T, E, State> for Result<T, Report<E, State>>
where
    State: SeverityState,
{
    fn and_then_report<NewE, NewState>(
        self,
        f: impl FnOnce(Report<E, State>) -> Report<NewE, NewState>,
    ) -> Result<T, Report<NewE, NewState>>
    where
        NewState: SeverityState,
    {
        self.map_err(f)
    }
}

/// Read-only inspection trait for `Result<T, Report<E, State>>`.
///
/// Provides convenient accessors that return `None` on the `Ok` path,
/// avoiding the need to manually match before reading report fields.
pub trait InspectReportExt<T, E, State = MissingSeverity>
where
    State: SeverityState,
{
    /// Returns a reference to the inner `Report` on the error path, or `None`.
    fn report_ref(&self) -> Option<&Report<E, State>>;

    /// Returns the report's attachments on the error path, or `None`.
    fn report_attachments(&self) -> Option<&[Attachment]>;

    /// Returns the report's metadata on the error path, or `None`.
    fn report_metadata(&self) -> Option<&ReportMetadata>;

    /// Returns the report's error code on the error path, or `None`.
    fn report_error_code(&self) -> Option<&ErrorCode>;

    /// Returns the report's severity on the error path, or `None`.
    fn report_severity(&self) -> Option<Severity>;

    /// Returns the report's category on the error path, or `None`.
    fn report_category(&self) -> Option<&str>;

    /// Returns whether the report is retryable on the error path, or `None`.
    fn report_retryable(&self) -> Option<bool>;
}

impl<T, E, State> InspectReportExt<T, E, State> for Result<T, Report<E, State>>
where
    State: SeverityState,
{
    fn report_ref(&self) -> Option<&Report<E, State>> {
        self.as_ref().err()
    }

    fn report_attachments(&self) -> Option<&[Attachment]> {
        self.report_ref().map(Report::attachments)
    }

    fn report_metadata(&self) -> Option<&ReportMetadata> {
        self.report_ref().map(Report::metadata)
    }

    fn report_error_code(&self) -> Option<&ErrorCode> {
        self.report_ref().and_then(Report::error_code)
    }

    fn report_severity(&self) -> Option<Severity> {
        self.report_ref().and_then(Report::severity)
    }

    fn report_category(&self) -> Option<&str> {
        self.report_ref().and_then(Report::category)
    }

    fn report_retryable(&self) -> Option<bool> {
        self.report_ref().and_then(Report::retryable)
    }
}
