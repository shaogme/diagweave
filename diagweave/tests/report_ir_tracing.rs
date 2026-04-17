//! Tests for tracing exporter functionality.
//!
//! This module contains tests related to:
//! - TracingExporterTrait implementation
//! - Tracing field serialization
//! - Severity level handling in tracing events
//! - PreparedTracingEmission and EmitStats

mod report_common;

#[cfg(feature = "tracing")]
use diagweave::prelude::*;
#[cfg(all(feature = "tracing", feature = "std"))]
use diagweave::trace::PreparedTracingLevel;
#[cfg(feature = "tracing")]
use diagweave::trace::TracingExporterTrait;
#[cfg(feature = "tracing")]
use diagweave::trace::{EmitStats, PreparedTracingEmission};
#[cfg(feature = "tracing")]
use report_common::*;
#[cfg(feature = "tracing")]
use std::cell::Cell;
#[cfg(all(feature = "tracing", feature = "std"))]
use std::collections::BTreeMap;
#[cfg(all(feature = "tracing", feature = "std"))]
use std::sync::{Arc, Mutex};
#[cfg(all(feature = "tracing", feature = "std"))]
use tracing::Subscriber;
#[cfg(all(feature = "tracing", feature = "std"))]
use tracing::field::{Field, Visit};
#[cfg(all(feature = "tracing", feature = "std"))]
use tracing_subscriber::layer::{Context, Layer};
#[cfg(all(feature = "tracing", feature = "std"))]
use tracing_subscriber::prelude::*;
#[cfg(all(feature = "tracing", feature = "std"))]
use tracing_subscriber::registry::LookupSpan;

#[cfg(feature = "tracing")]
#[test]
fn tracing_exporter_trait_receives_prepared_emission() {
    let _guard = init_test();

    struct CountingExporter<'a> {
        calls: &'a Cell<usize>,
        stack_trace_present: &'a Cell<bool>,
        trace_events: &'a Cell<usize>,
    }

    impl TracingExporterTrait for CountingExporter<'_> {
        fn export_prepared(&self, emission: PreparedTracingEmission<'_>) -> EmitStats {
            let stats = emission.stats();
            let ir = emission.ir();
            self.calls.set(self.calls.get() + 1);
            self.stack_trace_present
                .set(ir.metadata.stack_trace().is_some());
            self.trace_events
                .set(ir.trace.events().map(|e| e.len()).unwrap_or(0));
            stats
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
        .with_severity(Severity::Info)
        .with_trace_ids(
            TraceId::from_str("4bf92f3577b34da6a3ce929d0e0e4736").unwrap(),
            SpanId::from_str("00f067aa0ba902b7").unwrap(),
        )
        .with_trace_event(TraceEvent {
            name: "db.query".into(),
            level: Some(TraceEventLevel::Info),
            timestamp_unix_nano: Some(1_713_337_100_000_000_000),
            attributes: vec![TraceEventAttribute {
                key: "db.system".into(),
                value: AttachmentValue::from("postgres"),
            }],
        })
        .with_display_cause("fallback path");

    report.prepare_tracing().emit_with(&exporter);
    assert_eq!(calls.get(), 1);
    assert!(!stack_trace_present.get());
    assert_eq!(trace_events.get(), 1);
}

#[cfg(all(feature = "tracing", feature = "std"))]
#[test]
fn tracing_exporter_uses_report_severity_for_unset_trace_events_and_carries_context() {
    let _guard = init_test();

    #[derive(Default)]
    struct FieldVisitor {
        fields: BTreeMap<String, String>,
    }

    impl Visit for FieldVisitor {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.fields
                .insert(field.name().to_string(), format!("{value:?}"));
        }
    }

    #[derive(Clone)]
    struct EventCollector {
        events: Arc<Mutex<Vec<CapturedEvent>>>,
    }

    struct CapturedEvent {
        level: tracing::Level,
        target: String,
        fields: BTreeMap<String, String>,
    }

    impl<S> Layer<S> for EventCollector
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            let mut visitor = FieldVisitor::default();
            event.record(&mut visitor);
            self.events.lock().expect("event lock").push(CapturedEvent {
                level: *event.metadata().level(),
                target: event.metadata().target().to_string(),
                fields: visitor.fields,
            });
        }
    }

    let events = Arc::new(Mutex::new(Vec::new()));
    let collector = EventCollector {
        events: Arc::clone(&events),
    };
    let subscriber = tracing_subscriber::registry().with(collector);
    let _subscriber = tracing::subscriber::set_default(subscriber);

    let report = Report::new(ApiError::Unauthorized)
        .with_severity(Severity::Error)
        .with_trace_ids(
            TraceId::from_str("4bf92f3577b34da6a3ce929d0e0e4736").unwrap(),
            SpanId::from_str("00f067aa0ba902b7").unwrap(),
        )
        .with_parent_span_id(ParentSpanId::from_str("1111111111111111").unwrap())
        .with_trace_sampled(true)
        .with_trace_state("vendor=blue")
        .with_trace_flags(1)
        .with_trace_event(TraceEvent {
            name: "db.query".into(),
            level: None,
            timestamp_unix_nano: Some(1_713_337_100_000_000_000),
            attributes: vec![],
        });

    let prepared = report.prepare_tracing();
    assert_eq!(prepared.report_level(), PreparedTracingLevel::Error);
    assert_eq!(
        prepared.trace_event_level(0),
        Some(PreparedTracingLevel::Error)
    );
    prepared.emit();

    let events = events.lock().expect("events lock");
    let trace_event = events
        .iter()
        .find(|event| event.target == "diagweave::trace_event")
        .expect("trace event should be emitted");

    assert_eq!(trace_event.level, tracing::Level::ERROR);
    assert!(
        trace_event
            .fields
            .get("trace_id")
            .is_some_and(|v| v.contains("4bf92f3577b34da6a3ce929d0e0e4736"))
    );
    assert!(
        trace_event
            .fields
            .get("span_id")
            .is_some_and(|v| v.contains("00f067aa0ba902b7"))
    );
    assert!(
        trace_event
            .fields
            .get("parent_span_id")
            .is_some_and(|v| v.contains("1111111111111111"))
    );
    assert!(
        trace_event
            .fields
            .get("trace_sampled")
            .is_some_and(|v| v.contains("true"))
    );
    assert!(
        trace_event
            .fields
            .get("trace_state")
            .is_some_and(|v| v.contains("vendor=blue"))
    );
    assert!(
        trace_event
            .fields
            .get("trace_flags")
            .is_some_and(|v| v.contains("1"))
    );
}

#[cfg(all(feature = "tracing", feature = "std"))]
#[test]
fn diagnostic_ir_requires_explicit_severity_upgrade_before_tracing() {
    let _guard = init_test();

    #[derive(Clone)]
    struct EventCollector {
        events: Arc<Mutex<Vec<()>>>,
    }

    impl<S> Layer<S> for EventCollector
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        fn on_event(&self, _event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            self.events.lock().expect("event lock").push(());
        }
    }

    let events = Arc::new(Mutex::new(Vec::new()));
    let collector = EventCollector {
        events: Arc::clone(&events),
    };
    let subscriber = tracing_subscriber::registry().with(collector);
    let _subscriber = tracing::subscriber::set_default(subscriber);

    let report = Report::new(ApiError::Unauthorized)
        .with_trace_ids(
            TraceId::from_str("4bf92f3577b34da6a3ce929d0e0e4736").unwrap(),
            SpanId::from_str("00f067aa0ba902b7").unwrap(),
        )
        .with_trace_event(TraceEvent {
            name: "db.query".into(),
            level: Some(TraceEventLevel::Info),
            timestamp_unix_nano: Some(1_713_337_100_000_000_000),
            attributes: vec![],
        });

    let ir = report.to_diagnostic_ir().with_severity(Severity::Warn);
    let prepared = ir.prepare_tracing();
    assert_eq!(prepared.report_level(), PreparedTracingLevel::Warn);

    let captured_events = events.lock().expect("events lock");
    assert!(
        captured_events.is_empty(),
        "preparing a tracing emission should not emit eagerly"
    );
    drop(captured_events);

    prepared.emit();

    let captured_events = events.lock().expect("events lock");
    assert!(
        !captured_events.is_empty(),
        "upgraded diagnostic ir should emit through tracing"
    );
}
