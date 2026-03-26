#[path = "json/attachment.rs"]
mod attachment;
#[path = "json/report.rs"]
mod report;

use core::error::Error;
use core::fmt::{self, Display, Formatter, Write};

use crate::report::{ErrorCode, Report};

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
    let pretty = options.json_pretty;
    let mut first = true;

    f.write_char('{')?;
    write_object_field(f, pretty, 0, &mut first, "schema_version", |f| {
        write_json_string(f, REPORT_JSON_SCHEMA_VERSION)
    })?;
    write_object_field(f, pretty, 0, &mut first, "error", |f| {
        report::write_error_object(f, pretty, 1, report.inner())
    })?;
    write_object_field(f, pretty, 0, &mut first, "metadata", |f| {
        report::write_metadata_object(f, pretty, 1, report, options)
    })?;
    #[cfg(feature = "trace")]
    if report.trace().is_some() {
        write_object_field(f, pretty, 0, &mut first, "trace", |f| {
            report::write_trace_object(f, pretty, 1, report)
        })?;
    }
    write_object_field(f, pretty, 0, &mut first, "context", |f| {
        attachment::write_context_array(f, pretty, 1, report)
    })?;
    write_object_field(f, pretty, 0, &mut first, "attachments", |f| {
        attachment::write_attachments_array(f, pretty, 1, report)
    })?;

    if pretty && !first {
        f.write_char('\n')?;
        write_indent(f, 0)?;
    }
    f.write_char('}')
}

// Internal utilities used by submodules

fn write_error_code(f: &mut Formatter<'_>, code: &ErrorCode) -> fmt::Result {
    match code {
        ErrorCode::Integer(v) => write!(f, "{v}"),
        ErrorCode::String(v) => write_json_string(f, v),
    }
}

fn write_option_string(f: &mut Formatter<'_>, value: Option<&str>) -> fmt::Result {
    match value {
        Some(v) => write_json_string(f, v),
        None => f.write_str("null"),
    }
}

fn write_json_display(f: &mut Formatter<'_>, value: &(impl Display + ?Sized)) -> fmt::Result {
    f.write_char('"')?;
    {
        let mut escaper = JsonStringEscaper { out: f };
        write!(&mut escaper, "{value}")?;
    }
    f.write_char('"')
}

fn write_json_string(f: &mut Formatter<'_>, value: impl AsRef<str>) -> fmt::Result {
    f.write_char('"')?;
    {
        let mut escaper = JsonStringEscaper { out: f };
        escaper.write_str(value.as_ref())?;
    }
    f.write_char('"')
}

struct JsonStringEscaper<'a, 'b> {
    out: &'a mut Formatter<'b>,
}

impl Write for JsonStringEscaper<'_, '_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for ch in s.chars() {
            match ch {
                '"' => self.out.write_str("\\\"")?,
                '\\' => self.out.write_str("\\\\")?,
                '\n' => self.out.write_str("\\n")?,
                '\r' => self.out.write_str("\\r")?,
                '\t' => self.out.write_str("\\t")?,
                '\u{08}' => self.out.write_str("\\b")?,
                '\u{0C}' => self.out.write_str("\\f")?,
                c if c <= '\u{1F}' => write!(self.out, "\\u{:04X}", c as u32)?,
                c => self.out.write_char(c)?,
            }
        }
        Ok(())
    }
}

fn write_object_field<F>(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    first: &mut bool,
    key: &str,
    mut write_value: F,
) -> fmt::Result
where
    F: FnMut(&mut Formatter<'_>) -> fmt::Result,
{
    if *first {
        *first = false;
    } else {
        f.write_char(',')?;
    }
    if pretty {
        f.write_char('\n')?;
        write_indent(f, depth + 1)?;
    }
    write_json_string(f, key)?;
    f.write_char(':')?;
    if pretty {
        f.write_char(' ')?;
    }
    write_value(f)
}

fn write_array_item_prefix(
    f: &mut Formatter<'_>,
    pretty: bool,
    depth: usize,
    first: &mut bool,
) -> fmt::Result {
    if *first {
        *first = false;
    } else {
        f.write_char(',')?;
    }
    if pretty {
        f.write_char('\n')?;
        write_indent(f, depth + 1)?;
    }
    Ok(())
}

fn close_object(f: &mut Formatter<'_>, pretty: bool, depth: usize, empty: bool) -> fmt::Result {
    if pretty && !empty {
        f.write_char('\n')?;
        write_indent(f, depth)?;
    }
    f.write_char('}')
}

fn close_array(f: &mut Formatter<'_>, pretty: bool, depth: usize, empty: bool) -> fmt::Result {
    if pretty && !empty {
        f.write_char('\n')?;
        write_indent(f, depth)?;
    }
    f.write_char(']')
}

fn write_indent(f: &mut Formatter<'_>, depth: usize) -> fmt::Result {
    for _ in 0..depth {
        f.write_str("  ")?;
    }
    Ok(())
}
