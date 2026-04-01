mod report_common;
use diagweave::prelude::*;
use diagweave::render::{PrettyIndent, StackTraceFilter};
use diagweave::report::CauseCollectOptions;
use diagweave::report::{Attachment, ContextValue, StackFrame, StackTrace, StackTraceFormat};
use report_common::*;
use std::collections::BTreeMap;
use std::error::Error;
#[cfg(feature = "std")]
use std::sync::atomic::Ordering;

#[test]
fn metadata_and_attachments_are_recorded_and_formatted() {
    let _guard = init_test();

    let mut payload = BTreeMap::new();
    payload.insert("method".to_owned(), AttachmentValue::from("password"));
    payload.insert("attempt".to_owned(), AttachmentValue::Unsigned(2));

    let report = Report::new(AuthError::InvalidToken)
        .with_error_code("AUTH.INVALID_TOKEN")
        .with_severity(Severity::Warn)
        .with_category("auth")
        .with_retryable(false)
        .with_ctx("request_id", "tx-100")
        .attach_printable("check authorization flow")
        .attach_payload(
            "auth_payload",
            AttachmentValue::from(payload),
            Some("application/json"),
        );

    assert_eq!(report.context().map_or(0, |ctx| ctx.len()), 1);
    assert_eq!(report.attachments().len(), 2);
    assert_eq!(
        report
            .context()
            .and_then(|ctx| ctx.iter().next())
            .map(|(key, value)| (key.as_ref().to_owned(), value.clone())),
        Some((
            "request_id".to_owned(),
            ContextValue::String("tx-100".into())
        ))
    );
    assert_eq!(
        report.attachments()[0].as_note(),
        Some("check authorization flow".to_owned())
    );
    assert!(report.attachments()[1].as_payload().is_some());
    assert_eq!(
        report.metadata().error_code().map(ToString::to_string),
        Some("AUTH.INVALID_TOKEN".to_owned())
    );
    assert_eq!(
        report.to_string(),
        "auth invalid token [code=AUTH.INVALID_TOKEN, severity=warn, category=auth, retryable=false, request_id=tx-100, check authorization flow, auth_payload={attempt: 2, method: password} (application/json)]"
    );
}

#[test]
fn diagweave_wraps_previous_report_as_source() {
    let _guard = init_test();

    let inner = Report::new(AuthError::InvalidToken)
        .with_error_code("AUTH.INVALID_TOKEN")
        .with_ctx("request_id", "tx-2");
    let outer = inner.boundary(ApiError::Unauthorized);

    assert_eq!(outer.to_string(), "api unauthorized");
    let source = outer.source().expect("diagweave should preserve source");
    assert_eq!(
        source.to_string(),
        "auth invalid token [code=AUTH.INVALID_TOKEN, request_id=tx-2]"
    );
}

#[test]
fn diagweave_with_changes_context_and_keeps_metadata() {
    let _guard = init_test();

    let outer = Report::new(AuthError::InvalidToken)
        .with_error_code("AUTH.INVALID_TOKEN")
        .with_ctx("request_id", "tx-9")
        .map_err(|_| ApiError::Wrapped { code: 401 });

    assert_eq!(
        outer.to_string(),
        "api wrapped code=401 [code=AUTH.INVALID_TOKEN, request_id=tx-9]"
    );
    assert!(outer.source().is_none());
}

fn fail_auth() -> Result<(), AuthError> {
    Err(AuthError::InvalidToken)
}

#[test]
fn error_value_diag_is_supported() {
    let _guard = init_test();

    let report = Report::new(AuthError::InvalidToken).with_error_code("AUTH.INVALID_TOKEN");
    assert_eq!(
        report.to_string(),
        "auth invalid token [code=AUTH.INVALID_TOKEN]"
    );
}

#[test]
#[cfg(debug_assertions)]
fn report_debug_is_pretty_like_in_debug_profile() {
    let _guard = init_test();

    let debug_text = format!(
        "{:?}",
        Report::new(AuthError::InvalidToken)
            .with_error_code("AUTH.INVALID_TOKEN")
            .with_ctx("request_id", "tx-debug")
    );
    assert!(debug_text.contains("Report:"));
    assert!(debug_text.contains("attachments:"));
    assert!(debug_text.contains("display_causes:"));
}

#[test]
#[cfg(not(debug_assertions))]
fn report_debug_is_compact_in_release_profile() {
    let _guard = init_test();

    let debug_text = format!("{:?}", Report::new(AuthError::InvalidToken));
    assert!(debug_text.starts_with("Report {"));
}

#[test]
fn result_ext_builds_report_chain() {
    let _guard = init_test();

    let err = fail_auth()
        .diag()
        .with_ctx("request_id", 77u64)
        .with_error_code("AUTH.INVALID_TOKEN")
        .boundary(ApiError::Unauthorized)
        .expect_err("should fail");

    assert_eq!(err.to_string(), "api unauthorized");
    let source = err.source().expect("outer should have source");
    assert_eq!(
        source.to_string(),
        "auth invalid token [code=AUTH.INVALID_TOKEN, request_id=77]"
    );
}

#[test]
fn result_ext_attach_payload_accepts_dynamic_media_type() {
    let _guard = init_test();

    let media_type = "application/json".to_owned();
    let err = fail_auth()
        .diag()
        .attach_payload("body", AttachmentValue::from("ok"), Some(media_type))
        .expect_err("should fail");

    assert!(matches!(
        &err.attachments()[0],
        Attachment::Payload {
            name,
            value: AttachmentValue::String(value),
            media_type: Some(media_type),
        } if name == "body"
            && value == "ok"
            && media_type == "application/json"
    ));
}

#[test]
#[cfg(feature = "std")]
fn global_context_injector_applies_to_new_reports() {
    let _guard = init_test();
    ensure_global_injector_installed();

    struct InjectGuard;
    impl Drop for InjectGuard {
        fn drop(&mut self) {
            INJECT_ENABLED.store(false, Ordering::Relaxed);
        }
    }
    let _inject_guard = InjectGuard;

    INJECT_ENABLED.store(true, Ordering::Relaxed);

    let report = Report::new(AuthError::InvalidToken);

    assert_eq!(report.context().map_or(0, |ctx| ctx.len()), 1);
    assert_eq!(
        report
            .context()
            .and_then(|ctx| ctx.iter().next())
            .map(|(key, value)| (key.as_ref().to_owned(), value.clone())),
        Some((
            "request_id".to_owned(),
            ContextValue::String("req-42".into())
        ))
    );
    #[cfg(feature = "trace")]
    {
        let trace = report.trace().expect("trace should be injected");
        assert_eq!(
            trace.context.trace_id.as_ref().map(|v| v.as_ref()),
            Some("4bf92f3577b34da6a3ce929d0e0e4736")
        );
        assert_eq!(
            trace.context.span_id.as_ref().map(|v| v.as_ref()),
            Some("00f067aa0ba902b7")
        );
    }
}

#[test]
#[cfg(feature = "std")]
fn global_context_injector_can_be_disabled_by_user_logic() {
    let _guard = init_test();
    ensure_global_injector_installed();
    INJECT_ENABLED.store(false, Ordering::Relaxed);

    let report = Report::new(AuthError::InvalidToken);
    assert!(
        report
            .context()
            .into_iter()
            .flat_map(|map| map.iter())
            .all(|(key, _)| key.as_ref() != "request_id")
    );
}

#[test]
fn result_ext_diagweave_with_maps_error() {
    let _guard = init_test();

    let err = fail_auth()
        .diag()
        .attach_note("incoming token is stale")
        .with_category("auth")
        .map_inner(|_| ApiError::Wrapped { code: 403 })
        .expect_err("should fail");

    assert_eq!(
        err.to_string(),
        "api wrapped code=403 [category=auth, incoming token is stale]"
    );
    assert!(err.source().is_none());
}

#[test]
fn lazy_context_and_note_evaluate_only_on_error() {
    let _guard = init_test();

    let ok: Result<(), Report<AuthError>> = Ok(());
    let counter = std::cell::Cell::new(0usize);
    let _ = ok
        .with_ctx_lazy("hot_path", || {
            counter.set(counter.get() + 1);
            ContextValue::Bool(true)
        })
        .attach_note_lazy(|| {
            counter.set(counter.get() + 1);
            "should not allocate".to_owned()
        });
    assert_eq!(counter.get(), 0);

    let err = fail_auth()
        .diag()
        .with_ctx_lazy("retry", || ContextValue::Unsigned(3))
        .attach_note_lazy(|| "token stale".to_owned())
        .expect_err("must fail");
    assert_eq!(err.to_string(), "auth invalid token [retry=3, token stale]");
}

#[test]
fn pretty_output_is_structured() {
    let _guard = init_test();

    let pretty = Report::new(AuthError::InvalidToken)
        .with_error_code("AUTH.INVALID_TOKEN")
        .with_severity(Severity::Error)
        .with_ctx("request_id", "tx-pretty")
        .attach_payload(
            "raw_body",
            AttachmentValue::Bytes(vec![1, 2, 3]),
            Some("application/octet-stream".to_owned()),
        )
        .boundary(ApiError::Unauthorized)
        .pretty()
        .to_string();

    assert!(pretty.contains("Error:"));
    assert!(pretty.contains("  - message: api unauthorized"));
    assert!(pretty.contains("Governance:"));
    assert!(pretty.contains("Context:"));
    assert!(pretty.contains("Attachments:"));
    assert!(pretty.contains("Source Errors:"));
    assert!(pretty.contains("auth invalid token [code=AUTH.INVALID_TOKEN, severity=error, request_id=tx-pretty, raw_body=<3 bytes> (application/octet-stream)]"));
}

#[test]
fn pretty_indents_nested_source_errors() {
    let _guard = init_test();

    let pretty = Report::new(ApiError::Unauthorized)
        .boundary(ApiError::Wrapped { code: 500 })
        .boundary(ApiError::Wrapped { code: 501 })
        .pretty()
        .to_string();

    assert!(pretty.contains("  - source:\n    - message:"));
}

#[test]
fn pretty_respects_max_source_depth() {
    let _guard = init_test();

    let report = Report::new(AuthError::InvalidToken)
        .boundary(ApiError::Unauthorized)
        .boundary(ApiError::Wrapped { code: 500 });

    let options = ReportRenderOptions {
        max_source_depth: 1,
        detect_source_cycle: true,
        ..ReportRenderOptions::default()
    };
    let pretty = report.render(Pretty::new(options)).to_string();
    assert!(pretty.contains("truncated by max_source_depth"));
}

#[test]
fn pretty_stops_on_cycle() {
    let _guard = init_test();

    let report = Report::new(LoopError);
    let pretty = report
        .render(Pretty::new(ReportRenderOptions::default()))
        .to_string();
    assert!(pretty.contains("cycle detected and repeated branch skipped"));
}

#[test]
fn pretty_can_hide_type_and_empty_sections_and_change_indent() {
    let _guard = init_test();

    let options = ReportRenderOptions {
        max_source_depth: 16,
        detect_source_cycle: true,
        pretty_indent: PrettyIndent::Spaces(4),
        show_type_name: false,
        show_empty_sections: false,
        show_governance_section: true,
        show_trace_section: true,
        show_trace_event_details: true,
        show_stack_trace_section: true,
        show_context_section: true,
        show_attachments_section: true,
        show_cause_chains_section: true,
        stack_trace_max_lines: 24,
        stack_trace_include_raw: true,
        stack_trace_include_frames: true,
        stack_trace_filter: StackTraceFilter::All,
        json_pretty: false,
    };
    let pretty = Report::new(AuthError::InvalidToken)
        .render(Pretty::new(options))
        .to_string();
    assert!(pretty.contains("Error:"));
    assert!(pretty.contains("    - message: auth invalid token"));
    assert!(!pretty.contains("  - type:"));
    assert!(!pretty.contains("Governance:"));
    assert!(!pretty.contains("Context:"));
    assert!(!pretty.contains("Attachments:"));
    assert!(!pretty.contains("Display Causes:"));
    assert!(!pretty.contains("Source Errors:"));
}

#[test]
fn pretty_can_hide_type_names_in_source_chains() {
    let _guard = init_test();

    let pretty = Report::new(ApiError::Unauthorized)
        .with_diag_src_err(AuthError::InvalidToken)
        .render(Pretty::new(ReportRenderOptions {
            show_type_name: false,
            show_cause_chains_section: true,
            show_empty_sections: true,
            ..ReportRenderOptions::default()
        }))
        .to_string();

    assert!(pretty.contains("Source Errors:"));
    assert!(pretty.contains("auth invalid token"));
    assert!(!pretty.contains("- type:"));
}

#[test]
fn custom_renderer_trait_is_supported() {
    let _guard = init_test();

    let report = Report::new(AuthError::InvalidToken);
    let rendered = report.render(TinyRenderer).to_string();
    assert_eq!(rendered, "tiny: auth invalid token");
}

#[test]
fn stack_trace_metadata_api_works() {
    let _guard = init_test();

    let trace = StackTrace::new(StackTraceFormat::Raw).with_raw("frame-a\\nframe-b");
    let report = Report::new(ApiError::Unauthorized).with_stack_trace(trace.clone());
    assert_eq!(report.stack_trace(), Some(&trace));
    assert_eq!(report.to_string(), "api unauthorized [stack_trace=present]");

    let cleared = report.clear_stack_trace();
    assert!(cleared.stack_trace().is_none());
}

#[test]
fn report_field_getters_are_exposed() {
    let _guard = init_test();

    let report = Report::new(AuthError::InvalidToken)
        .with_error_code("AUTH.INVALID_TOKEN")
        .with_severity(Severity::Warn)
        .with_category("auth")
        .with_retryable(false);

    assert_eq!(
        report.error_code().map(ToString::to_string),
        Some("AUTH.INVALID_TOKEN".to_owned())
    );
    assert_eq!(report.severity(), Some(Severity::Warn));
    assert_eq!(report.severity(), Some(Severity::Warn));
    assert_eq!(report.category(), Some("auth"));
    assert_eq!(report.retryable(), Some(false));
}

#[test]
fn public_cause_visit_apis_are_accessible() {
    let _guard = init_test();

    let report = Report::new(AuthError::InvalidToken)
        .with_display_cause("token stale")
        .with_diag_src_err(ApiError::Unauthorized);
    let mut display = Vec::new();
    let display_state = report
        .visit_causes(|cause| {
            display.push(cause.to_string());
            Ok(())
        })
        .expect("display causes");
    let mut origin_source = Vec::new();
    let origin_source_state = report
        .visit_origin_sources(|err| {
            origin_source.push(err.message);
            Ok(())
        })
        .expect("origin source errors");
    let mut diagnostic_source = Vec::new();
    let diagnostic_source_state = report
        .visit_diag_sources(|err| {
            diagnostic_source.push(err.message);
            Ok(())
        })
        .expect("diagnostic source errors");

    assert_eq!(display, vec!["token stale".to_owned()]);
    assert!(origin_source.is_empty());
    assert_eq!(diagnostic_source, vec!["api unauthorized".to_owned()]);
    assert!(!display_state.truncated);
    assert!(!display_state.cycle_detected);
    assert!(!origin_source_state.truncated);
    assert!(!origin_source_state.cycle_detected);
    assert!(!diagnostic_source_state.truncated);
    assert!(!diagnostic_source_state.cycle_detected);

    let cycle = Report::new(LoopError)
        .visit_origin_src_ext(
            CauseCollectOptions {
                max_depth: 8,
                detect_cycle: true,
            },
            |_| Ok(()),
        )
        .expect("cycle traversal");
    assert!(cycle.cycle_detected);

    let mut iter = report.iter_diag_srcs_ext(CauseCollectOptions {
        max_depth: 4,
        detect_cycle: true,
    });
    let collected: Vec<String> = iter.by_ref().map(|err| err.message).collect();
    let iter_state = iter.state();
    assert_eq!(collected, vec!["api unauthorized".to_owned()]);
    assert!(!iter_state.truncated);
    assert!(!iter_state.cycle_detected);
}

#[test]
fn source_iteration_can_disable_cycle_detection() {
    let _guard = init_test();

    let report = Report::new(LoopError);
    let mut iter = report.iter_origin_src_ext(CauseCollectOptions {
        max_depth: 4,
        detect_cycle: false,
    });
    let collected: Vec<String> = iter.by_ref().map(|err| err.message).collect();
    let iter_state = iter.state();

    assert_eq!(
        collected,
        vec![
            "loop error".to_owned(),
            "loop error".to_owned(),
            "loop error".to_owned(),
            "loop error".to_owned(),
        ]
    );
    assert!(iter_state.truncated);
    assert!(!iter_state.cycle_detected);
}

#[test]
fn wrap_keeps_explicit_source_chain_isolated_from_inner_source() {
    let _guard = init_test();

    #[derive(Debug)]
    struct NaturalSourceError;

    impl std::fmt::Display for NaturalSourceError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "natural source")
        }
    }

    impl Error for NaturalSourceError {}

    #[derive(Debug)]
    struct SourcefulError;

    impl std::fmt::Display for SourcefulError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "sourceful error")
        }
    }

    impl Error for SourcefulError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&NATURAL_SOURCE)
        }
    }

    static NATURAL_SOURCE: NaturalSourceError = NaturalSourceError;

    let report = Report::new(SourcefulError)
        .with_diag_src_err(ApiError::Unauthorized)
        .boundary(ApiError::Wrapped { code: 500 });

    let messages: Vec<String> = report
        .iter_origin_src_ext(CauseCollectOptions {
            max_depth: 8,
            detect_cycle: true,
        })
        .map(|entry| entry.message)
        .collect();

    assert!(!messages.iter().any(|message| message == "api unauthorized"));
    assert!(messages.iter().any(|message| message == "natural source"));
}

#[test]
fn source_iteration_keeps_top_level_siblings_at_same_depth() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_diag_src_err(AuthError::InvalidToken)
        .with_diag_src_err(std::io::Error::other("network down"));
    let collected: Vec<(String, usize)> = report
        .iter_diag_srcs_ext(CauseCollectOptions {
            max_depth: 4,
            detect_cycle: true,
        })
        .map(|err| (err.message, err.depth))
        .collect();

    assert_eq!(
        collected,
        vec![
            ("auth invalid token".to_owned(), 0),
            ("network down".to_owned(), 0),
        ]
    );
}

#[test]
fn source_iteration_keeps_siblings_after_truncation() {
    let _guard = init_test();

    let deep_branch = Report::new(AuthError::InvalidToken).boundary(ApiError::Unauthorized);
    let report = Report::new(ApiError::Wrapped { code: 400 })
        .with_diag_src_err(deep_branch)
        .with_diag_src_err(std::io::Error::other("network down"));

    let collected: Vec<String> = report
        .iter_diag_srcs_ext(CauseCollectOptions {
            max_depth: 1,
            detect_cycle: true,
        })
        .map(|err| err.message)
        .collect();

    assert!(collected.iter().any(|message| message == "network down"));
}

#[test]
fn source_errors_iterator_only_uses_attached_chain() {
    let _guard = init_test();

    #[derive(Debug)]
    struct NaturalSourceError;

    impl std::fmt::Display for NaturalSourceError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "natural source")
        }
    }

    impl Error for NaturalSourceError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&NATURAL_SOURCE)
        }
    }

    static NATURAL_SOURCE: NaturalSourceError = NaturalSourceError;

    let report = Report::new(NaturalSourceError).with_diag_src_err(AuthError::InvalidToken);
    let collected: Vec<String> = report.diag_source_errors().map(|err| err.message).collect();

    assert_eq!(collected, vec!["auth invalid token".to_owned()]);
}

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
