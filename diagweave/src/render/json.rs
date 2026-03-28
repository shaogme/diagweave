#[path = "json/attachment.rs"]
mod attachment;
#[path = "json/helpers.rs"]
mod helpers;
#[path = "json/report.rs"]
mod report;
#[cfg(feature = "trace")]
#[path = "json/trace.rs"]
mod trace;

use core::error::Error;
use core::fmt::{self, Display, Formatter, Write};

use crate::report::Report;

use super::{REPORT_JSON_SCHEMA_VERSION, ReportRenderOptions};

pub(super) use helpers::{
    close_array, close_object, write_array_item_prefix, write_error_code, write_indent,
    write_json_display, write_json_string, write_object_field, write_option_string,
};

pub(super) fn write_json_report<E>(
    report: &Report<E>,
    options: ReportRenderOptions,
    f: &mut Formatter<'_>,
) -> fmt::Result
where
    E: Error + Display + 'static,
{
    let pretty = options.json_pretty;
    let mut first = true;
    let section_flags = calc_section_flags(report);

    f.write_char('{')?;
    write_object_field(f, pretty, 0, &mut first, "schema_version", |f| {
        write_json_string(f, REPORT_JSON_SCHEMA_VERSION)
    })?;
    write_object_field(f, pretty, 0, &mut first, "error", |f| {
        report::write_error_object(f, pretty, 1, report.inner())
    })?;
    if options.show_governance_section
        && (options.show_empty_sections || section_flags.has_metadata)
    {
        write_object_field(f, pretty, 0, &mut first, "metadata", |f| {
            report::write_metadata_object(f, pretty, 1, report)
        })?;
    }
    if (options.show_stack_trace_section || options.show_cause_chains_section)
        && (options.show_empty_sections || section_flags.has_diag_bag)
    {
        write_object_field(f, pretty, 0, &mut first, "diagnostic_bag", |f| {
            report::write_diag_bag(f, pretty, 1, report, options)
        })?;
    }
    #[cfg(feature = "trace")]
    if options.show_trace_section
        && (options.show_empty_sections || report.trace().is_some_and(|trace| !trace.is_empty()))
    {
        write_object_field(f, pretty, 0, &mut first, "trace", |f| {
            report::write_trace_object(f, pretty, 1, report)
        })?;
    }
    if options.show_context_section && (options.show_empty_sections || section_flags.has_context) {
        write_object_field(f, pretty, 0, &mut first, "context", |f| {
            attachment::write_context_array(f, pretty, 1, report)
        })?;
    }
    if options.show_attachments_section
        && (options.show_empty_sections || section_flags.has_attachments)
    {
        write_object_field(f, pretty, 0, &mut first, "attachments", |f| {
            attachment::write_attachments_array(f, pretty, 1, report)
        })?;
    }

    if pretty && !first {
        f.write_char('\n')?;
        write_indent(f, 0)?;
    }
    f.write_char('}')
}

struct JsonSectionFlags {
    has_metadata: bool,
    has_context: bool,
    has_attachments: bool,
    has_diag_bag: bool,
}

fn calc_section_flags<E>(report: &Report<E>) -> JsonSectionFlags
where
    E: Error + Display + 'static,
{
    let metadata = report.metadata();
    let has_metadata = metadata.error_code.is_some()
        || metadata.severity.is_some()
        || metadata.category.is_some()
        || metadata.retryable.is_some();
    let has_context = report
        .attachments()
        .iter()
        .any(|attachment| matches!(attachment, crate::report::Attachment::Context { .. }));
    let has_attachments = report
        .attachments()
        .iter()
        .any(|attachment| !matches!(attachment, crate::report::Attachment::Context { .. }));
    let has_diag_bag = has_stack_trace(report)
        || has_display_causes(report)
        || has_origin_source_errors(report)
        || has_diag_source_errors(report);
    JsonSectionFlags {
        has_metadata,
        has_context,
        has_attachments,
        has_diag_bag,
    }
}

fn has_stack_trace<E>(report: &Report<E>) -> bool
where
    E: Error + Display + 'static,
{
    report.stack_trace().is_some()
}

fn has_display_causes<E>(report: &Report<E>) -> bool
where
    E: Error + Display + 'static,
{
    report.display_causes_chain().is_some()
}

fn has_origin_source_errors<E>(report: &Report<E>) -> bool
where
    E: Error + Display + 'static,
{
    report.origin_src_err_chain().is_some() || report.inner().source().is_some()
}

fn has_diag_source_errors<E>(report: &Report<E>) -> bool
where
    E: Error + Display + 'static,
{
    report.diag_src_err_chain().is_some()
}
