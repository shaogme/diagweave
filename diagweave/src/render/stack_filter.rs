use super::StackTraceFilter;
use crate::report::StackFrame;

#[inline]
pub fn should_filter_frame(frame: &StackFrame, filter: &StackTraceFilter) -> bool {
    match filter {
        StackTraceFilter::All => false,
        StackTraceFilter::AppOnly => is_std_or_runtime_frame(frame),
        StackTraceFilter::AppFocused => is_std_or_runtime_frame(frame) || is_internal_frame(frame),
    }
}

#[inline]
pub fn is_std_or_runtime_frame(frame: &StackFrame) -> bool {
    frame.module_path.as_ref().is_some_and(|m| {
        m.starts_with("std::")
            || m.starts_with("core::")
            || m.starts_with("alloc::")
            || m.starts_with("backtrace::")
            || m.contains("rust_begin_unwind")
            || m.contains("rust_panic")
    })
}

#[inline]
pub fn is_internal_frame(frame: &StackFrame) -> bool {
    frame.module_path.as_ref().is_some_and(|m| {
        m.starts_with("diagweave::") || m.contains("diagnostic") || m.contains("report")
    })
}

pub fn filtered_frames<'a>(
    frames: &'a [StackFrame],
    filter: &StackTraceFilter,
) -> impl Iterator<Item = (usize, &'a StackFrame)> {
    frames
        .iter()
        .enumerate()
        .filter(move |(_, f)| !should_filter_frame(f, filter))
}
