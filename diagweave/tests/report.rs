mod report_common;
use diagweave::prelude::*;
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
        .attach("request_id", "tx-100")
        .attach_printable("check authorization flow")
        .attach_payload(
            "auth_payload",
            AttachmentValue::Object(payload),
            Some("application/json".to_owned()),
        );

    assert_eq!(report.attachments().len(), 3);
    assert!(matches!(
        &report.attachments()[0],
        Attachment::Context { key, value: AttachmentValue::String(value) }
            if key == "request_id" && value == "tx-100"
    ));
    assert_eq!(
        report.attachments()[0].as_context(),
        Some(("request_id", &AttachmentValue::String("tx-100".to_owned())))
    );
    assert_eq!(
        report.attachments()[1].as_note(),
        Some("check authorization flow")
    );
    assert!(report.attachments()[2].as_payload().is_some());
    assert_eq!(
        report.metadata().error_code.as_deref(),
        Some("AUTH.INVALID_TOKEN")
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
        .attach("request_id", "tx-2");
    let outer = inner.wrap(ApiError::Unauthorized);

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
        .attach("request_id", "tx-9")
        .wrap_with(|_| ApiError::Wrapped { code: 401 });

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
            .attach("request_id", "tx-debug")
    );
    assert!(debug_text.contains("Report:"));
    assert!(debug_text.contains("attachments:"));
    assert!(debug_text.contains("cause_store:"));
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
        .with_context("request_id", 77u64)
        .with_error_code("AUTH.INVALID_TOKEN")
        .wrap(ApiError::Unauthorized)
        .expect_err("should fail");

    assert_eq!(err.to_string(), "api unauthorized");
    let source = err.source().expect("outer should have source");
    assert_eq!(
        source.to_string(),
        "auth invalid token [code=AUTH.INVALID_TOKEN, request_id=77]"
    );
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

    assert!(matches!(
        &report.attachments()[0],
        Attachment::Context { key, value: AttachmentValue::String(value) }
            if key == "request_id" && value == "req-42"
    ));
    #[cfg(feature = "trace")]
    {
        let trace = report.trace().expect("trace should be injected");
        assert_eq!(trace.context.trace_id.as_deref(), Some("trace-42"));
        assert_eq!(trace.context.span_id.as_deref(), Some("span-42"));
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
            .attachments()
            .iter()
            .all(|attachment| attachment.as_context().map(|(k, _)| k) != Some("request_id"))
    );
}

#[test]
fn result_ext_diagweave_with_maps_error() {
    let _guard = init_test();

    let err = fail_auth()
        .diag()
        .with_note("incoming token is stale")
        .with_category("auth")
        .wrap_with(|_| ApiError::Wrapped { code: 403 })
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
        .context_lazy("hot_path", || {
            counter.set(counter.get() + 1);
            AttachmentValue::Bool(true)
        })
        .note_lazy(|| {
            counter.set(counter.get() + 1);
            "should not allocate".to_owned()
        });
    assert_eq!(counter.get(), 0);

    let err = fail_auth()
        .diag()
        .context_lazy("retry", || AttachmentValue::Unsigned(3))
        .note_lazy(|| "token stale".to_owned())
        .expect_err("must fail");
    assert_eq!(err.to_string(), "auth invalid token [retry=3, token stale]");
}

#[test]
fn pretty_output_is_structured() {
    let _guard = init_test();

    let pretty = Report::new(AuthError::InvalidToken)
        .with_error_code("AUTH.INVALID_TOKEN")
        .with_severity(Severity::Error)
        .attach("request_id", "tx-pretty")
        .attach_payload(
            "raw_body",
            AttachmentValue::Bytes(vec![1, 2, 3]),
            Some("application/octet-stream".to_owned()),
        )
        .wrap(ApiError::Unauthorized)
        .pretty()
        .to_string();

    assert!(pretty.contains("Error:"));
    assert!(pretty.contains("  - message: api unauthorized"));
    assert!(pretty.contains("Governance:"));
    assert!(pretty.contains("Context:"));
    assert!(pretty.contains("Attachments:"));
    assert!(pretty.contains("Error Sources:"));
    assert!(pretty.contains("auth invalid token [code=AUTH.INVALID_TOKEN, severity=error, request_id=tx-pretty, raw_body=<3 bytes> (application/octet-stream)]"));
}

#[test]
fn pretty_respects_max_source_depth() {
    let _guard = init_test();

    let report = Report::new(AuthError::InvalidToken)
        .wrap(ApiError::Unauthorized)
        .wrap(ApiError::Wrapped { code: 500 });

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
    assert!(pretty.contains("cycle detected and traversal stopped"));
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
        show_stack_trace_section: true,
        show_context_section: true,
        show_attachments_section: true,
        show_causes_section: true,
        stack_trace_max_lines: 24,
        stack_trace_include_raw: true,
        stack_trace_include_frames: true,
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
    assert!(!pretty.contains("Error Sources:"));
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
fn pretty_options_can_hide_specific_sections() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_error_code("API.UNAUTHORIZED")
        .attach("request_id", "req-sec-1");
    let opts = ReportRenderOptions {
        show_empty_sections: true,
        show_governance_section: false,
        show_context_section: false,
        show_attachments_section: false,
        show_causes_section: false,
        ..ReportRenderOptions::default()
    };
    let pretty = report.render(Pretty::new(opts)).to_string();
    assert!(!pretty.contains("Governance:"));
    assert!(!pretty.contains("Context:"));
    assert!(!pretty.contains("Attachments:"));
    assert!(!pretty.contains("Display Causes:"));
    assert!(!pretty.contains("Error Sources:"));
}
