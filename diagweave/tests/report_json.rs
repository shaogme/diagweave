mod report_common;
#[cfg(feature = "json")]
use diagweave::prelude::*;
#[cfg(feature = "json")]
use report_common::*;

#[cfg(feature = "json")]
#[test]
fn render_format_supports_compact_pretty_and_json() {
    let report = Report::new(AuthError::InvalidToken)
        .with_error_code("AUTH.INVALID_TOKEN")
        .with_retryable(true)
        .attach("request_id", "tx-json")
        .attach_payload(
            "http.request",
            AttachmentValue::Array(vec![
                AttachmentValue::from("GET"),
                AttachmentValue::from("/session"),
            ]),
            Some("application/x.debug".to_owned()),
        )
        .wrap(ApiError::Unauthorized);

    let _guard = init_test();

    let compact = report.render(Compact).to_string();
    assert_eq!(compact, "api unauthorized");

    let pretty = report
        .render(Pretty::new(ReportRenderOptions::default()))
        .to_string();
    assert!(pretty.contains("Governance:"));

    {
        let json = report
            .render(Json::new(ReportRenderOptions::default()))
            .to_string();
        assert!(json.contains("\"schema_version\""));
        assert!(json.contains("\"v0.1.0\""));
        assert!(json.contains("\"error\""));
        assert!(json.contains("\"metadata\""));
        assert!(json.contains("\"diagnostic_bag\""));
        assert!(json.contains("\"context\""));
        assert!(json.contains("\"attachments\""));
        assert!(json.contains("\"stack_trace\""));
        assert!(json.contains("\"display_causes\""));
        assert!(json.contains("\"source_errors\""));

        let parsed: serde_json::Value = serde_json::from_str(&json).expect("json schema shape");
        assert_eq!(parsed["schema_version"], REPORT_JSON_SCHEMA_VERSION);
        assert_eq!(parsed["error"]["message"], "api unauthorized");
        assert!(parsed["metadata"]["error_code"].is_null());
        assert!(parsed["metadata"]["retryable"].is_null());
        assert!(parsed["diagnostic_bag"]["stack_trace"].is_null());
        assert!(parsed["diagnostic_bag"]["display_causes"].is_null());
        assert!(parsed["diagnostic_bag"]["source_errors"].is_object());
        #[cfg(feature = "trace")]
        assert!(parsed["trace"].is_null());
        assert_eq!(parsed["attachments"].as_array().map(|a| a.len()), Some(0));
    }
}

#[cfg(feature = "json")]
#[test]
fn json_schema_document_is_exposed() {
    let schema = report_json_schema();
    assert!(schema.contains("\"$schema\""));
    assert!(schema.contains(REPORT_JSON_SCHEMA_VERSION));
    assert!(schema.contains("\"metadata\""));
}

#[cfg(feature = "json")]
#[test]
fn json_document_carries_metadata_and_structured_attachments() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_error_code("API.UNAUTHORIZED")
        .with_severity(Severity::Error)
        .with_category("auth")
        .with_retryable(false)
        .attach("request_id", "req-json")
        .attach_printable("token rejected")
        .attach_payload(
            "response",
            AttachmentValue::Bytes(vec![7, 8, 9]),
            Some("application/octet-stream".to_owned()),
        );

    let json = report.render(Json::default()).to_string();
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("json schema shape");

    assert_eq!(
        parsed["metadata"]["error_code"].as_str(),
        Some("API.UNAUTHORIZED")
    );
    assert_eq!(parsed["metadata"]["severity"].as_str(), Some("error"));
    assert_eq!(parsed["metadata"]["category"].as_str(), Some("auth"));
    assert_eq!(parsed["metadata"]["retryable"].as_bool(), Some(false));
    assert!(parsed["diagnostic_bag"]["stack_trace"].is_null());
    assert!(parsed["diagnostic_bag"]["display_causes"].is_null());
    assert!(parsed["diagnostic_bag"]["source_errors"].is_null());
    #[cfg(feature = "trace")]
    assert!(parsed["trace"].is_null());
    assert_eq!(parsed["context"].as_array().map(|a| a.len()), Some(1));
    assert_eq!(parsed["attachments"].as_array().map(|a| a.len()), Some(2));
}

#[cfg(feature = "json")]
#[test]
fn json_preserves_empty_cause_chains_with_state() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_display_cause_chain(DisplayCauseChain {
            items: vec![],
            truncated: true,
            cycle_detected: true,
        })
        .with_source_error_chain(SourceErrorChain {
            items: vec![],
            truncated: true,
            cycle_detected: true,
        });

    let json = report.render(Json::default()).to_string();
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("json schema shape");

    let display = &parsed["diagnostic_bag"]["display_causes"];
    assert!(display.is_object());
    assert_eq!(display["items"].as_array().map(|a| a.len()), Some(0));
    assert_eq!(display["truncated"].as_bool(), Some(true));
    assert_eq!(display["cycle_detected"].as_bool(), Some(true));

    let source = &parsed["diagnostic_bag"]["source_errors"];
    assert!(source.is_object());
    assert_eq!(source["items"].as_array().map(|a| a.len()), Some(0));
    assert_eq!(source["truncated"].as_bool(), Some(true));
    assert_eq!(source["cycle_detected"].as_bool(), Some(true));
}

#[cfg(feature = "json")]
#[test]
fn json_renderer_honors_section_visibility_options() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_error_code("API.UNAUTHORIZED")
        .attach("request_id", "req-json")
        .attach_printable("token rejected");

    let opts = ReportRenderOptions {
        show_governance_section: false,
        show_trace_section: false,
        show_stack_trace_section: false,
        show_context_section: false,
        show_attachments_section: false,
        show_cause_chains_section: false,
        show_empty_sections: false,
        ..ReportRenderOptions::default()
    };

    let json = report.render(Json::new(opts)).to_string();
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("json schema shape");

    assert!(parsed.get("metadata").is_none());
    assert!(parsed.get("diagnostic_bag").is_none());
    assert!(parsed.get("trace").is_none());
    assert!(parsed.get("context").is_none());
    assert!(parsed.get("attachments").is_none());
    assert_eq!(parsed["schema_version"], REPORT_JSON_SCHEMA_VERSION);
    assert_eq!(parsed["error"]["message"], "api unauthorized");
}

#[cfg(feature = "json")]
#[test]
fn json_display_causes_respect_depth_limits() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_display_cause("first")
        .with_display_cause("second");

    let opts = ReportRenderOptions {
        max_source_depth: 1,
        show_cause_chains_section: true,
        show_empty_sections: false,
        ..ReportRenderOptions::default()
    };

    let json = report.render(Json::new(opts)).to_string();
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("json schema shape");
    let display = &parsed["diagnostic_bag"]["display_causes"];

    assert_eq!(display["items"].as_array().map(|a| a.len()), Some(1));
    assert_eq!(display["truncated"].as_bool(), Some(true));
}

#[cfg(feature = "json")]
#[test]
fn json_renderer_supports_pretty_option() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized).with_error_code("API.UNAUTHORIZED");
    let opts = ReportRenderOptions {
        json_pretty: true,
        ..ReportRenderOptions::default()
    };
    let payload = report.render(Json::new(opts)).to_string();
    assert!(payload.contains('\n'));
    assert!(payload.contains("  \"schema_version\""));
}
