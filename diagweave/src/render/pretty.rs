use alloc::borrow::ToOwned;
use alloc::string::String;
use core::error::Error;
use core::fmt::{self, Display, Formatter};

use super::{
    DiagnosticIr, DiagnosticIrAttachment, PrettyIndent, ReportRenderOptions, ReportRenderer,
};
use crate::report::{CauseStore, Report};

/// A renderer that outputs the diagnostic report in a human-readable pretty format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pretty {
    /// Options for rendering.
    pub options: ReportRenderOptions,
}

impl Pretty {
    /// Creates a new pretty renderer with specific options.
    pub fn new(options: ReportRenderOptions) -> Self {
        Self { options }
    }
}

impl<E, C> ReportRenderer<E, C> for Pretty
where
    E: Error + Display + 'static,
    C: CauseStore,
{
    fn render(&self, report: &Report<E, C>, f: &mut Formatter<'_>) -> fmt::Result {
        let ir = report.to_diagnostic_ir(self.options);
        let indent = pretty_indent(self.options.pretty_indent);

        render_error_section(f, &ir, &self.options, &indent)?;
        render_governance_section(f, &ir, &self.options, &indent)?;
        #[cfg(feature = "trace")]
        render_trace_section(f, &ir, &self.options, &indent)?;
        render_stack_trace(f, &ir, &self.options, &indent)?;
        render_context_section(f, &ir, &self.options, &indent)?;
        render_attachments(f, &ir, &self.options, &indent)?;
        render_display_causes(f, &ir, &self.options, &indent)?;
        render_source_errors(f, &ir, &self.options, &indent)?;

        Ok(())
    }
}

fn pretty_indent(indent: PrettyIndent) -> String {
    match indent {
        PrettyIndent::Spaces(n) => " ".repeat(n as usize),
        PrettyIndent::Tab => "\t".to_owned(),
    }
}

fn render_error_section(
    f: &mut Formatter<'_>,
    ir: &DiagnosticIr,
    options: &ReportRenderOptions,
    indent: &str,
) -> fmt::Result {
    writeln!(f, "Error:")?;
    writeln!(f, "{indent}- message: {}", ir.error.message)?;
    if options.show_type_name {
        writeln!(f, "{indent}- type: {}", ir.error.r#type)?;
    }
    Ok(())
}

fn render_governance_section(
    f: &mut Formatter<'_>,
    ir: &DiagnosticIr,
    options: &ReportRenderOptions,
    indent: &str,
) -> fmt::Result {
    let metadata = &ir.metadata;
    let has_metadata = metadata.error_code.is_some()
        || metadata.severity.is_some()
        || metadata.category.is_some()
        || metadata.retryable.is_some()
        || metadata.stack_trace.is_some()
        || metadata.display_causes.is_some()
        || metadata.source_errors.is_some();

    if options.show_governance_section && (options.show_empty_sections || has_metadata) {
        writeln!(f, "Governance:")?;
        if !has_metadata {
            writeln!(f, "{indent}- (none)")?;
        } else {
            render_gov_meta(f, metadata, indent)?;
            render_gov_stack(f, metadata, indent)?;
            render_gov_causes(f, metadata, indent)?;
            render_gov_errors(f, metadata, indent)?;
        }
    }
    Ok(())
}

fn render_gov_meta(
    f: &mut Formatter<'_>,
    metadata: &super::DiagnosticIrMetadata,
    indent: &str,
) -> fmt::Result {
    if let Some(error_code) = &metadata.error_code {
        writeln!(f, "{indent}- error_code: {error_code}")?;
    }
    if let Some(severity) = metadata.severity {
        writeln!(f, "{indent}- severity: {severity}")?;
    }
    if let Some(category) = &metadata.category {
        writeln!(f, "{indent}- category: {category}")?;
    }
    if let Some(retryable) = metadata.retryable {
        writeln!(f, "{indent}- retryable: {retryable}")?;
    }
    Ok(())
}

fn render_gov_stack(
    f: &mut Formatter<'_>,
    metadata: &super::DiagnosticIrMetadata,
    indent: &str,
) -> fmt::Result {
    if let Some(stack_trace) = &metadata.stack_trace {
        writeln!(f, "{indent}- stack_trace.format: {:?}", stack_trace.format)?;
        writeln!(
            f,
            "{indent}- stack_trace.frames: {}",
            stack_trace.frames.len()
        )?;
    }
    Ok(())
}

fn render_gov_causes(
    f: &mut Formatter<'_>,
    metadata: &super::DiagnosticIrMetadata,
    indent: &str,
) -> fmt::Result {
    if let Some(display_causes) = &metadata.display_causes {
        writeln!(
            f,
            "{indent}- display_causes.count: {}",
            display_causes.items.len()
        )?;
        writeln!(
            f,
            "{indent}- display_causes.truncated: {}",
            display_causes.truncated
        )?;
        writeln!(
            f,
            "{indent}- display_causes.cycle_detected: {}",
            display_causes.cycle_detected
        )?;
    }
    Ok(())
}

fn render_gov_errors(
    f: &mut Formatter<'_>,
    metadata: &super::DiagnosticIrMetadata,
    indent: &str,
) -> fmt::Result {
    if let Some(source_errors) = &metadata.source_errors {
        writeln!(
            f,
            "{indent}- source_errors.count: {}",
            source_errors.items.len()
        )?;
        writeln!(
            f,
            "{indent}- source_errors.truncated: {}",
            source_errors.truncated
        )?;
        writeln!(
            f,
            "{indent}- source_errors.cycle_detected: {}",
            source_errors.cycle_detected
        )?;
    }
    Ok(())
}

#[cfg(feature = "trace")]
fn render_trace_section(
    f: &mut Formatter<'_>,
    ir: &DiagnosticIr,
    options: &ReportRenderOptions,
    indent: &str,
) -> fmt::Result {
    if options.show_trace_section && (options.show_empty_sections || !ir.trace.is_empty()) {
        writeln!(f, "Trace:")?;
        if ir.trace.is_empty() {
            writeln!(f, "{indent}- (none)")?;
        } else {
            let trace = &ir.trace;
            if let Some(trace_id) = &trace.context.trace_id {
                writeln!(f, "{indent}- trace_id: {trace_id}")?;
            }
            if let Some(span_id) = &trace.context.span_id {
                writeln!(f, "{indent}- span_id: {span_id}")?;
            }
            if let Some(parent_span_id) = &trace.context.parent_span_id {
                writeln!(f, "{indent}- parent_span_id: {parent_span_id}")?;
            }
            if let Some(sampled) = trace.context.sampled {
                writeln!(f, "{indent}- sampled: {sampled}")?;
            }
            if let Some(trace_state) = &trace.context.trace_state {
                writeln!(f, "{indent}- trace_state: {trace_state}")?;
            }
            if let Some(flags) = trace.context.flags {
                writeln!(f, "{indent}- flags: {flags}")?;
            }
            for (idx, event) in trace.events.iter().enumerate() {
                writeln!(f, "{indent}- event[{idx}]: {}", event.name)?;
            }
        }
    }
    Ok(())
}

fn render_stack_trace(
    f: &mut Formatter<'_>,
    ir: &DiagnosticIr,
    options: &ReportRenderOptions,
    indent: &str,
) -> fmt::Result {
    let stack_trace = ir.metadata.stack_trace.as_ref();
    let has_stack = stack_trace.is_some();
    if !options.show_stack_trace_section || (!options.show_empty_sections && !has_stack) {
        return Ok(());
    }

    writeln!(f, "Stack Trace:")?;
    let Some(stack_trace) = stack_trace else {
        return writeln!(f, "{indent}- (none)");
    };

    writeln!(f, "{indent}- format: {:?}", stack_trace.format)?;
    if options.stack_trace_include_frames && !stack_trace.frames.is_empty() {
        for (idx, frame) in stack_trace.frames.iter().enumerate() {
            writeln!(
                f,
                "{indent}- frame[{idx}]: symbol={:?}, module={:?}, file={:?}, line={:?}, column={:?}",
                frame.symbol, frame.module_path, frame.file, frame.line, frame.column
            )?;
        }
    } else if options.stack_trace_include_raw {
        render_raw_stack_trace(f, stack_trace, options, indent)?;
    } else {
        writeln!(f, "{indent}- (hidden by options)")?;
    }
    Ok(())
}

fn render_raw_stack_trace(
    f: &mut Formatter<'_>,
    stack_trace: &crate::report::StackTrace,
    options: &ReportRenderOptions,
    indent: &str,
) -> fmt::Result {
    if let Some(raw) = &stack_trace.raw {
        for line in raw.lines().take(options.stack_trace_max_lines) {
            writeln!(f, "{indent}- {line}")?;
        }
        if raw.lines().count() > options.stack_trace_max_lines {
            writeln!(f, "{indent}- ... truncated stack trace output")?;
        }
    } else {
        writeln!(f, "{indent}- (empty)")?;
    }
    Ok(())
}

fn render_context_section(
    f: &mut Formatter<'_>,
    ir: &DiagnosticIr,
    options: &ReportRenderOptions,
    indent: &str,
) -> fmt::Result {
    if options.show_context_section && (options.show_empty_sections || !ir.context.is_empty()) {
        writeln!(f, "Context:")?;
        if ir.context.is_empty() {
            writeln!(f, "{indent}- (none)")?;
        } else {
            for item in &ir.context {
                writeln!(f, "{indent}- {}: {}", item.key, item.value)?;
            }
        }
    }
    Ok(())
}

fn render_attachments(
    f: &mut Formatter<'_>,
    ir: &DiagnosticIr,
    options: &ReportRenderOptions,
    indent: &str,
) -> fmt::Result {
    if options.show_attachments_section
        && (options.show_empty_sections || !ir.attachments.is_empty())
    {
        writeln!(f, "Attachments:")?;
        if ir.attachments.is_empty() {
            writeln!(f, "{indent}- (none)")?;
        } else {
            for item in &ir.attachments {
                match item {
                    DiagnosticIrAttachment::Note { message } => {
                        writeln!(f, "{indent}- note: {message}")?
                    }
                    DiagnosticIrAttachment::Payload {
                        name,
                        value,
                        media_type,
                    } => match media_type {
                        Some(media_type) => {
                            writeln!(f, "{indent}- payload {name} ({media_type}): {value}")?
                        }
                        None => writeln!(f, "{indent}- payload {name}: {value}")?,
                    },
                }
            }
        }
    }
    Ok(())
}

fn render_display_causes(
    f: &mut Formatter<'_>,
    ir: &DiagnosticIr,
    options: &ReportRenderOptions,
    indent: &str,
) -> fmt::Result {
    let causes = ir.metadata.display_causes.as_ref();
    if options.show_cause_chains_section && (options.show_empty_sections || causes.is_some()) {
        writeln!(f, "Display Causes:")?;
        if let Some(causes) = causes {
            if causes.items.is_empty() {
                writeln!(f, "{indent}- (none)")?;
            } else {
                for (idx, cause) in causes.items.iter().enumerate() {
                    writeln!(f, "{indent}{}. {}", idx + 1, cause)?;
                }
            }
            if causes.truncated {
                writeln!(f, "{indent}- ... truncated by max_source_depth")?;
            }
            if causes.cycle_detected {
                writeln!(f, "{indent}- ... cycle detected and traversal stopped")?;
            }
        } else {
            writeln!(f, "{indent}- (none)")?;
        }
    }
    Ok(())
}

fn render_source_errors(
    f: &mut Formatter<'_>,
    ir: &DiagnosticIr,
    options: &ReportRenderOptions,
    indent: &str,
) -> fmt::Result {
    let sources = ir.metadata.source_errors.as_ref();
    if options.show_cause_chains_section && (options.show_empty_sections || sources.is_some()) {
        writeln!(f, "Source Errors:")?;
        if let Some(sources) = sources {
            if sources.items.is_empty() {
                writeln!(f, "{indent}- (none)")?;
            } else {
                for (idx, source) in sources.items.iter().enumerate() {
                    writeln!(f, "{indent}{}. {}", idx + 1, source.message)?;
                }
            }
            if sources.truncated {
                writeln!(f, "{indent}- ... truncated by max_source_depth")?;
            }
            if sources.cycle_detected {
                writeln!(f, "{indent}- ... cycle detected and traversal stopped")?;
            }
        } else {
            writeln!(f, "{indent}- (none)")?;
        }
    }
    Ok(())
}
