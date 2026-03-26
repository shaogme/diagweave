mod report_common;
use diagweave::prelude::*;
use report_common::*;
#[cfg(feature = "tracing")]
use std::cell::Cell;
#[cfg(feature = "trace")]
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
        .with_metadata(ReportMetadata {
            error_code: Some("API.UNAUTHORIZED".to_owned()),
            severity: Some(Severity::Error),
            category: Some("auth".to_owned()),
            retryable: Some(false),
            stack_trace: None,
            display_causes: None,
            source_errors: None,
        })
        .attach("request_id", "req-ir-1")
        .attach_printable("note")
        .attach_payload(
            "response",
            AttachmentValue::Redacted {
                kind: Some("secret".to_owned()),
                reason: Some("pii".to_owned()),
            },
            Some("application/json".to_owned()),
        )
        .with_display_cause(AuthError::InvalidToken)
        .with_display_cause("retry happened");

    let ir = report.to_diagnostic_ir(ReportRenderOptions::default());
    assert_eq!(ir.error.message, "api unauthorized");
    assert!(!ir.error.r#type.is_empty());
    assert_eq!(ir.metadata.error_code.as_deref(), Some("API.UNAUTHORIZED"));
    assert_eq!(ir.context.len(), 1);
    assert_eq!(ir.attachments.len(), 2);
    let display_causes = ir
        .metadata
        .display_causes
        .as_ref()
        .expect("display causes should exist");
    assert_eq!(display_causes.items.len(), 2);
    assert!(!display_causes.truncated);
    assert!(!display_causes.cycle_detected);
}

#[cfg(feature = "trace")]
#[test]
fn diagnostic_ir_maps_to_tracing_and_otel_adapters() {
    let _guard = init_test();

    let report = Report::new(ApiError::Unauthorized)
        .with_error_code("API.UNAUTHORIZED")
        .with_severity(Severity::Error)
        .with_retryable(false)
        .with_trace_ids("4bf92f3577b34da6a3ce929d0e0e4736", "00f067aa0ba902b7")
        .with_parent_span_id("1111111111111111")
        .with_trace_sampled(true)
        .with_trace_state("vendor=blue")
        .with_trace_flags(1)
        .with_trace_event(TraceEvent {
            name: "auth.lookup".to_owned(),
            level: Some(TraceEventLevel::Warn),
            timestamp_unix_nano: Some(1_713_337_000_000_000_000),
            attributes: vec![
                TraceEventAttribute {
                    key: "db.system".to_owned(),
                    value: AttachmentValue::from("postgres"),
                },
                TraceEventAttribute {
                    key: "db.statement".to_owned(),
                    value: AttachmentValue::Redacted {
                        kind: Some("sql".to_owned()),
                        reason: Some("sensitive".to_owned()),
                    },
                },
            ],
        })
        .attach("request_id", "req-otel-1")
        .attach_printable("attachment-note")
        .attach_payload(
            "payload",
            AttachmentValue::Object(BTreeMap::from([
                ("path".to_owned(), AttachmentValue::from("/health")),
                ("status".to_owned(), AttachmentValue::Unsigned(401)),
            ])),
            Some("application/json".to_owned()),
        )
        .with_display_cause(AuthError::InvalidToken)
        .with_display_cause("fallback path");

    let ir = report.to_diagnostic_ir(ReportRenderOptions::default());
    let tracing_fields = ir.to_tracing_fields();
    assert!(tracing_fields.iter().any(|f| f.key == "error.message"));
    assert!(tracing_fields.iter().any(|f| f.key == "error.code"));
    assert!(tracing_fields.iter().any(|f| f.key == "trace.trace_id"));
    assert!(tracing_fields.iter().any(|f| f.key == "trace.event.0.name"));
    assert!(tracing_fields.iter().any(|f| f.key == "context.request_id"));
    assert!(
        tracing_fields
            .iter()
            .any(|f| f.key.starts_with("attachment.payload."))
    );
    assert!(
        tracing_fields
            .iter()
            .any(|f| f.key == "display_causes.present")
    );

    let otel = ir.to_otel_envelope();
    assert!(
        otel.attributes
            .iter()
            .any(|a| a.key == "stack_trace.present")
    );
    assert!(
        otel.attributes
            .iter()
            .any(|a| a.key == "display_causes.present")
    );
    assert!(otel.attributes.iter().any(|a| a.key == "trace.event_count"));
    assert!(otel.events.iter().any(|e| e.name == "trace.event"));
    assert!(
        otel.events
            .iter()
            .any(|e| e.name == "report.attachment.payload")
    );
}

#[cfg(feature = "tracing")]
#[test]
fn tracing_exporter_trait_receives_diagnostic_ir() {
    let _guard = init_test();

    // use std::cell::Cell; moved to top

    struct CountingExporter<'a> {
        calls: &'a Cell<usize>,
        stack_trace_present: &'a Cell<bool>,
        trace_events: &'a Cell<usize>,
    }

    impl TracingExporterTrait for CountingExporter<'_> {
        fn export_ir(&self, ir: &DiagnosticIr) {
            self.calls.set(self.calls.get() + 1);
            self.stack_trace_present
                .set(ir.metadata.stack_trace.is_some());
            self.trace_events.set(ir.trace.events.len());
        }
    }

    let calls = Cell::new(0usize);
    let stack_trace_present = Cell::new(false);
    let trace_events = Cell::new(0usize);
    let exporter = CountingExporter {
        calls: &calls,
        stack_trace_present: &stack_trace_present,
        trace_events: &trace_events,
    };

    let report = Report::new(ApiError::Unauthorized)
        .with_trace_ids("4bf92f3577b34da6a3ce929d0e0e4736", "00f067aa0ba902b7")
        .with_trace_event(TraceEvent {
            name: "db.query".to_owned(),
            level: Some(TraceEventLevel::Info),
            timestamp_unix_nano: Some(1_713_337_100_000_000_000),
            attributes: vec![TraceEventAttribute {
                key: "db.system".to_owned(),
                value: AttachmentValue::from("postgres"),
            }],
        })
        .with_display_cause("fallback path");

    report.emit_tracing_with(&exporter, ReportRenderOptions::default());
    assert_eq!(calls.get(), 1);
    assert!(!stack_trace_present.get());
    assert_eq!(trace_events.get(), 1);
}
