use core::error::Error;
use core::fmt::{self, Debug, Display, Formatter};

use crate::report::Attachment;
use crate::report::SourceErrorChain;

use super::{Report, SeverityState};

/// Macro to write a field with separator handling.
/// Writes ", " before the field if idx > 0, then increments idx.
macro_rules! write_field {
    ($f:expr, $idx:expr, $name:expr, $val:expr) => {{
        if $idx > 0 {
            write!($f, ", ")?;
        }
        write!($f, "{}={}", $name, $val)?;
        $idx += 1;
    }};
}

/// Macro to write raw content with separator handling.
/// Writes ", " before the content if idx > 0, then increments idx.
macro_rules! write_raw {
    ($f:expr, $idx:expr, $($arg:tt)*) => {{
        if $idx > 0 {
            write!($f, ", ")?;
        }
        write!($f, $($arg)*)?;
        $idx += 1;
    }};
}

impl<E, State> Debug for Report<E, State>
where
    E: Debug,
    State: SeverityState + Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        #[cfg(debug_assertions)]
        {
            writeln!(f, "Report:")?;
            writeln!(f, " - error: {:?}", self.inner())?;
            writeln!(f, " - metadata: {:?}", self.metadata())?;
            if let Some(diag) = self.diagnostics() {
                writeln!(f, " - attachments:")?;
                if diag.attachments.is_empty() {
                    writeln!(f, " - (none)")?;
                } else {
                    for attachment in &diag.attachments {
                        writeln!(f, " - {:?}", attachment)?;
                    }
                }
                #[cfg(feature = "trace")]
                writeln!(f, " - trace: {:?}", diag.trace)?;
                let display_causes = diag
                    .display_causes
                    .as_ref()
                    .map(|v| v.items.as_slice())
                    .unwrap_or(&[]);
                if display_causes.is_empty() {
                    writeln!(f, " - display_causes: (none)")?;
                } else {
                    writeln!(f, " - display_causes:")?;
                    for cause in display_causes {
                        writeln!(f, " - {}", cause)?;
                    }
                }
            } else {
                writeln!(f, " - attachments: (none)")?;
                #[cfg(feature = "trace")]
                writeln!(f, " - trace: (none)")?;
                writeln!(f, " - display_causes: (none)")?;
            }
            Ok(())
        }
        #[cfg(not(debug_assertions))]
        {
            f.debug_struct("Report")
                .field("inner", self.inner())
                .field("cold", &self.cold)
                .finish()
        }
    }
}

impl<E, State> Display for Report<E, State>
where
    E: Display,
    State: SeverityState,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner())?;
        let metadata = self.metadata();
        let has_metadata = metadata.error_code().is_some()
            || self.severity().is_some()
            || metadata.category().is_some()
            || metadata.retryable().is_some();
        let has_diagnostics = self.diagnostics().is_some_and(|diag| {
            diag.stack_trace.is_some()
                || !diag.context.is_empty()
                || !diag.system.is_empty()
                || !diag.attachments.is_empty()
                || {
                    #[cfg(feature = "trace")]
                    {
                        diag.trace.as_ref().is_some_and(|trace| !trace.is_empty())
                    }
                    #[cfg(not(feature = "trace"))]
                    {
                        false
                    }
                }
        });
        if !has_diagnostics && !has_metadata {
            return Ok(());
        }
        write!(f, " [")?;
        let mut idx = 0usize;
        self.fmt_metadata_fields(f, &mut idx)?;
        self.fmt_diag_fields(f, &mut idx)?;
        write!(f, "]")
    }
}

impl<E, State> Report<E, State>
where
    E: Display,
    State: SeverityState,
{
    fn fmt_metadata_fields(&self, f: &mut Formatter<'_>, idx: &mut usize) -> fmt::Result {
        let metadata = self.metadata();
        if let Some(code) = metadata.error_code() {
            write_field!(f, *idx, "code", code);
        }
        if let Some(sev) = self.severity() {
            write_field!(f, *idx, "severity", &sev);
        }
        if let Some(cat) = metadata.category() {
            write_field!(f, *idx, "category", &cat);
        }
        if let Some(ret) = metadata.retryable() {
            write_field!(f, *idx, "retryable", &ret);
        }
        Ok(())
    }

    fn fmt_diag_fields(&self, f: &mut Formatter<'_>, idx: &mut usize) -> fmt::Result {
        let Some(diag) = self.diagnostics() else {
            return Ok(());
        };

        #[cfg(feature = "trace")]
        if let Some(trace) = diag.trace.as_ref() {
            if let Some(tid) = &trace.context.trace_id {
                write_field!(f, *idx, "trace_id", tid.as_ref());
            }
            if let Some(sid) = &trace.context.span_id {
                write_field!(f, *idx, "span_id", sid.as_ref());
            }
        }

        if diag.stack_trace.is_some() {
            write_field!(f, *idx, "stack_trace", "present");
        }

        for (key, value) in diag.context.sorted_entries() {
            write_field!(f, *idx, key, value);
        }

        for (key, value) in diag.system.sorted_entries() {
            write_field!(f, *idx, format_args!("system.{}", key), value);
        }

        for attachment in &diag.attachments {
            match attachment {
                Attachment::Note { message } => {
                    write_raw!(f, *idx, "{}", message);
                }
                Attachment::Payload {
                    name,
                    value,
                    media_type,
                } => match media_type {
                    Some(mt) => write_raw!(f, *idx, "{}={} ({})", name, value, mt),
                    None => write_raw!(f, *idx, "{}={}", name, value),
                },
            }
        }

        Ok(())
    }
}

impl<E, State> Error for Report<E, State>
where
    E: Error + 'static,
    State: SeverityState + Debug,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.diagnostics()
            .and_then(|diag| {
                diag.origin_source_errors
                    .as_ref()
                    .and_then(SourceErrorChain::first_error)
            })
            .or_else(|| self.inner().source())
    }
}
