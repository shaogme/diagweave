mod report_common;
use diagweave::prelude::*;
use diagweave::render::{PrettyIndent, StackTraceFilter};
use diagweave::report::{Attachment, ContextValue, StackTrace, StackTraceFormat};
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

    assert_eq!(report.context().len(), 1);
    assert_eq!(report.attachments().len(), 2);
    assert_eq!(
        report
            .context()
            .iter()
            .next()
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
    let outer = inner.map_err(|_| ApiError::Unauthorized);

    // Now map_err accumulates source chain and preserves metadata
    assert_eq!(
        outer.to_string(),
        "api unauthorized [code=AUTH.INVALID_TOKEN, request_id=tx-2]"
    );
    let source = outer.source().expect("diagweave should preserve source");
    assert_eq!(source.to_string(), "auth invalid token");
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
    // Now map_err accumulates source chain by default
    assert!(outer.source().is_some());
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
        .diag(|r| {
            r.with_ctx("request_id", 77u64)
                .with_error_code("AUTH.INVALID_TOKEN")
                .map_err(|_| ApiError::Unauthorized)
        })
        .expect_err("should fail");

    // Metadata is propagated to the outer error
    assert_eq!(
        err.to_string(),
        "api unauthorized [code=AUTH.INVALID_TOKEN, request_id=77]"
    );
    let source = err.source().expect("outer should have source");
    assert_eq!(source.to_string(), "auth invalid token");
}

#[test]
fn result_ext_attach_payload_accepts_dynamic_media_type() {
    let _guard = init_test();

    let media_type = "application/json".to_owned();
    let err = fail_auth()
        .diag(|r| r.attach_payload("body", AttachmentValue::from("ok"), Some(media_type)))
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

    assert_eq!(report.context().len(), 1);
    assert_eq!(
        report
            .context()
            .iter()
            .next()
            .map(|(key, value)| (key.as_ref().to_owned(), value.clone())),
        Some((
            "request_id".to_owned(),
            ContextValue::String("req-42".into())
        ))
    );
    #[cfg(feature = "trace")]
    {
        let trace = report.trace();
        assert!(!trace.is_empty(), "trace should be injected");
        assert_eq!(
            trace
                .context()
                .and_then(|ctx| ctx.trace_id.as_ref().map(|v| v.as_ref())),
            Some("4bf92f3577b34da6a3ce929d0e0e4736")
        );
        assert_eq!(
            trace
                .context()
                .and_then(|ctx| ctx.span_id.as_ref().map(|v| v.as_ref())),
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
            .iter()
            .all(|(key, _)| key.as_ref() != "request_id")
    );
}

#[test]
fn result_ext_diagweave_with_maps_error() {
    let _guard = init_test();

    let err = fail_auth()
        .diag(|r| {
            r.attach_note("incoming token is stale")
                .with_category("auth")
                .map_err(|_| ApiError::Wrapped { code: 403 })
        })
        .expect_err("should fail");

    assert_eq!(
        err.to_string(),
        "api wrapped code=403 [category=auth, incoming token is stale]"
    );
    // map_err accumulates source chain by default
    assert!(err.source().is_some());
}

#[test]
fn lazy_context_and_note_evaluate_only_on_error() {
    let _guard = init_test();

    let ok: Result<(), Report<AuthError>> = Ok(());
    let counter = std::cell::Cell::new(0usize);
    let _ = ok
        .and_then_report(|r| {
            counter.set(counter.get() + 1);
            r.with_ctx("hot_path", ContextValue::Bool(true))
        })
        .and_then_report(|r| {
            counter.set(counter.get() + 1);
            r.attach_note("should not allocate")
        });
    assert_eq!(counter.get(), 0);

    let err = fail_auth()
        .diag(|r| {
            r.with_ctx("retry", ContextValue::Unsigned(3))
                .attach_note("token stale")
        })
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
        .map_err(|_| ApiError::Unauthorized)
        .pretty()
        .to_string();

    assert!(pretty.contains("Error:"));
    assert!(pretty.contains(" - message: api unauthorized"));
    assert!(pretty.contains("Governance:"));
    assert!(pretty.contains("- error_code: AUTH.INVALID_TOKEN"));
    assert!(pretty.contains("- severity: error"));
    assert!(pretty.contains("Context:"));
    assert!(pretty.contains("- request_id: tx-pretty"));
    assert!(pretty.contains("Attachments:"));
    assert!(pretty.contains("payload raw_body (application/octet-stream): <3 bytes>"));
    assert!(pretty.contains("Origin Source Errors:"));
    assert!(pretty.contains("- message: auth invalid token"));
}

#[test]
fn pretty_indents_nested_source_errors() {
    let _guard = init_test();

    let pretty = Report::new(ApiError::Unauthorized)
        .map_err(|_| ApiError::Wrapped { code: 500 })
        .map_err(|_| ApiError::Wrapped { code: 501 })
        .pretty()
        .to_string();

    assert!(pretty.contains("  - source:\n    - message:"));
}

#[test]
fn pretty_respects_max_source_depth() {
    let _guard = init_test();

    let report = Report::new(AuthError::InvalidToken)
        .map_err(|_| ApiError::Unauthorized)
        .map_err(|_| ApiError::Wrapped { code: 500 });

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
