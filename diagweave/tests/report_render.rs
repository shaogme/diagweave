//! Tests for report rendering options and stack trace filtering.
//!
//! This module contains tests related to:
//! - StackTraceFilter enum behavior
//! - Stack trace frame filtering (All, AppOnly, AppFocused)
//! - ReportRenderOptions presets (developer, production, minimal)

mod report_common;
use diagweave::prelude::*;
use diagweave::render::StackTraceFilter;
use diagweave::report::{StackFrame, StackTrace, StackTraceFormat};
use report_common::*;

#[test]
fn stack_trace_filter_enum_values() {
    let _guard = init_test();

    assert_eq!(StackTraceFilter::All, StackTraceFilter::All);
    assert_eq!(StackTraceFilter::AppOnly, StackTraceFilter::AppOnly);
    assert_eq!(StackTraceFilter::AppFocused, StackTraceFilter::AppFocused);

    assert_ne!(StackTraceFilter::All, StackTraceFilter::AppOnly);
    assert_ne!(StackTraceFilter::AppOnly, StackTraceFilter::AppFocused);
}

#[test]
fn stack_trace_filter_default_is_all() {
    let _guard = init_test();

    let options = ReportRenderOptions::default();
    assert_eq!(options.stack_trace_filter, StackTraceFilter::All);
}

#[test]
fn stack_trace_filter_app_only_removes_std_frames() {
    let _guard = init_test();

    let frames = vec![
        StackFrame {
            symbol: Some("main".into()),
            module_path: Some("app::main".into()),
            file: Some("src/main.rs".into()),
            line: Some(10),
            column: Some(5),
        },
        StackFrame {
            symbol: Some("rust_begin_unwind".into()),
            module_path: Some("std::panicking".into()),
            file: Some("panicking.rs".into()),
            line: Some(100),
            column: Some(1),
        },
        StackFrame {
            symbol: Some("process_abortion".into()),
            module_path: Some("alloc::vec".into()),
            file: Some("vec.rs".into()),
            line: Some(200),
            column: Some(1),
        },
    ];

    let trace = StackTrace::new(StackTraceFormat::Native).with_frames(frames);
    let report = Report::new(ApiError::Unauthorized).with_stack_trace(trace);

    let options = ReportRenderOptions {
        stack_trace_filter: StackTraceFilter::AppOnly,
        stack_trace_max_lines: 10,
        ..ReportRenderOptions::default()
    };

    let pretty = report.render(Pretty::new(options)).to_string();

    assert!(pretty.contains("app::main"));
    assert!(!pretty.contains("std::panicking"));
    assert!(!pretty.contains("alloc::vec"));
}

#[test]
fn stack_trace_filter_app_focused_removes_std_and_internal_frames() {
    let _guard = init_test();

    let frames = vec![
        StackFrame {
            symbol: Some("handler".into()),
            module_path: Some("my_app::handler".into()),
            file: Some("handler.rs".into()),
            line: Some(50),
            column: Some(3),
        },
        StackFrame {
            symbol: Some("report_internal".into()),
            module_path: Some("diagweave::report".into()),
            file: Some("report.rs".into()),
            line: Some(100),
            column: Some(1),
        },
        StackFrame {
            symbol: Some("unwrap".into()),
            module_path: Some("core::panicking".into()),
            file: Some("panicking.rs".into()),
            line: Some(300),
            column: Some(1),
        },
    ];

    let trace = StackTrace::new(StackTraceFormat::Native).with_frames(frames);
    let report = Report::new(ApiError::Unauthorized).with_stack_trace(trace);

    let options = ReportRenderOptions {
        stack_trace_filter: StackTraceFilter::AppFocused,
        stack_trace_max_lines: 10,
        ..ReportRenderOptions::default()
    };

    let pretty = report.render(Pretty::new(options)).to_string();

    assert!(pretty.contains("my_app::handler"));
    assert!(!pretty.contains("diagweave::report"));
    assert!(!pretty.contains("core::panicking"));
}

#[test]
fn stack_trace_max_lines_limits_displayed_frames() {
    let _guard = init_test();

    let frames: Vec<StackFrame> = (0..20)
        .map(|i| StackFrame {
            symbol: Some(format!("func_{}", i).into()),
            module_path: Some("app::module".into()),
            file: Some("src/lib.rs".into()),
            line: Some(i * 10),
            column: Some(1),
        })
        .collect();

    let trace = StackTrace::new(StackTraceFormat::Native).with_frames(frames);
    let report = Report::new(ApiError::Unauthorized).with_stack_trace(trace);

    let options = ReportRenderOptions {
        stack_trace_filter: StackTraceFilter::All,
        stack_trace_max_lines: 5,
        ..ReportRenderOptions::default()
    };

    let pretty = report.render(Pretty::new(options)).to_string();

    assert!(pretty.contains("func_0"));
    assert!(pretty.contains("func_4"));
    assert!(pretty.contains("more frames filtered"));
    assert!(!pretty.contains("func_5"));
}

#[test]
fn report_render_options_developer_preset() {
    let _guard = init_test();

    let options = ReportRenderOptions::developer();

    assert!(options.show_trace_event_details);
    assert_eq!(options.stack_trace_filter, StackTraceFilter::All);
    assert_eq!(options.stack_trace_max_lines, 50);
}

#[test]
fn report_render_options_production_preset() {
    let _guard = init_test();

    let options = ReportRenderOptions::production();

    assert!(options.show_trace_event_details);
    assert_eq!(options.stack_trace_filter, StackTraceFilter::AppOnly);
    assert_eq!(options.stack_trace_max_lines, 15);
}

#[test]
fn report_render_options_minimal_preset() {
    let _guard = init_test();

    let options = ReportRenderOptions::minimal();

    assert!(!options.show_trace_event_details);
    assert_eq!(options.stack_trace_filter, StackTraceFilter::AppFocused);
    assert_eq!(options.stack_trace_max_lines, 5);
    assert!(!options.show_empty_sections);
    assert!(!options.show_type_name);
}
