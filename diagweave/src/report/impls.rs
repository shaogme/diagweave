use core::error::Error;
use core::fmt::{self, Debug, Display, Formatter};

use crate::report::Attachment;

use super::Report;

impl<E> Debug for Report<E>
where
    E: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        #[cfg(debug_assertions)]
        {
            writeln!(f, "Report:")?;
            writeln!(f, "  - error: {:?}", self.inner())?;
            writeln!(f, "  - metadata: {:?}", self.metadata())?;
            if let Some(diag) = self.diagnostics() {
                writeln!(f, "  - attachments:")?;
                if diag.attachments.is_empty() {
                    writeln!(f, "    - (none)")?;
                } else {
                    for attachment in &diag.attachments {
                        writeln!(f, "    - {:?}", attachment)?;
                    }
                }
                #[cfg(feature = "trace")]
                writeln!(f, "  - trace: {:?}", diag.trace)?;
                let display_causes = diag
                    .display_causes
                    .as_ref()
                    .map(|v| v.items.as_slice())
                    .unwrap_or(&[]);
                if display_causes.is_empty() {
                    writeln!(f, "  - display_causes: (none)")?;
                } else {
                    writeln!(f, "  - display_causes:")?;
                    for cause in display_causes {
                        writeln!(f, "    - {}", cause)?;
                    }
                }
            } else {
                writeln!(f, "  - attachments: (none)")?;
                #[cfg(feature = "trace")]
                writeln!(f, "  - trace: (none)")?;
                writeln!(f, "  - display_causes: (none)")?;
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

impl<E> Display for Report<E>
where
    E: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner())?;
        let metadata = self.metadata();
        let has_metadata = metadata.error_code.is_some()
            || metadata.severity.is_some()
            || metadata.category.is_some()
            || metadata.retryable.is_some();
        let has_diagnostics = self.diagnostics().is_some_and(|diag| {
            diag.stack_trace.is_some() || !diag.attachments.is_empty() || {
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

impl<E> Report<E>
where
    E: Display,
{
    fn fmt_metadata_fields(&self, f: &mut Formatter<'_>, idx: &mut usize) -> fmt::Result {
        let metadata = self.metadata();
        let mut write_field = |name: &str, val: &dyn Display| -> fmt::Result {
            if *idx > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{name}={val}")?;
            *idx += 1;
            Ok(())
        };
        if let Some(code) = &metadata.error_code {
            write_field("code", code)?;
        }
        if let Some(sev) = metadata.severity {
            write_field("severity", &sev)?;
        }
        if let Some(cat) = metadata.category.as_deref() {
            write_field("category", &cat)?;
        }
        if let Some(ret) = metadata.retryable {
            write_field("retryable", &ret)?;
        }
        Ok(())
    }

    fn fmt_diag_fields(&self, f: &mut Formatter<'_>, idx: &mut usize) -> fmt::Result {
        if let Some(diag) = self.diagnostics() {
            #[cfg(feature = "trace")]
            {
                let mut write_field = |name: &str, val: &dyn Display| -> fmt::Result {
                    if *idx > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{name}={val}")?;
                    *idx += 1;
                    Ok(())
                };

                if let Some(trace) = diag.trace.as_ref() {
                    if let Some(tid) = &trace.context.trace_id {
                        write_field("trace_id", &tid.as_ref())?;
                    }
                    if let Some(sid) = &trace.context.span_id {
                        write_field("span_id", &sid.as_ref())?;
                    }
                }
            }

            if diag.stack_trace.is_some() {
                if *idx > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "stack_trace=present")?;
                *idx += 1;
            }

            for attachment in &diag.attachments {
                if *idx > 0 {
                    write!(f, ", ")?;
                }
                match attachment {
                    Attachment::Context { key, value } => write!(f, "{key}={value}")?,
                    Attachment::Note { message } => write!(f, "{message}")?,
                    Attachment::Payload {
                        name,
                        value,
                        media_type,
                    } => match media_type {
                        Some(mt) => write!(f, "{name}={value} ({mt})")?,
                        None => write!(f, "{name}={value}")?,
                    },
                }
                *idx += 1;
            }
        }
        Ok(())
    }
}

impl<E> Error for Report<E>
where
    E: Error + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.diagnostics()
            .and_then(|diag| {
                diag.source_errors.as_ref().and_then(|v| {
                    v.items
                        .first()
                        .map(|err| err.error.as_ref() as &(dyn Error + 'static))
                })
            })
            .or_else(|| self.inner().source())
    }
}
