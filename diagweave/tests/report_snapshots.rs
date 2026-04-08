mod report_common;
use diagweave::prelude::*;
use diagweave::report::{AttachmentValue, StackTrace, StackTraceFormat};
#[cfg(all(feature = "json", feature = "trace"))]
use diagweave::report::{
    ParentSpanId, SpanId, TraceEvent, TraceEventAttribute, TraceEventLevel, TraceId,
};
use report_common::*;

#[test]
fn snap_pretty_basic() {
    let _guard = init_test();

    let report = Report::new(AuthError::InvalidToken)
        .with_error_code("AUTH.INVALID_TOKEN")
        .with_severity(Severity::Error)
        .with_ctx("request_id", "tx-snap-1")
        .attach_note("token expired at midnight")
        .map_err(|_| ApiError::Unauthorized);

    let output = report.snap_pretty();
    insta::assert_snapshot!(output);
}

#[test]
fn snap_pretty_with_source_chain() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_diag_src_err(AuthError::InvalidToken)
        .with_diag_src_err(std::io::Error::other("network down"));

    let output = report.snap_pretty();
    insta::assert_snapshot!(output);
}

#[test]
fn snap_pretty_with_stack_trace() {
    let _guard = init_test();

    let trace = StackTrace::new(StackTraceFormat::Raw).with_raw("frame-a\nframe-b\nframe-c");
    let report = Report::new(ApiError::Unauthorized)
        .with_stack_trace(trace)
        .with_error_code("API.ERR");

    let output = report.snap_pretty();
    insta::assert_snapshot!(output);
}

#[test]
fn snap_pretty_nested_sources() {
    let _guard = init_test();

    let pretty = Report::new(ApiError::Unauthorized)
        .map_err(|_| ApiError::Wrapped { code: 500 })
        .map_err(|_| ApiError::Wrapped { code: 501 })
        .snap_pretty();

    insta::assert_snapshot!(pretty);
}

#[test]
fn snap_compact_basic() {
    let _guard = init_test();

    let report = Report::new(AuthError::InvalidToken)
        .with_error_code("AUTH.INVALID_TOKEN")
        .with_ctx("request_id", "tx-compact");

    let output = report.snap_compact();
    insta::assert_snapshot!(output);
}

#[test]
fn snap_compact_with_boundary() {
    let _guard = init_test();

    let report = Report::new(AuthError::InvalidToken)
        .with_error_code("AUTH.INVALID_TOKEN")
        .with_ctx("request_id", "tx-2")
        .map_err(|_| ApiError::Unauthorized);

    let output = report.snap_compact();
    insta::assert_snapshot!(output);
}

#[cfg(feature = "json")]
#[test]
fn snap_json_basic() {
    let _guard = init_test();

    let report = Report::new(AuthError::InvalidToken)
        .with_error_code("AUTH.INVALID_TOKEN")
        .with_severity(Severity::Error)
        .with_ctx("request_id", "tx-json-snap")
        .attach_note("snapshot test note")
        .map_err(|_| ApiError::Unauthorized);

    let output = report.snap_json();
    insta::assert_snapshot!(output);
}

#[cfg(feature = "json")]
#[test]
fn snap_json_with_attachments() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_error_code("API.UNAUTHORIZED")
        .with_severity(Severity::Error)
        .with_category("auth")
        .with_retryable(false)
        .with_ctx("request_id", "req-json-snap")
        .attach_printable("token rejected")
        .attach_payload(
            "response",
            AttachmentValue::Bytes(vec![7, 8, 9]),
            Some("application/octet-stream".to_owned()),
        );

    let output = report.snap_json();
    insta::assert_snapshot!(output);
}

#[cfg(feature = "json")]
#[test]
fn snap_json_with_source_errors() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_diag_src_err(AuthError::InvalidToken)
        .with_diag_src_err(std::io::Error::other("network down"));

    let output = report.snap_json();
    insta::assert_snapshot!(output);
}

#[cfg(all(feature = "json", feature = "trace"))]
#[test]
fn snap_json_with_trace() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_trace_ids(
            TraceId::new("4bf92f3577b34da6a3ce929d0e0e4736").unwrap(),
            SpanId::new("00f067aa0ba902b7").unwrap(),
        )
        .with_parent_span_id(ParentSpanId::new("1111111111111111").unwrap())
        .with_trace_sampled(true)
        .with_trace_state("vendor=blue")
        .with_trace_flags(1)
        .with_trace_event(TraceEvent {
            name: "db.query".into(),
            level: Some(TraceEventLevel::Info),
            timestamp_unix_nano: Some(1_713_337_100_000_000_000),
            attributes: vec![TraceEventAttribute {
                key: "db.system".into(),
                value: AttachmentValue::from("postgres"),
            }],
        });

    let output = report.snap_json();
    insta::assert_snapshot!(output);
}

#[test]
fn snap_pretty_empty_report() {
    let _guard = init_test();

    let report = Report::new(AuthError::InvalidToken);
    let output = report.snap_pretty();
    insta::assert_snapshot!(output);
}

#[test]
fn snap_pretty_with_display_causes() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_display_cause(AuthError::InvalidToken)
        .with_display_cause("request was retried")
        .with_display_cause("fallback cache missed")
        .with_display_cause(ApiError::Wrapped { code: 502 });

    let output = report.snap_pretty();
    insta::assert_snapshot!(output);
}

#[test]
fn snap_pretty_with_system_context() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_system("host", "prod-web-01")
        .with_system("region", "us-east-1");

    let output = report.snap_pretty();
    insta::assert_snapshot!(output);
}

#[test]
fn snap_pretty_with_all_sections() {
    let _guard = init_test();

    let trace = StackTrace::new(StackTraceFormat::Raw).with_raw("main\nhandler\nprocess");
    let report = Report::new(AuthError::InvalidToken)
        .with_error_code("AUTH.ERR")
        .with_severity(Severity::Warn)
        .with_category("security")
        .with_retryable(true)
        .with_ctx("user_id", "u-123")
        .with_system("host", "prod-01")
        .attach_note("investigate later")
        .attach_payload("payload", AttachmentValue::from("data"), None::<&str>)
        .with_display_cause("token expired")
        .with_diag_src_err(std::io::Error::other("conn refused"))
        .with_stack_trace(trace);

    let output = report.snap_pretty();
    insta::assert_snapshot!(output);
}

#[cfg(all(feature = "json", feature = "otel"))]
#[test]
fn snap_otel_basic() {
    let _guard = init_test();

    let report = Report::new(AuthError::InvalidToken)
        .with_error_code("AUTH.INVALID_TOKEN")
        .with_severity(Severity::Error)
        .with_ctx("request_id", "tx-otel-1")
        .attach_note("otel snapshot note");

    let output = report.snap_otel();
    insta::assert_snapshot!(output);
}

#[cfg(all(feature = "json", feature = "otel"))]
#[test]
fn snap_otel_with_diag_sources() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_severity(Severity::Warn)
        .with_diag_src_err(AuthError::InvalidToken)
        .with_diag_src_err(std::io::Error::other("network down"));

    let output = report.snap_otel();
    insta::assert_snapshot!(output);
}

#[cfg(all(feature = "json", feature = "otel", feature = "trace"))]
#[test]
fn snap_otel_with_trace() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_severity(Severity::Error)
        .with_trace_ids(
            TraceId::new("4bf92f3577b34da6a3ce929d0e0e4736").unwrap(),
            SpanId::new("00f067aa0ba902b7").unwrap(),
        )
        .with_parent_span_id(ParentSpanId::new("1111111111111111").unwrap())
        .with_trace_sampled(true)
        .with_trace_event(TraceEvent {
            name: "db.query".into(),
            level: Some(TraceEventLevel::Info),
            timestamp_unix_nano: Some(1_713_337_100_000_000_000),
            attributes: vec![TraceEventAttribute {
                key: "db.system".into(),
                value: AttachmentValue::from("postgres"),
            }],
        });

    let output = report.snap_otel();
    insta::assert_snapshot!(output);
}

#[cfg(all(feature = "json", feature = "otel"))]
#[test]
fn snap_otel_with_payload() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_severity(Severity::Error)
        .with_category("payment")
        .with_ctx("order_id", "ord-9002")
        .attach_payload(
            "response_body",
            AttachmentValue::Bytes(vec![1, 2, 3, 4]),
            Some("application/json"),
        );

    let output = report.snap_otel();
    insta::assert_snapshot!(output);
}

#[cfg(all(feature = "json", feature = "otel"))]
#[test]
fn snap_otel_record_count() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized).with_severity(Severity::Error);

    let ir = report.to_diagnostic_ir();
    let otel = ir.to_otel_envelope();
    assert_eq!(
        otel.records.len(),
        1,
        "should have exactly 1 record (exception event)"
    );
}

#[cfg(all(feature = "json", feature = "otel", feature = "trace"))]
#[test]
fn snap_otel_record_count_with_trace() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_severity(Severity::Error)
        .with_trace_ids(
            TraceId::new("4bf92f3577b34da6a3ce929d0e0e4736").unwrap(),
            SpanId::new("00f067aa0ba902b7").unwrap(),
        )
        .with_trace_event(TraceEvent {
            name: "auth.check".into(),
            level: Some(TraceEventLevel::Warn),
            timestamp_unix_nano: None,
            attributes: vec![],
        });

    let ir = report.to_diagnostic_ir();
    let otel = ir.to_otel_envelope();
    assert_eq!(
        otel.records.len(),
        2,
        "should have 2 records (exception + trace event)"
    );
    assert_eq!(otel.records[0].name, "exception");
    assert_eq!(otel.records[1].name, "auth.check");
}
