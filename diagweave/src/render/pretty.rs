use alloc::vec;
use core::error::Error;
use core::fmt::{self, Display, Formatter};

use crate::report::{ContextMap, Report, SeverityState};

use super::{PrettyIndent, ReportRenderOptions, ReportRenderer, filtered_frames};

const INDENT_SPACES: &str = {
    const LEN: usize = 64;
    const SPACES: [u8; LEN] = [b' '; LEN];
    match alloc::str::from_utf8(&SPACES) {
        Ok(s) => s,
        Err(_) => panic!("Invalid UTF-8"),
    }
};
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

impl<E, State> ReportRenderer<E, State> for Pretty
where
    E: Error + Display + 'static,
    State: SeverityState,
{
    fn render(&self, report: &Report<E, State>, f: &mut Formatter<'_>) -> fmt::Result {
        let options = self.options;
        render_error_section(
            f,
            report.inner(),
            core::any::type_name::<E>(),
            options.pretty_indent,
            options.show_type_name,
        )?;
        render_governance_section(f, report, options)?;
        #[cfg(feature = "trace")]
        render_trace_section(f, report, options)?;
        #[cfg(not(feature = "trace"))]
        if options.show_trace_section && options.show_empty_sections {
            writeln!(f, "Trace:")?;
            write_indent(f, options.pretty_indent)?;
            writeln!(f, "- (none)")?;
        }
        render_stack_trace(f, report, options)?;
        render_attachments(f, report, options)?;
        render_display_causes(f, report, options)?;
        render_src_errors_section(
            f,
            report,
            options,
            "Origin Source Errors:",
            true,
            Report::origin_src_err_view,
        )?;
        render_src_errors_section(
            f,
            report,
            options,
            "Diagnostic Source Errors:",
            false,
            Report::diag_src_err_view,
        )?;
        Ok(())
    }
}

fn write_indent(f: &mut Formatter<'_>, indent: PrettyIndent) -> fmt::Result {
    match indent {
        PrettyIndent::Spaces(n) => {
            let mut remaining = usize::from(n);
            while remaining > 0 {
                let chunk = remaining.min(INDENT_SPACES.len());
                f.write_str(&INDENT_SPACES[..chunk])?;
                remaining -= chunk;
            }
        }
        PrettyIndent::Tab => {
            f.write_str("\t")?;
        }
    }
    Ok(())
}

fn render_error_section(
    f: &mut Formatter<'_>,
    error: &impl Display,
    type_name: &str,
    indent: PrettyIndent,
    show_type_name: bool,
) -> fmt::Result {
    writeln!(f, "Error:")?;
    write_indent(f, indent)?;
    writeln!(f, "- message: {error}")?;
    if show_type_name {
        write_indent(f, indent)?;
        writeln!(f, "- type: {type_name}")?;
    }
    Ok(())
}

fn render_governance_section<State>(
    f: &mut Formatter<'_>,
    report: &Report<impl Error + 'static, State>,
    options: ReportRenderOptions,
) -> fmt::Result
where
    State: SeverityState,
{
    let metadata = report.metadata();
    let has_metadata = metadata.error_code().is_some()
        || metadata.severity().is_some()
        || metadata.category().is_some()
        || metadata.retryable().is_some();

    if options.show_governance_section && (options.show_empty_sections || has_metadata) {
        writeln!(f, "Governance:")?;
        if !has_metadata {
            write_indent(f, options.pretty_indent)?;
            writeln!(f, "- (none)")?;
        } else {
            render_gov_meta(f, metadata, options.pretty_indent)?;
        }
    }
    Ok(())
}

fn render_gov_meta<State>(
    f: &mut Formatter<'_>,
    metadata: &crate::report::ReportMetadata<State>,
    indent: PrettyIndent,
) -> fmt::Result
where
    State: SeverityState,
{
    if let Some(error_code) = metadata.error_code() {
        write_indent(f, indent)?;
        writeln!(f, "- error_code: {error_code}")?;
    }
    if let Some(severity) = metadata.severity() {
        write_indent(f, indent)?;
        writeln!(f, "- severity: {severity}")?;
    }
    if let Some(category) = metadata.category() {
        write_indent(f, indent)?;
        writeln!(f, "- category: {category}")?;
    }
    if let Some(retryable) = metadata.retryable() {
        write_indent(f, indent)?;
        writeln!(f, "- retryable: {retryable}")?;
    }
    Ok(())
}

#[cfg(feature = "trace")]
fn render_trace_section<State>(
    f: &mut Formatter<'_>,
    report: &Report<impl Error + 'static, State>,
    options: ReportRenderOptions,
) -> fmt::Result
where
    State: SeverityState,
{
    let Some(trace) = report.trace() else {
        if options.show_trace_section && options.show_empty_sections {
            writeln!(f, "Trace:")?;
            write_indent(f, options.pretty_indent)?;
            writeln!(f, "- (none)")?;
        }
        return Ok(());
    };

    if options.show_trace_section && (options.show_empty_sections || !trace.is_empty()) {
        writeln!(f, "Trace:")?;
        if trace.is_empty() {
            write_indent(f, options.pretty_indent)?;
            writeln!(f, "- (none)")?;
        } else {
            if let Some(trace_id) = &trace.context.trace_id {
                write_indent(f, options.pretty_indent)?;
                writeln!(f, "- trace_id: {}", trace_id.as_ref())?;
            }
            if let Some(span_id) = &trace.context.span_id {
                write_indent(f, options.pretty_indent)?;
                writeln!(f, "- span_id: {}", span_id.as_ref())?;
            }
            if let Some(parent_span_id) = &trace.context.parent_span_id {
                write_indent(f, options.pretty_indent)?;
                writeln!(f, "- parent_span_id: {}", parent_span_id.as_ref())?;
            }
            if let Some(sampled) = trace.context.sampled {
                write_indent(f, options.pretty_indent)?;
                writeln!(f, "- sampled: {sampled}")?;
            }
            if let Some(trace_state) = &trace.context.trace_state {
                write_indent(f, options.pretty_indent)?;
                writeln!(f, "- trace_state: {trace_state}")?;
            }
            if let Some(flags) = trace.context.flags {
                write_indent(f, options.pretty_indent)?;
                writeln!(f, "- flags: {flags}")?;
            }
            for (idx, event) in trace.events.iter().enumerate() {
                write_indent(f, options.pretty_indent)?;
                if let Some(level) = &event.level {
                    writeln!(f, "- event[{idx}]: {} (level: {})", event.name, level)?;
                } else {
                    writeln!(f, "- event[{idx}]: {}", event.name)?;
                }
                if options.show_trace_event_details {
                    if let Some(timestamp) = event.timestamp_unix_nano {
                        write_indent(f, options.pretty_indent)?;
                        writeln!(f, "  - timestamp: {timestamp}")?;
                    }
                    if !event.attributes.is_empty() {
                        write_indent(f, options.pretty_indent)?;
                        writeln!(f, "  - attributes:")?;
                        for attr in &event.attributes {
                            write_indent(f, options.pretty_indent)?;
                            writeln!(f, "    - {}: {}", attr.key, attr.value)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn render_stack_trace<State>(
    f: &mut Formatter<'_>,
    report: &Report<impl Error + 'static, State>,
    options: ReportRenderOptions,
) -> fmt::Result
where
    State: SeverityState,
{
    let stack_trace = report.stack_trace();
    let has_stack = stack_trace.is_some();
    if !options.show_stack_trace_section || (!options.show_empty_sections && !has_stack) {
        return Ok(());
    }

    writeln!(f, "Stack Trace:")?;
    let Some(stack_trace) = stack_trace else {
        write_indent(f, options.pretty_indent)?;
        return writeln!(f, "- (none)");
    };

    write_indent(f, options.pretty_indent)?;
    writeln!(f, "- format: {:?}", stack_trace.format)?;
    if options.stack_trace_include_frames && !stack_trace.frames.is_empty() {
        let mut displayed_count = 0;
        for (idx, frame) in filtered_frames(&stack_trace.frames, &options.stack_trace_filter) {
            write_indent(f, options.pretty_indent)?;
            writeln!(
                f,
                "- frame[{idx}]: symbol={:?}, module={:?}, file={:?}, line={:?}, column={:?}",
                frame.symbol, frame.module_path, frame.file, frame.line, frame.column
            )?;
            displayed_count += 1;
            if displayed_count >= options.stack_trace_max_lines {
                let remaining = stack_trace.frames.len() - idx - 1;
                if remaining > 0 {
                    write_indent(f, options.pretty_indent)?;
                    writeln!(f, "- ... {remaining} more frames filtered")?;
                }
                break;
            }
        }
    } else if options.stack_trace_include_raw {
        render_raw_stack_trace(
            f,
            stack_trace,
            options.pretty_indent,
            options.stack_trace_max_lines,
        )?;
    } else {
        write_indent(f, options.pretty_indent)?;
        writeln!(f, "- (hidden by options)")?;
    }
    Ok(())
}

fn render_raw_stack_trace(
    f: &mut Formatter<'_>,
    stack_trace: &crate::report::StackTrace,
    indent: PrettyIndent,
    max_lines: usize,
) -> fmt::Result {
    if let Some(raw) = &stack_trace.raw {
        for line in raw.lines().take(max_lines) {
            write_indent(f, indent)?;
            writeln!(f, "- {line}")?;
        }
        if raw.lines().count() > max_lines {
            write_indent(f, indent)?;
            writeln!(f, "- ... truncated stack trace output")?;
        }
    } else {
        write_indent(f, indent)?;
        writeln!(f, "- (empty)")?;
    }
    Ok(())
}

fn render_attachments<State>(
    f: &mut Formatter<'_>,
    report: &Report<impl Error + 'static, State>,
    options: ReportRenderOptions,
) -> fmt::Result
where
    State: SeverityState,
{
    render_system_section(f, report, options)?;
    render_context_section(f, report, options)?;
    render_attachment_section(f, report, options)?;
    Ok(())
}

fn render_system_section<State>(
    f: &mut Formatter<'_>,
    report: &Report<impl Error + 'static, State>,
    options: ReportRenderOptions,
) -> fmt::Result
where
    State: SeverityState,
{
    if !options.show_context_section {
        return Ok(());
    }

    let mut wrote_header = false;
    let system = report.system();
    for (section_name, section) in system.sections() {
        if render_system_subsection(
            f,
            section_name,
            section,
            options.pretty_indent,
            wrote_header,
        )? {
            wrote_header = true;
        }
    }

    if !wrote_header && options.show_empty_sections {
        writeln!(f, "System:")?;
        write_indent(f, options.pretty_indent)?;
        writeln!(f, "- (none)")?;
    }
    Ok(())
}

fn render_system_subsection(
    f: &mut Formatter<'_>,
    section_name: &str,
    section: &ContextMap,
    indent: PrettyIndent,
    wrote_header: bool,
) -> Result<bool, fmt::Error> {
    if section.is_empty() {
        return Ok(false);
    }

    if !wrote_header {
        writeln!(f, "System:")?;
    }
    write_indent(f, indent)?;
    writeln!(f, "- {section_name}:")?;

    for (key, value) in section.sorted_entries() {
        write_indent(f, indent)?;
        write_indent(f, indent)?;
        writeln!(f, "- {}: {}", key.as_ref(), value)?;
    }
    Ok(true)
}

fn render_context_section<State>(
    f: &mut Formatter<'_>,
    report: &Report<impl Error + 'static, State>,
    options: ReportRenderOptions,
) -> fmt::Result
where
    State: SeverityState,
{
    if !options.show_context_section {
        return Ok(());
    }

    let mut wrote_header = false;
    let context = report.context();
    for (key, value) in context.sorted_entries() {
        if !wrote_header {
            wrote_header = true;
            writeln!(f, "Context:")?;
        }
        write_indent(f, options.pretty_indent)?;
        writeln!(f, "- {}: {}", key.as_ref(), value)?;
    }

    if !wrote_header && options.show_empty_sections {
        writeln!(f, "Context:")?;
        write_indent(f, options.pretty_indent)?;
        writeln!(f, "- (none)")?;
    }
    Ok(())
}

fn render_attachment_section<State>(
    f: &mut Formatter<'_>,
    report: &Report<impl Error + 'static, State>,
    options: ReportRenderOptions,
) -> fmt::Result
where
    State: SeverityState,
{
    if !options.show_attachments_section {
        return Ok(());
    }

    let mut wrote_header = false;
    report.visit_attachments(|item| {
        match item {
            crate::report::AttachmentVisit::Note { message } => {
                if !wrote_header {
                    wrote_header = true;
                    writeln!(f, "Attachments:")?;
                }
                write_indent(f, options.pretty_indent)?;
                writeln!(f, "- note: {message}")?;
            }
            crate::report::AttachmentVisit::Payload {
                name,
                value,
                media_type,
            } => {
                if !wrote_header {
                    wrote_header = true;
                    writeln!(f, "Attachments:")?;
                }
                write_indent(f, options.pretty_indent)?;
                match media_type {
                    Some(media_type) => {
                        writeln!(
                            f,
                            "- payload {} ({}): {}",
                            name.as_ref(),
                            media_type.as_ref(),
                            value
                        )?;
                    }
                    None => {
                        writeln!(f, "- payload {}: {}", name.as_ref(), value)?;
                    }
                }
            }
        }
        Ok(())
    })?;

    if !wrote_header && options.show_empty_sections {
        writeln!(f, "Attachments:")?;
        write_indent(f, options.pretty_indent)?;
        writeln!(f, "- (none)")?;
    }
    Ok(())
}

fn render_display_causes<State>(
    f: &mut Formatter<'_>,
    report: &Report<impl Error + 'static, State>,
    options: ReportRenderOptions,
) -> fmt::Result
where
    State: SeverityState,
{
    if !options.show_cause_chains_section {
        return Ok(());
    }

    let traversal_options = crate::report::CauseCollectOptions {
        max_depth: options.max_source_depth,
        detect_cycle: options.detect_source_cycle,
    };
    let mut count = 0usize;
    let mut wrote_header = false;
    let traversal = report.visit_causes_ext(traversal_options, |cause| {
        if !wrote_header {
            wrote_header = true;
            writeln!(f, "Display Causes:")?;
        }
        count += 1;
        write_indent(f, options.pretty_indent)?;
        writeln!(f, "{}. {}", count, cause)
    })?;

    let should_show_section =
        options.show_empty_sections || count > 0 || traversal.truncated || traversal.cycle_detected;
    if should_show_section && !wrote_header {
        writeln!(f, "Display Causes:")?;
        wrote_header = true;
    }

    if wrote_header && count == 0 && options.show_empty_sections {
        write_indent(f, options.pretty_indent)?;
        writeln!(f, "- (none)")?;
    }
    if wrote_header && traversal.truncated {
        write_indent(f, options.pretty_indent)?;
        writeln!(f, "- ... truncated by max_source_depth")?;
    }
    if wrote_header && traversal.cycle_detected {
        write_indent(f, options.pretty_indent)?;
        writeln!(f, "- ... cycle detected and traversal stopped")?;
    }
    Ok(())
}

fn render_src_errors_section<E, State, F>(
    f: &mut Formatter<'_>,
    report: &Report<E, State>,
    options: ReportRenderOptions,
    title: &str,
    hide_report_wrapper_types: bool,
    source_chain: F,
) -> fmt::Result
where
    E: Error + 'static,
    State: SeverityState,
    F: FnOnce(
        &Report<E, State>,
        crate::report::CauseCollectOptions,
    ) -> Option<crate::report::SourceErrorChain>,
{
    if !options.show_cause_chains_section {
        return Ok(());
    }

    let traversal_options = crate::report::CauseCollectOptions {
        max_depth: options.max_source_depth,
        detect_cycle: options.detect_source_cycle,
    };
    let Some(source_errors) = source_chain(report, traversal_options) else {
        if options.show_empty_sections {
            writeln!(f, "{title}")?;
            write_indent(f, options.pretty_indent)?;
            writeln!(f, "- (none)")?;
        }
        return Ok(());
    };

    if options.show_empty_sections
        || !source_errors.is_empty()
        || source_errors.truncated
        || source_errors.cycle_detected
    {
        writeln!(f, "{title}")?;
    }
    if source_errors.is_empty() {
        if options.show_empty_sections {
            write_indent(f, options.pretty_indent)?;
            writeln!(f, "- (none)")?;
        }
    } else {
        render_source_error_chain(
            f,
            &source_errors,
            options.pretty_indent,
            1,
            options.show_type_name,
            hide_report_wrapper_types,
        )?;
    }
    if source_errors.truncated {
        write_indent(f, options.pretty_indent)?;
        writeln!(f, "- ... truncated by max_source_depth")?;
    }
    if source_errors.cycle_detected {
        write_indent(f, options.pretty_indent)?;
        writeln!(f, "- ... cycle detected and repeated branch skipped")?;
    }
    Ok(())
}

fn render_source_error_chain(
    f: &mut Formatter<'_>,
    source_errors: &crate::report::SourceErrorChain,
    indent: PrettyIndent,
    depth: usize,
    show_type_name: bool,
    hide_report_wrapper_types: bool,
) -> fmt::Result {
    let mut stack = vec![(source_errors.roots_slice(), 0usize, depth)];
    while let Some((ids, mut index, current_depth)) = stack.pop() {
        if index >= ids.len() {
            continue;
        }
        let node_id = ids[index];
        index += 1;
        stack.push((ids, index, current_depth));

        let Some(item) = source_errors.node(node_id) else {
            continue;
        };
        write_depth_indent(f, indent, current_depth)?;
        writeln!(f, "- message: {}", item.error)?;
        if show_type_name {
            let type_name = item.display_type_name(hide_report_wrapper_types);
            write_depth_indent(f, indent, current_depth)?;
            writeln!(f, "- type: {}", type_name.unwrap_or("null"))?;
        }
        let source_ids = item.source_roots.as_slice();
        if !source_ids.is_empty() {
            write_depth_indent(f, indent, current_depth + 1)?;
            writeln!(f, "- source:")?;
            stack.push((source_ids, 0, current_depth + 1));
        }
    }
    Ok(())
}

fn write_depth_indent(f: &mut Formatter<'_>, indent: PrettyIndent, depth: usize) -> fmt::Result {
    for _ in 0..depth {
        write_indent(f, indent)?;
    }
    Ok(())
}
