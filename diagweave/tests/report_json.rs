mod report_common;
#[cfg(feature = "json")]
use diagweave::prelude::*;
#[cfg(feature = "json")]
use diagweave::render::{Json, REPORT_JSON_SCHEMA_VERSION, report_json_schema};
#[cfg(feature = "json")]
use diagweave::report::ReportMetadata;
#[cfg(feature = "json")]
use diagweave::report::{DisplayCauseChain, SourceErrorChain};
#[cfg(feature = "json")]
use report_common::*;

#[cfg(feature = "json")]
#[test]
fn render_format_supports_compact_pretty_and_json() {
    let report = Report::new(AuthError::InvalidToken)
        .with_error_code("AUTH.INVALID_TOKEN")
        .with_retryable(true)
        .with_ctx("request_id", "tx-json")
        .attach_payload(
            "http.request",
            AttachmentValue::Array(vec![
                AttachmentValue::from("GET"),
                AttachmentValue::from("/session"),
            ]),
            Some("application/x.debug".to_owned()),
        )
        .map_err(|_| ApiError::Unauthorized);

    let _guard = init_test();

    let compact = report.render(Compact::summary()).to_string();
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
        // stack_trace and display_causes are omitted when absent (no null output)
        assert!(!json.contains("\"stack_trace\""));
        assert!(!json.contains("\"display_causes\""));
        assert!(!json.contains("\"diagnostic_source_errors\""));
        assert!(!json.contains("\"origin_source_errors\""));

        let parsed: serde_json::Value = serde_json::from_str(&json).expect("json schema shape");
        assert_eq!(parsed["schema_version"], REPORT_JSON_SCHEMA_VERSION);
        assert_eq!(parsed["error"]["message"], "api unauthorized");
        // map_err preserves all metadata fields
        assert_eq!(parsed["metadata"]["error_code"].as_str(), Some("AUTH.INVALID_TOKEN"));
        assert_eq!(parsed["metadata"]["retryable"].as_bool(), Some(true));
        // stack_trace and display_causes are omitted when absent
        assert_eq!(parsed["diagnostic_bag"]["stack_trace"].as_object(), None);
        assert_eq!(parsed["diagnostic_bag"]["display_causes"].as_object(), None);
        assert_eq!(parsed["diagnostic_bag"]["origin_source_errors"].as_object(), None);
        #[cfg(feature = "trace")]
        assert!(parsed["trace"].is_object());
        assert_eq!(parsed["attachments"].as_array().map(|a| a.len()), Some(1));
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
        .with_ctx("request_id", "req-json")
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
    assert_eq!(parsed["metadata"]["severity"].as_str(), Some("error"));
    assert_eq!(parsed["metadata"]["category"].as_str(), Some("auth"));
    assert_eq!(parsed["metadata"]["retryable"].as_bool(), Some(false));
    // Fields are omitted when absent, not null
    assert_eq!(parsed["diagnostic_bag"]["stack_trace"].as_object(), None);
    assert_eq!(parsed["diagnostic_bag"]["display_causes"].as_object(), None);
    assert_eq!(parsed["diagnostic_bag"]["origin_source_errors"].as_object(), None);
    assert_eq!(parsed["diagnostic_bag"]["diagnostic_source_errors"].as_object(), None);
    #[cfg(feature = "trace")]
    assert!(parsed["trace"].is_object());
    assert_eq!(parsed["context"].as_object().map(|a| a.len()), Some(1));
    assert_eq!(parsed["attachments"].as_array().map(|a| a.len()), Some(2));
}

#[cfg(feature = "json")]
#[test]
fn report_metadata_requires_explicit_severity_typestate_for_deserialization() {
    let json = serde_json::json!({
        "error_code": "API.UNAUTHORIZED",
        "severity": "error",
        "category": "auth",
        "retryable": false
    })
    .to_string();

    let metadata: ReportMetadata<HasSeverity> =
        serde_json::from_str(&json).expect("explicit typestate should deserialize");

    assert_eq!(
        metadata.error_code().map(ToString::to_string),
        Some("API.UNAUTHORIZED".to_owned())
    );
    assert_eq!(metadata.severity(), Some(Severity::Error));
    assert_eq!(metadata.severity(), Some(Severity::Error));
    assert_eq!(metadata.category(), Some("auth"));
    assert_eq!(metadata.retryable(), Some(false));
}

#[cfg(feature = "json")]
#[test]
fn json_preserves_empty_cause_chains_with_state() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .set_display_causes(DisplayCauseChain {
            items: vec![],
            truncated: true,
            cycle_detected: true,
        })
        .set_diag_src_errs({
            let mut chain = SourceErrorChain::default();
            chain.truncated = true;
            chain.cycle_detected = true;
            chain
        });

    let json = report.render(Json::default()).to_string();
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("json schema shape");

    let display = &parsed["diagnostic_bag"]["display_causes"];
    assert!(display.is_object());
    assert_eq!(display["items"].as_array().map(|a| a.len()), Some(0));
    assert_eq!(display["truncated"].as_bool(), Some(true));
    assert_eq!(display["cycle_detected"].as_bool(), Some(true));

    let source = &parsed["diagnostic_bag"]["diagnostic_source_errors"];
    assert!(source.is_object());
    assert_eq!(source["roots"].as_array().map(|a| a.len()), Some(0));
    assert_eq!(source["nodes"].as_array().map(|a| a.len()), Some(0));
    assert_eq!(source["truncated"].as_bool(), Some(true));
    assert_eq!(source["cycle_detected"].as_bool(), Some(true));
}

#[cfg(feature = "json")]
#[test]
fn json_source_errors_include_error_type() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_diag_src_err(AuthError::InvalidToken)
        .with_diag_src_err(std::io::Error::other("network down"));

    let json = report.render(Json::default()).to_string();
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("json schema shape");
    let source = &parsed["diagnostic_bag"]["diagnostic_source_errors"];

    let nodes = source["nodes"].as_array().expect("nodes should be array");
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0]["message"], "auth invalid token");
    assert_eq!(nodes[0]["type"], std::any::type_name::<AuthError>());
}

#[cfg(feature = "json")]
#[test]
fn json_source_errors_without_concrete_type_omit_type_field() {
    let _guard = init_test();

    let report = Report::new(LoopError);

    let json = report.render(Json::default()).to_string();
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("json schema shape");
    let source = &parsed["diagnostic_bag"]["origin_source_errors"];
    let nodes = source["nodes"].as_array().expect("nodes should be array");
    // type field is omitted when absent, not null
    assert_eq!(
        nodes[0].get("type"),
        None
    );
}

#[cfg(feature = "json")]
#[test]
fn json_source_errors_hide_internal_report_wrapper_types() {
    let _guard = init_test();

    let report = Report::new(AuthError::InvalidToken).map_err(|_| ApiError::Unauthorized);

    let json = report.render(Json::default()).to_string();
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("json schema shape");
    // map_err does not create origin_source_errors, field is omitted
    assert_eq!(parsed["diagnostic_bag"]["origin_source_errors"].as_object(), None);
}

#[cfg(feature = "json")]
#[test]
fn json_source_errors_remap_roots_and_children_to_dense_indices() {
    let _guard = init_test();

    #[derive(Debug)]
    struct ChainLinkError {
        label: &'static str,
        source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
    }

    impl std::fmt::Display for ChainLinkError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.label)
        }
    }

    impl std::error::Error for ChainLinkError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.source
                .as_deref()
                .map(|v| v as &(dyn std::error::Error + 'static))
        }
    }

    fn chain(root: &'static str, child: &'static str) -> ChainLinkError {
        ChainLinkError {
            label: root,
            source: Some(Box::new(ChainLinkError {
                label: child,
                source: None,
            })),
        }
    }

    let report = Report::new(ApiError::Unauthorized)
        .with_diag_src_err(chain("left-root", "left-child"))
        .with_diag_src_err(chain("right-root", "right-child"));

    let json = report
        .render(Json::new(ReportRenderOptions {
            max_source_depth: 1,
            ..ReportRenderOptions::default()
        }))
        .to_string();
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("json schema shape");
    let source = &parsed["diagnostic_bag"]["diagnostic_source_errors"];

    assert_eq!(source["roots"], serde_json::json!([0, 1]));
    let nodes = source["nodes"].as_array().expect("nodes should be array");
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0]["message"], "left-root");
    assert_eq!(nodes[1]["message"], "right-root");
    assert_eq!(nodes[0]["source_roots"], serde_json::json!([]));
    assert_eq!(nodes[1]["source_roots"], serde_json::json!([]));
}

#[cfg(feature = "json")]
#[test]
fn json_renderer_honors_section_visibility_options() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_error_code("API.UNAUTHORIZED")
        .with_ctx("request_id", "req-json")
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

#[cfg(all(feature = "json", feature = "trace"))]
#[test]
fn json_trace_section_uses_shared_trace_shape() {
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

    let json = report.render(Json::default()).to_string();
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("json schema shape");
    let trace = &parsed["trace"];

    assert!(trace.get("error").is_none());
    assert_eq!(
        trace["context"]["trace_id"].as_str(),
        Some("4bf92f3577b34da6a3ce929d0e0e4736")
    );
    assert_eq!(
        trace["context"]["span_id"].as_str(),
        Some("00f067aa0ba902b7")
    );
    assert_eq!(
        trace["context"]["parent_span_id"].as_str(),
        Some("1111111111111111")
    );
    assert_eq!(trace["context"]["sampled"].as_bool(), Some(true));
    assert_eq!(
        trace["context"]["trace_state"].as_str(),
        Some("vendor=blue")
    );
    assert_eq!(trace["context"]["flags"].as_u64(), Some(1));
    assert_eq!(trace["events"].as_array().map(|a| a.len()), Some(1));
    assert_eq!(
        trace["events"][0]["attributes"][0]["value"]["kind"].as_str(),
        Some("string")
    );
}

#[cfg(all(feature = "json", feature = "trace"))]
#[test]
fn json_trace_section_keeps_tagged_trace_values() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized).with_trace_event(TraceEvent {
        name: "db.query".into(),
        level: Some(TraceEventLevel::Warn),
        timestamp_unix_nano: Some(1_713_337_100_000_000_000),
        attributes: vec![
            TraceEventAttribute {
                key: "db.statement".into(),
                value: AttachmentValue::Redacted {
                    kind: Some("sql".into()),
                    reason: Some("sensitive".into()),
                },
            },
            TraceEventAttribute {
                key: "blob".into(),
                value: AttachmentValue::Bytes(vec![1, 2, 3]),
            },
        ],
    });

    let json = report.render(Json::default()).to_string();
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("json schema shape");
    let trace = &parsed["trace"];
    let attrs = trace["events"][0]["attributes"]
        .as_array()
        .expect("attributes");

    assert_eq!(attrs[0]["value"]["kind"].as_str(), Some("redacted"));
    assert_eq!(attrs[0]["value"]["value"]["kind"].as_str(), Some("sql"));
    assert_eq!(attrs[1]["value"]["kind"].as_str(), Some("bytes"));
    assert_eq!(
        attrs[1]["value"]["value"].as_array().map(|a| a.len()),
        Some(3)
    );
}

#[cfg(all(feature = "json", feature = "trace"))]
#[test]
fn json_trace_section_rejects_non_finite_floats() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized).with_trace_event(TraceEvent {
        name: "db.query".into(),
        level: Some(TraceEventLevel::Info),
        timestamp_unix_nano: None,
        attributes: vec![TraceEventAttribute {
            key: "latency".into(),
            value: AttachmentValue::Float(f64::INFINITY),
        }],
    });

    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = report.render(Json::default()).to_string();
        }))
        .is_err()
    );
}
