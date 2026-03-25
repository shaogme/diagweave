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
        #[cfg(feature = "trace")]
        assert!(json.contains("\"trace\""));
        assert!(json.contains("\"context\""));
        assert!(json.contains("\"attachments\""));
        assert!(json.contains("\"stack_trace\""));
        assert!(json.contains("\"causes\""));

        let parsed: ReportJsonDocument = serde_json::from_str(&json).expect("json schema shape");
        assert_eq!(parsed.schema_version, REPORT_JSON_SCHEMA_VERSION);
        assert_eq!(parsed.error.message, "api unauthorized");
        assert_eq!(parsed.metadata.error_code.as_deref(), None);
        assert!(parsed.metadata.retryable.is_none());
        assert!(parsed.metadata.stack_trace.is_none());
        assert!(parsed.metadata.causes.is_some());
        assert_eq!(parsed.attachments.len(), 0);
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
    let parsed: ReportJsonDocument = serde_json::from_str(&json).expect("json schema shape");

    assert_eq!(
        parsed.metadata.error_code.as_deref(),
        Some("API.UNAUTHORIZED")
    );
    assert_eq!(parsed.metadata.severity, Some(Severity::Error));
    assert_eq!(parsed.metadata.category.as_deref(), Some("auth"));
    assert_eq!(parsed.metadata.retryable, Some(false));
    assert!(parsed.metadata.causes.is_none());
    #[cfg(feature = "trace")]
    assert!(parsed.trace.events.is_empty());
    assert_eq!(parsed.context.len(), 1);
    assert_eq!(parsed.attachments.len(), 2);
    assert!(matches!(
        parsed.attachments[0],
        ReportJsonAttachment::Note { .. }
    ));
    assert!(matches!(
        parsed.attachments[1],
        ReportJsonAttachment::Payload { .. }
    ));
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
