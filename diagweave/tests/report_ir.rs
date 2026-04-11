mod report_common;
#[cfg(feature = "otel")]
use diagweave::adapters::OtelValue;
use diagweave::prelude::*;
use diagweave::report::ReportMetadata;
use report_common::*;
#[cfg(feature = "otel")]
use std::collections::BTreeMap;

#[test]
fn cause_tree_supports_multiple_sources_and_events() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_display_cause(AuthError::InvalidToken)
        .with_display_cause("request was retried")
        .with_display_cause("fallback cache missed")
        .with_display_cause(ApiError::Wrapped { code: 502 });

    let pretty = report.pretty().to_string();
    assert!(pretty.contains("auth invalid token"));
    assert!(pretty.contains("request was retried"));
    assert!(pretty.contains("fallback cache missed"));
    assert!(pretty.contains("api wrapped code=502"));
}

#[test]
fn diagnostic_ir_is_structured_and_renderer_independent() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_metadata(
            ReportMetadata::default()
                .with_error_code("API.UNAUTHORIZED")
                .with_category("auth")
                .with_retryable(false),
        )
        .set_severity(Severity::Error)
        .with_ctx("request_id", "req-ir-1")
        .attach_printable("note")
        .attach_payload(
            "response",
            AttachmentValue::Redacted {
                kind: Some("secret".into()),
                reason: Some("pii".into()),
            },
            "application/json".into(),
        )
        .with_display_cause(AuthError::InvalidToken)
        .with_display_cause("retry happened");

    let ir = report.to_diagnostic_ir();
    assert_eq!(ir.error.message, "api unauthorized");
    assert!(!ir.error.r#type.is_empty());
    assert_eq!(
        ir.metadata.error_code().map(ToString::to_string),
        Some("API.UNAUTHORIZED".to_owned())
    );
    assert_eq!(ir.metadata.severity(), Some(Severity::Error));
    assert_eq!(ir.metadata.severity(), Some(Severity::Error));
    assert_eq!(ir.context_count, 1);
    assert_eq!(ir.attachment_count, 2);
}

#[cfg(feature = "trace")]
#[test]
fn source_errors_field_matches_json_shape_in_tracing_fields() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_diag_src_err(AuthError::InvalidToken)
        .with_diag_src_err(std::io::Error::other("network down"));

    let ir = report.to_diagnostic_ir();
    let fields = ir.to_tracing_fields();
    let source_errors = fields
        .iter()
        .find(|f| f.key == "diagnostic_bag.diagnostic_source_errors")
        .map(|f| &f.value)
        .expect("report.diagnostic_source_errors field should be present");

    let AttachmentValue::Object(map) = source_errors else {
        panic!("report.diagnostic_source_errors should be object");
    };
    assert_eq!(map.get("truncated"), Some(&AttachmentValue::Bool(false)));
    assert_eq!(
        map.get("cycle_detected"),
        Some(&AttachmentValue::Bool(false))
    );

    let Some(AttachmentValue::Array(roots)) = map.get("roots") else {
        panic!("roots should be an array");
    };
    assert_eq!(roots.len(), 2);
    assert_eq!(roots[0], AttachmentValue::Integer(0));
    assert_eq!(roots[1], AttachmentValue::Integer(1));

    let Some(AttachmentValue::Array(nodes)) = map.get("nodes") else {
        panic!("nodes should be an array");
    };
    assert_eq!(nodes.len(), 2);
    let AttachmentValue::Object(first) = &nodes[0] else {
        panic!("first source error should be object");
    };
    assert_eq!(
        first.get("message"),
        Some(&AttachmentValue::String("auth invalid token".into()))
    );
    assert_eq!(
        first.get("type"),
        Some(&AttachmentValue::String(
            std::any::type_name::<AuthError>().into()
        ))
    );
    assert_eq!(
        first.get("source_roots"),
        Some(&AttachmentValue::Array(Vec::new()))
    );
}

#[cfg(feature = "otel")]
#[test]
fn otel_value_conversion_handles_unsigned_overflow_redacted_and_nested_object() {
    let _guard = init_test();

    let nested = AttachmentValue::from(BTreeMap::from([
        ("a".to_owned(), AttachmentValue::Unsigned(u64::MAX)),
        (
            "b".to_owned(),
            AttachmentValue::Array(vec![
                AttachmentValue::Bool(true),
                AttachmentValue::from(BTreeMap::from([(
                    "inner".to_owned(),
                    AttachmentValue::String("ok".into()),
                )])),
            ]),
        ),
    ]));

    let report = Report::new(ApiError::Unauthorized)
        .with_ctx("overflow", ContextValue::Unsigned(u64::MAX))
        .with_ctx(
            "secret",
            ContextValue::Redacted {
                kind: Some("token".into()),
                reason: Some("sensitive".into()),
            },
        )
        .attach_payload("nested", nested, Some("application/json"));

    let ir = report.to_diagnostic_ir().with_severity(Severity::Error);
    let otel = ir.to_otel_envelope();
    let record = otel.records.first().expect("report record should exist");
    assert_eq!(record.name, "exception");
    assert_eq!(record.severity_text.as_deref(), Some("error"));
    assert_eq!(record.severity_number, Some(17));

    let overflow_ctx = record
        .attributes
        .iter()
        .find(|v| v.key == "overflow")
        .expect("overflow attribute should exist");
    assert_eq!(overflow_ctx.value, OtelValue::U64(u64::MAX));

    let secret_ctx = record
        .attributes
        .iter()
        .find(|v| v.key == "secret")
        .expect("secret attribute should exist");
    match &secret_ctx.value {
        OtelValue::KvList(attrs) => {
            assert!(attrs.iter().any(|a| a.key == "kind"));
            assert!(attrs.iter().any(|a| a.key == "reason"));
        }
        other => panic!("expected redacted to convert into kvlist, got: {other:?}"),
    }

    let nested_payload = record
        .attributes
        .iter()
        .find(|a| a.key == "attachment.payload.nested")
        .map(|a| &a.value)
        .expect("nested payload should exist");
    match nested_payload {
        OtelValue::KvList(attrs) => {
            let a_value = attrs
                .iter()
                .find(|a| a.key == "a")
                .map(|a| &a.value)
                .expect("nested.a should exist");
            assert_eq!(a_value, &OtelValue::U64(u64::MAX));
            let b_value = attrs
                .iter()
                .find(|a| a.key == "b")
                .map(|a| &a.value)
                .expect("nested.b should exist");
            match b_value {
                OtelValue::Array(items) => {
                    assert_eq!(items.len(), 2);
                }
                other => panic!("nested.b should be array, got: {other:?}"),
            }
        }
        other => panic!("nested payload should be kvlist, got: {other:?}"),
    }
}

#[cfg(feature = "otel")]
#[test]
fn diagnostic_ir_requires_explicit_severity_upgrade_before_otel() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized);
    let ir = report.to_diagnostic_ir().with_severity(Severity::Warn);
    let otel = ir.to_otel_envelope();
    let record = otel.records.first().expect("report record should exist");

    assert_eq!(record.name, "exception");
    assert_eq!(record.severity_text.as_deref(), Some("warn"));
    assert_eq!(record.severity_number, Some(13));
}

#[cfg(all(feature = "trace", feature = "otel"))]
#[test]
fn diagnostic_ir_maps_to_tracing_and_otel_adapters() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_error_code("API.UNAUTHORIZED")
        .with_severity(Severity::Error)
        .with_retryable(false);
    let report = report
        .with_trace_ids(
            TraceId::new("4bf92f3577b34da6a3ce929d0e0e4736").unwrap(),
            SpanId::new("00f067aa0ba902b7").unwrap(),
        )
        .with_parent_span_id(ParentSpanId::new("1111111111111111").unwrap())
        .with_trace_sampled(true)
        .with_trace_state("vendor=blue")
        .with_trace_flags(1)
        .with_trace_event(TraceEvent {
            name: "auth.lookup".into(),
            level: Some(TraceEventLevel::Warn),
            timestamp_unix_nano: Some(1_713_337_000_000_000_000),
            attributes: vec![
                TraceEventAttribute {
                    key: "db.system".into(),
                    value: AttachmentValue::from("postgres"),
                },
                TraceEventAttribute {
                    key: "db.statement".into(),
                    value: AttachmentValue::Redacted {
                        kind: Some("sql".into()),
                        reason: Some("sensitive".into()),
                    },
                },
            ],
        })
        .with_ctx("request_id", "req-otel-1")
        .attach_printable("attachment-note")
        .attach_payload(
            "payload",
            AttachmentValue::from(BTreeMap::from([
                ("path", AttachmentValue::from("/health")),
                ("status", AttachmentValue::Unsigned(401)),
            ])),
            Some("application/json"),
        )
        .with_display_cause(AuthError::InvalidToken)
        .with_display_cause("fallback path");

    let ir = report.to_diagnostic_ir();
    let tracing_fields = ir.to_tracing_fields();
    assert!(tracing_fields.iter().any(|f| f.key == "error"));
    assert!(
        tracing_fields
            .iter()
            .any(|f| f.key == "metadata.error_code")
    );
    assert!(tracing_fields.iter().any(|f| f.key == "metadata.severity"));
    let trace_value = tracing_fields
        .iter()
        .find(|f| f.key == "trace")
        .map(|f| &f.value)
        .expect("trace field should be present");
    let AttachmentValue::Object(trace_obj) = trace_value else {
        panic!("trace should be object");
    };
    let Some(AttachmentValue::Object(trace_error)) = trace_obj.get("error") else {
        panic!("trace.error should be object");
    };
    assert_eq!(
        trace_error.get("message"),
        Some(&AttachmentValue::String("api unauthorized".into()))
    );
    assert_eq!(
        trace_error.get("type"),
        Some(&AttachmentValue::String(
            std::any::type_name::<ApiError>().into()
        ))
    );
    let Some(AttachmentValue::Array(events)) = trace_obj.get("events") else {
        panic!("trace.events should be array");
    };
    assert!(!events.is_empty());
    let otel = ir.to_otel_envelope();
    let report_record = otel
        .records
        .iter()
        .find(|record| record.name == "exception")
        .expect("report record should exist");
    assert!(
        report_record
            .attributes
            .iter()
            .any(|a| a.key == "diagnostic_bag.display_causes")
    );
    let trace_record = otel
        .records
        .iter()
        .find(|record| record.name == "auth.lookup")
        .expect("trace record should exist");
    assert_eq!(
        trace_record.timestamp_unix_nano,
        Some(1_713_337_000_000_000_000)
    );
    assert_eq!(trace_record.severity_text.as_deref(), Some("warn"));
    assert_eq!(trace_record.severity_number, Some(13));
    assert!(trace_record.trace_id.as_ref().map(|v| v.as_ref()).is_some());
    assert!(
        trace_record
            .attributes
            .iter()
            .any(|a| a.key == "trace.parent_span_id")
    );
}

#[cfg(feature = "trace")]
#[test]
fn tracing_exporter_skips_empty_trace_section() {
    let _guard = init_test();

    let report =
        Report::new(ApiError::Unauthorized).with_trace(diagweave::report::ReportTrace::default());
    let ir = report.to_diagnostic_ir();
    let fields = ir.to_tracing_fields();
    assert!(fields.iter().all(|field| field.key != "trace"));
}

#[cfg(feature = "trace")]
#[test]
fn hex_ids_reject_all_zero_values() {
    assert!(TraceId::new("00000000000000000000000000000000").is_err());
    assert!(SpanId::new("0000000000000000").is_err());
    assert!(ParentSpanId::new("0000000000000000").is_err());
}

#[cfg(all(feature = "json", feature = "otel", feature = "trace"))]
#[test]
fn otel_envelope_serializes_with_expected_serde_shape() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_severity(Severity::Error)
        .with_trace_ids(
            TraceId::new("4bf92f3577b34da6a3ce929d0e0e4736").unwrap(),
            SpanId::new("00f067aa0ba902b7").unwrap(),
        )
        .with_trace_event(TraceEvent {
            name: "db.query".into(),
            level: Some(TraceEventLevel::Info),
            timestamp_unix_nano: Some(1_713_337_100_000_000_000),
            attributes: vec![TraceEventAttribute {
                key: "db.system".into(),
                value: AttachmentValue::from("postgres"),
            }],
        });

    let ir = report.to_diagnostic_ir();
    let otel = ir.to_otel_envelope();
    let json = serde_json::to_value(&otel).expect("otel envelope should serialize");

    assert_eq!(
        serde_json::to_value(OtelValue::Null).expect("null value should serialize"),
        serde_json::json!("Null")
    );

    let records = json["records"].as_array().expect("records should be array");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["name"], "exception");
    assert_eq!(records[0]["severity_text"], "error");
    assert_eq!(records[0]["severity_number"], 17);
    assert_eq!(records[0]["body"]["KvList"][0]["key"], "message");
    assert_eq!(
        records[0]["body"]["KvList"][0]["value"],
        serde_json::json!({"String": "api unauthorized"})
    );
    assert_eq!(records[1]["name"], "db.query");
    assert_eq!(records[1]["severity_text"], "info");
    assert_eq!(records[1]["severity_number"], 9);
    assert!(records[1]["body"].is_null());
}
