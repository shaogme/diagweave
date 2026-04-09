use super::*;

impl<E, State> Report<E, State>
where
    E: Error + 'static,
    State: SeverityState,
{
    /// Iterates origin source errors using default collection options.
    pub fn iter_origin_sources(&self) -> ReportSourceErrorIter<'_> {
        self.iter_origin_src_ext(self.options().as_cause_options())
    }

    /// Iterates origin source errors using custom collection options.
    pub fn iter_origin_src_ext(&self, options: CauseCollectOptions) -> ReportSourceErrorIter<'_> {
        ReportSourceErrorIter::new_origin(self, options)
    }

    /// Iterates diagnostic source errors using default collection options.
    pub fn iter_diag_sources(&self) -> ReportSourceErrorIter<'_> {
        self.iter_diag_srcs_ext(self.options().as_cause_options())
    }

    /// Iterates diagnostic source errors using custom collection options.
    pub fn iter_diag_srcs_ext(&self, options: CauseCollectOptions) -> ReportSourceErrorIter<'_> {
        ReportSourceErrorIter::new_diagnostic(self, options)
    }
}

impl<E, State> Report<E, State>
where
    E: Error + 'static,
    State: SeverityState,
{
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
