use core::error::Error;
use core::fmt::{self, Display, Formatter};

use crate::report::Report;

use super::{ReportRenderOptions, ReportRenderer};

/// A renderer that outputs the diagnostic report in JSON format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Json {
    pub options: ReportRenderOptions,
}

pub const REPORT_JSON_SCHEMA_VERSION: &str = "v0.1.0";

pub const REPORT_JSON_SCHEMA_DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";

/// Returns the JSON schema for the diagnostic report.
pub fn report_json_schema() -> &'static str {
    include_str!("../../schemas/report-v0.1.0.schema.json")
}

impl Json {
    /// Creates a new JSON renderer with specific options.
    pub fn new(options: ReportRenderOptions) -> Self {
        Self { options }
    }
}

impl<E> ReportRenderer<E> for Json
where
    E: Error + Display + 'static,
{
    fn render(&self, report: &Report<E>, f: &mut Formatter<'_>) -> fmt::Result {
        render_json(report, self.options, f)
    }
}

fn render_json<E>(
    report: &Report<E>,
    options: ReportRenderOptions,
    f: &mut Formatter<'_>,
) -> fmt::Result
where
    E: Error + Display + 'static,
{
    let ir = report.to_diagnostic_ir(options);

    let encoded = if options.json_pretty {
        serde_json::to_string_pretty(&ir)
    } else {
        serde_json::to_string(&ir)
    };

    match encoded {
        Ok(payload) => write!(f, "{payload}"),
        Err(_) => write!(f, "{{\"error\":\"json serialization failed\"}}"),
    }
}
