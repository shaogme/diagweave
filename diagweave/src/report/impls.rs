use core::error::Error;
use core::fmt::{self, Debug, Display, Formatter};

use crate::report::Attachment;

use super::Report;
use super::store::CauseStore;

impl<E, C> Debug for Report<E, C>
where
    E: Debug,
    C: Debug + CauseStore,
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
                writeln!(f, "  - cause_store: {:?}", diag.causes)?;
            } else {
                writeln!(f, "  - attachments: (none)")?;
                #[cfg(feature = "trace")]
                writeln!(f, "  - trace: (none)")?;
                writeln!(f, "  - cause_store: (none)")?;
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

impl<E, C> Display for Report<E, C>
where
    E: Display,
    C: CauseStore,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner())?;
        let metadata = self.metadata();
        let has_metadata = metadata.error_code.is_some()
            || metadata.severity.is_some()
            || metadata.category.is_some()
            || metadata.retryable.is_some()
            || metadata.stack_trace.is_some()
            || metadata.display_causes.is_some()
            || metadata.error_sources.is_some();
        let has_diagnostics = self.diagnostics().is_some_and(|diag| {
            !diag.attachments.is_empty() || {
                #[cfg(feature = "trace")]
                {
                    !diag.trace.is_empty()
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

impl<E, C> Report<E, C>
where
    E: Display,
    C: CauseStore,
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
        if let Some(cat) = &metadata.category {
            write_field("category", cat)?;
        }
        if let Some(ret) = metadata.retryable {
            write_field("retryable", &ret)?;
        }
        if metadata.stack_trace.is_some() {
            write_field("stack_trace", &"present")?;
        }
        if let Some(display_causes) = &metadata.display_causes {
            write_field("display_causes", &display_causes.items.len())?;
        }
        if let Some(error_sources) = &metadata.error_sources {
            write_field("error_sources", &error_sources.items.len())?;
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

                if let Some(tid) = &diag.trace.context.trace_id {
                    write_field("trace_id", tid)?;
                }
                if let Some(sid) = &diag.trace.context.span_id {
                    write_field("span_id", sid)?;
                }
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

impl<E, C> Error for Report<E, C>
where
    E: Error + 'static,
    C: CauseStore + core::fmt::Debug,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.diagnostics()
            .and_then(|diag| diag.error_sources.first().map(|err| err.as_ref()))
            .or_else(|| self.inner().source())
    }
}
