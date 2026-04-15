use std::{
    env, io,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use diagweave::prelude::{
    AttachmentValue, Compact, GlobalContext, HasSeverity, Pretty, Report, ReportRenderOptions,
    ResultReportExt, Severity, SpanId, TraceEventAttribute, TraceEventLevel, TraceId,
    register_global_injector, set, union,
};
use diagweave::render::{Json, PrettyIndent, REPORT_JSON_SCHEMA_VERSION};
use diagweave::report::TraceFlags;
use opentelemetry::logs::{
    AnyValue, LogRecord as _, Logger as _, LoggerProvider as _, Severity as OtelLogSeverity,
};
use opentelemetry::trace::{TraceContextExt, TracerProvider as _};
use opentelemetry::{Key, KeyValue};
use opentelemetry_otlp::{Protocol, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::prelude::*;

mod payment {
    use super::*;

    set! {
        #[derive(Debug)]
        NetworkError = {
            #[from]
            #[display(transparent)]
            Io(io::Error),

            #[display("timeout after {0}ms")]
            Timeout(u64),
        }

        #[derive(Debug)]
        pub PaymentError = NetworkError | {
            #[display("payment declined by provider")]
            Declined,
        }
    }

    fn low_level_io() -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::ConnectionRefused,
            "payment provider refused connection",
        ))
    }

    fn declined_report(amount_cents: u64) -> Report<PaymentError, HasSeverity> {
        Report::new(PaymentError::Declined)
            .with_error_code("PAYMENT.DECLINED")
            .with_severity(Severity::Warn)
            .with_category("payment")
            .with_retryable(false)
            .attach_note("payment provider declined")
            .with_display_cause("risk policy rejected the transaction")
            .with_diag_src_err(io::Error::other("issuer hard decline"))
            .attach_payload(
                "provider_reply",
                serde_json::json!({
                    "provider": "mockpay",
                    "decision": "declined",
                    "decline_code": "insufficient_funds"
                }),
                Some("application/json"),
            )
            .push_trace_event_with(
                "payment.provider.decline",
                Some(TraceEventLevel::Warn),
                Some(1_713_337_001_000_000_000),
                vec![
                    TraceEventAttribute {
                        key: "payment.amount_cents".into(),
                        value: AttachmentValue::from(amount_cents),
                    },
                    TraceEventAttribute {
                        key: "payment.provider".into(),
                        value: AttachmentValue::from("mockpay"),
                    },
                ],
            )
            .with_ctx("payment_stage", "charge")
    }

    fn timeout_report(amount_cents: u64) -> Report<PaymentError, HasSeverity> {
        Report::new(PaymentError::from(NetworkError::Timeout(250)))
            .with_error_code("PAYMENT.TIMEOUT")
            .with_severity(Severity::Error)
            .with_category("payment")
            .with_retryable(true)
            .attach_note("payment provider timeout")
            .with_display_cause("upstream provider exceeded SLA")
            .with_diag_src_err(io::Error::new(
                io::ErrorKind::TimedOut,
                "provider response timeout",
            ))
            .attach_payload(
                "provider_reply",
                serde_json::json!({
                    "provider": "mockpay",
                    "decision": "timeout",
                    "timeout_ms": 250
                }),
                Some("application/json"),
            )
            .push_trace_event_with(
                "payment.provider.timeout",
                Some(TraceEventLevel::Error),
                Some(1_713_337_002_000_000_000),
                vec![
                    TraceEventAttribute {
                        key: "payment.amount_cents".into(),
                        value: AttachmentValue::from(amount_cents),
                    },
                    TraceEventAttribute {
                        key: "retryable".into(),
                        value: AttachmentValue::from(true),
                    },
                ],
            )
            .with_ctx("payment_stage", "charge")
    }

    fn network_report(
        amount_cents: u64,
        io_kind: io::ErrorKind,
        io_message: String,
    ) -> Report<PaymentError, HasSeverity> {
        let err = NetworkError::Io(io::Error::new(io_kind, io_message.clone()));
        Report::new(PaymentError::from(err))
            .with_error_code("PAYMENT.NETWORK")
            .with_severity(Severity::Error)
            .with_category("payment")
            .with_retryable(true)
            .attach_note("payment provider network error")
            .with_display_cause("tcp dial to provider failed")
            .with_diag_src_err(io::Error::new(io_kind, io_message))
            .attach_payload(
                "provider_reply",
                serde_json::json!({
                    "provider": "mockpay",
                    "decision": "network_error",
                    "io_kind": io_kind.to_string()
                }),
                Some("application/json"),
            )
            .push_trace_event_with(
                "payment.provider.io_error",
                Some(TraceEventLevel::Error),
                Some(1_713_337_003_000_000_000),
                vec![
                    TraceEventAttribute {
                        key: "payment.amount_cents".into(),
                        value: AttachmentValue::from(amount_cents),
                    },
                    TraceEventAttribute {
                        key: "error.kind".into(),
                        value: AttachmentValue::from(io_kind.to_string()),
                    },
                ],
            )
            .with_ctx("payment_stage", "charge")
    }

    /// Charges the payment provider for the given amount in cents.
    pub fn charge(amount_cents: u64) -> Result<(), Report<PaymentError, HasSeverity>> {
        match amount_cents {
            0 => Err(declined_report(amount_cents)),
            1 => Err(timeout_report(amount_cents)),
            2 => match low_level_io() {
                Ok(()) => Ok(()),
                Err(io_err) => Err(network_report(
                    amount_cents,
                    io_err.kind(),
                    io_err.to_string(),
                )),
            },
            _ => Ok(()),
        }
    }
}

union! {
    #[derive(Debug)]
    pub enum ScenarioError =
        order::OrderError as Order |
        payment::PaymentError as Payment |
        {
            #[display("bad request: {reason}")]
            BadRequest { reason: String },
        }
}

type ScenarioReport = Report<ScenarioError, HasSeverity>;

mod order {
    use super::*;

    set! {
        #[derive(Debug)]
        pub OrderError = {
            #[display("payment failed for order {order_id}")]
            PaymentFailed { order_id: u64 },

            #[display("order {order_id} is invalid")]
            InvalidOrder { order_id: u64 },
        }
    }

    /// Creates an order and runs the payment stage.
    pub fn create(order_id: u64) -> Result<(), Report<OrderError, HasSeverity>> {
        create_with_amount(order_id, 18800)
    }

    /// Creates an order and runs payment with a custom amount for scenario simulation.
    pub fn create_with_amount(
        order_id: u64,
        amount_cents: u64,
    ) -> Result<(), Report<OrderError, HasSeverity>> {
        if order_id == 0 {
            return Err(invalid_order_report(order_id));
        }
        run_payment_stage(order_id, amount_cents)
    }

    fn invalid_order_report(order_id: u64) -> Report<OrderError, HasSeverity> {
        Report::new(OrderError::invalid_order(order_id))
            .with_error_code("ORDER.INVALID")
            .with_severity(Severity::Warn)
            .with_category("order")
            .with_retryable(false)
            .attach_note("order validation failed")
            .with_display_cause("required fields missing")
            .attach_payload(
                "order_validation",
                serde_json::json!({
                    "order_id": order_id,
                    "reason": "non-zero order id required"
                }),
                Some("application/json"),
            )
            .push_trace_event_with(
                "order.validate",
                Some(TraceEventLevel::Warn),
                Some(1_713_337_004_000_000_000),
                vec![TraceEventAttribute {
                    key: "order.id".into(),
                    value: AttachmentValue::from(order_id),
                }],
            )
            .with_ctx("order_id", order_id)
    }

    fn run_payment_stage(
        order_id: u64,
        amount_cents: u64,
    ) -> Result<(), Report<OrderError, HasSeverity>> {
        payment::charge(amount_cents).and_then_report(|r| {
            r.with_ctx("order_id", order_id)
                .with_ctx("order_amount_cents", amount_cents)
                .attach_note("order pipeline entered payment stage")
                .with_error_code("ORDER.PAYMENT_FAILED")
                .with_severity(Severity::Error)
                .with_category("order")
                .with_retryable(true)
                .with_display_cause("order payment stage failed")
                .push_trace_event_with(
                    "order.payment",
                    Some(TraceEventLevel::Error),
                    Some(1_713_337_005_000_000_000),
                    vec![
                        TraceEventAttribute {
                            key: "order.id".into(),
                            value: AttachmentValue::from(order_id),
                        },
                        TraceEventAttribute {
                            key: "order.amount_cents".into(),
                            value: AttachmentValue::from(amount_cents),
                        },
                    ],
                )
                .map_err(|_err| OrderError::payment_failed(order_id))
        })?;
        Ok(())
    }
}

mod gateway {
    use super::*;

    /// Handles a single API request and maps failures to the shared scenario error union.
    pub fn handle_request(request_id: &str) -> Result<String, ScenarioReport> {
        match request_id {
            "bad-request" => bad_request(),
            "payment-declined" => payment_declined(),
            "order-network-error" => order_network_error(),
            _ => success_path(),
        }
    }

    fn bad_request() -> Result<String, ScenarioReport> {
        Err(Report::new(ScenarioError::BadRequest {
            reason: "missing auth header".to_owned(),
        })
        .with_severity(Severity::Warn)
        .attach_note("gateway rejected request")
        .with_ctx("route", "/v1/order"))
    }

    fn payment_declined() -> Result<String, ScenarioReport> {
        payment::charge(0).and_then_report(|r| {
            r.with_ctx("route", "/v1/charge")
                .attach_note("gateway forwarding to payment")
                .with_error_code("API.PAYMENT_DECLINED")
                .with_severity(Severity::Warn)
                .with_category("api")
                .with_retryable(false)
                .push_trace_event_with(
                    "gateway.forward.payment",
                    Some(TraceEventLevel::Warn),
                    Some(1_713_337_006_000_000_000),
                    vec![TraceEventAttribute {
                        key: "http.route".into(),
                        value: AttachmentValue::from("/v1/charge"),
                    }],
                )
                .map_err(ScenarioError::Payment)
        })?;
        Ok("OK".to_owned())
    }

    fn order_network_error() -> Result<String, ScenarioReport> {
        order::create_with_amount(9002, 2).and_then_report(|r| {
            r.with_ctx("route", "/v1/order")
                .attach_note("gateway forwarding to order service")
                .with_error_code("API.ORDER_UPSTREAM_FAILURE")
                .with_severity(Severity::Error)
                .with_category("api")
                .with_retryable(true)
                .with_display_cause("order service call failed")
                .push_trace_event_with(
                    "gateway.forward.order",
                    Some(TraceEventLevel::Error),
                    Some(1_713_337_007_000_000_000),
                    vec![TraceEventAttribute {
                        key: "http.route".into(),
                        value: AttachmentValue::from("/v1/order"),
                    }],
                )
                .map_err(ScenarioError::Order)
        })?;
        Ok("OK".to_owned())
    }

    fn success_path() -> Result<String, ScenarioReport> {
        order::create(9001).and_then_report(|r| {
            r.with_ctx("route", "/v1/order")
                .attach_note("gateway forwarding to order service")
                .push_trace_event_with(
                    "gateway.forward.order",
                    Some(TraceEventLevel::Info),
                    Some(1_713_337_008_000_000_000),
                    vec![TraceEventAttribute {
                        key: "http.route".into(),
                        value: AttachmentValue::from("/v1/order"),
                    }],
                )
                .map_err(ScenarioError::Order)
        })?;
        Ok("OK".to_owned())
    }
}

struct TelemetryHandles {
    mode: TelemetryMode,
    tracer_provider: SdkTracerProvider,
    logger_provider: SdkLoggerProvider,
}

#[derive(Debug, Clone)]
enum TelemetryMode {
    Stdout,
    Otlp {
        traces_endpoint: String,
        logs_endpoint: String,
    },
}

#[derive(Debug, Clone, Default)]
struct TelemetryConfig {
    traces_endpoint: Option<String>,
    logs_endpoint: Option<String>,
}

impl TelemetryConfig {
    fn from_env() -> Self {
        let shared = env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
        let traces_endpoint = env::var("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT")
            .ok()
            .or_else(|| shared.clone());
        let logs_endpoint = env::var("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT").ok().or(shared);

        Self {
            traces_endpoint,
            logs_endpoint,
        }
    }
}

impl Drop for TelemetryHandles {
    fn drop(&mut self) {
        let _ = self.logger_provider.shutdown();
        let _ = self.tracer_provider.shutdown();
    }
}

fn init_telemetry() -> TelemetryHandles {
    let resource = Resource::builder_empty()
        .with_attributes([
            KeyValue::new("service.name", "cloud-native-stack"),
            KeyValue::new("deployment.environment", "staging"),
        ])
        .build();

    let config = TelemetryConfig::from_env();
    let traces_endpoint = config
        .traces_endpoint
        .clone()
        .or_else(|| config.logs_endpoint.clone());
    let logs_endpoint = config
        .logs_endpoint
        .clone()
        .or_else(|| config.traces_endpoint.clone());

    let handles = match (traces_endpoint.as_deref(), logs_endpoint.as_deref()) {
        (Some(traces_endpoint), Some(logs_endpoint)) => {
            match build_otlp_telemetry(&resource, traces_endpoint, logs_endpoint) {
                Ok(handles) => handles,
                Err(err) => {
                    eprintln!(
                        "[otel] collector init failed, falling back to stdout exporter: {err}"
                    );
                    build_stdout_telemetry(&resource)
                }
            }
        }
        _ => build_stdout_telemetry(&resource),
    };

    let tracer = handles
        .tracer_provider
        .tracer("diagweave.examples.cloud-native-stack");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .without_time()
        .with_filter(tracing_subscriber::filter::LevelFilter::INFO);
    let subscriber = tracing_subscriber::registry()
        .with(fmt_layer)
        .with(otel_layer);
    let _ = tracing::subscriber::set_global_default(subscriber);

    handles
}

fn build_stdout_telemetry(resource: &Resource) -> TelemetryHandles {
    let tracer_provider = SdkTracerProvider::builder()
        .with_resource(resource.clone())
        .with_simple_exporter(opentelemetry_stdout::SpanExporter::default())
        .build();

    let logger_provider = SdkLoggerProvider::builder()
        .with_resource(resource.clone())
        .with_simple_exporter(opentelemetry_stdout::LogExporter::default())
        .build();

    TelemetryHandles {
        mode: TelemetryMode::Stdout,
        tracer_provider,
        logger_provider,
    }
}

fn build_otlp_telemetry(
    resource: &Resource,
    traces_endpoint: &str,
    logs_endpoint: &str,
) -> Result<TelemetryHandles, String> {
    let tracer_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_protocol(Protocol::HttpBinary)
        .with_endpoint(traces_endpoint)
        .build()
        .map_err(|err| err.to_string())?;

    let log_exporter = opentelemetry_otlp::LogExporter::builder()
        .with_http()
        .with_protocol(Protocol::HttpBinary)
        .with_endpoint(logs_endpoint)
        .build()
        .map_err(|err| err.to_string())?;

    let tracer_provider = SdkTracerProvider::builder()
        .with_resource(resource.clone())
        .with_simple_exporter(tracer_exporter)
        .build();

    let logger_provider = SdkLoggerProvider::builder()
        .with_resource(resource.clone())
        .with_simple_exporter(log_exporter)
        .build();

    Ok(TelemetryHandles {
        mode: TelemetryMode::Otlp {
            traces_endpoint: traces_endpoint.to_owned(),
            logs_endpoint: logs_endpoint.to_owned(),
        },
        tracer_provider,
        logger_provider,
    })
}

fn init_global_context() {
    const REQUEST_ID: &str = "req-20260327-0001";

    let _ = register_global_injector(|| {
        let mut ctx = GlobalContext::default();
        ctx.context.insert("request_id", REQUEST_ID);
        ctx.system.insert("service.name", "cloud-native-stack");
        ctx.system.insert("deployment.environment", "staging");
        if let Some(trace) = current_global_trace_context() {
            ctx.trace = Some(trace);
        }
        Some(ctx)
    });
}

fn main() {
    let telemetry = init_telemetry();
    init_global_context();
    println!("diagweave report json schema version = {REPORT_JSON_SCHEMA_VERSION}");
    match &telemetry.mode {
        TelemetryMode::Stdout => println!("otel transport = stdout"),
        TelemetryMode::Otlp {
            traces_endpoint,
            logs_endpoint,
        } => {
            println!(
                "otel transport = otlp, traces_endpoint={traces_endpoint}, logs_endpoint={logs_endpoint}"
            );
        }
    }

    let scenarios = [
        ("api:bad_request", Scenario::Api("bad-request")),
        ("api:payment_declined", Scenario::Api("payment-declined")),
        (
            "api:order_network_error",
            Scenario::Api("order-network-error"),
        ),
        ("order:invalid", Scenario::Order(0)),
        ("payment:declined", Scenario::Payment(0)),
        ("payment:timeout", Scenario::Payment(1)),
        ("payment:network_error", Scenario::Payment(2)),
        ("api:success_path", Scenario::Api("req-20260327-0001")),
    ];

    for (label, scenario) in scenarios {
        let span = tracing::info_span!("scenario", scenario = label);
        let _entered = span.enter();
        match scenario.run() {
            Ok(value) => println!("[{label}] OK: {value}"),
            Err(report) => render_report(label, report, &telemetry),
        }
    }
}

enum Scenario<'a> {
    Api(&'a str),
    Order(u64),
    Payment(u64),
}

impl<'a> Scenario<'a> {
    fn run(self) -> Result<String, ScenarioReport> {
        match self {
            Scenario::Api(request_id) => gateway::handle_request(request_id),
            Scenario::Order(order_id) => order::create(order_id)
                .map(|()| "OK".to_owned())
                .map_err(|report| report.map_err(ScenarioError::Order)),
            Scenario::Payment(amount_cents) => payment::charge(amount_cents)
                .map(|()| "OK".to_owned())
                .map_err(|report| report.map_err(ScenarioError::Payment)),
        }
    }
}

fn render_report(label: &str, report: ScenarioReport, telemetry: &TelemetryHandles) {
    let pretty_opts = ReportRenderOptions {
        pretty_indent: PrettyIndent::Spaces(2),
        show_empty_sections: false,
        ..ReportRenderOptions::default()
    };
    let json_opts = ReportRenderOptions {
        json_pretty: true,
        ..ReportRenderOptions::default()
    };

    println!("\n--- {label}: Compact (Human) ---");
    println!("{}", report.render(Compact::summary()));

    println!("--- {label}: Pretty (Human) ---");
    println!("{}", report.render(Pretty::new(pretty_opts)));

    println!("\n--- {label}: JSON (ELK) ---");
    println!("{}", report.render(Json::new(json_opts)));

    let ir = report.to_diagnostic_ir();
    let otel = ir.to_otel_envelope();
    let Some(report_record) = otel.records.first() else {
        println!("--- {label}: OTel Envelope ---");
        println!("records_count=0");
        return;
    };

    println!("--- {label}: OTel Envelope ---");
    println!("records_count={}", otel.records.len());
    println!("severity_text={:?}", report_record.severity_text.as_deref());
    println!("severity_number={:?}", report_record.severity_number);
    println!("attributes_count={}", report_record.attributes.len());
    println!("trace_id={:?}", report_record.trace_id.as_deref());
    println!("span_id={:?}", report_record.span_id.as_deref());
    println!("display_causes_count={}", report.display_causes().len());
    println!(
        "origin_source_errors_count={}",
        report.iter_origin_sources().count()
    );
    println!(
        "diagnostic_source_errors_count={}",
        report.iter_diag_sources().count()
    );

    emit_otel_envelope(label, &otel, telemetry);
}

fn current_global_trace_context() -> Option<diagweave::report::GlobalTraceContext> {
    let current_span = tracing::Span::current();
    let otel_context = current_span.context();
    let span_context = otel_context.span().span_context().clone();

    if !span_context.is_valid() {
        return None;
    }

    let trace_id = TraceId::new(span_context.trace_id().to_string()).ok();
    let span_id = SpanId::new(span_context.span_id().to_string()).ok();
    let sampled = span_context.is_sampled();

    Some(diagweave::report::GlobalTraceContext {
        trace_id,
        span_id,
        parent_span_id: None,
        sampled: Some(sampled),
        trace_state: None,
        flags: Some(TraceFlags::new(if sampled { 1 } else { 0 })),
    })
}

fn emit_otel_envelope(
    label: &str,
    otel: &diagweave::adapters::OtelEnvelope<'_>,
    telemetry: &TelemetryHandles,
) {
    let logger = telemetry
        .logger_provider
        .logger("diagweave.examples.cloud-native-stack");
    let record_count = otel.records.len();

    for (idx, event) in otel.records.iter().enumerate() {
        let mut record = logger.create_log_record();
        record.set_target("diagweave.examples.cloud-native-stack.otel");
        record.add_attribute(OTEL_ATTR_SCENARIO_NAME, label.to_owned());
        record.add_attribute(OTEL_ATTR_ENVELOPE_RECORD_COUNT, record_count as i64);
        record.add_attribute(OTEL_ATTR_ENVELOPE_RECORD_INDEX, idx as i64);
        record.add_attribute(OTEL_ATTR_EVENT_NAME, event.name.as_ref().to_owned());
        if let Some(timestamp_unix_nano) = event.timestamp_unix_nano {
            if let Some(timestamp) = unix_nanos_to_system_time(timestamp_unix_nano) {
                record.set_timestamp(timestamp);
            }
        }
        if let Some(severity_number) = event.severity_number {
            let log_severity = otel_log_severity(severity_number);
            record.set_severity_text(log_severity.name());
            record.set_severity_number(log_severity);
        }
        if let (Some(trace_id), Some(span_id)) = (event.trace_id.as_ref(), event.span_id.as_ref()) {
            let trace_id = opentelemetry::TraceId::from_hex(trace_id.as_ref()).ok();
            let span_id = opentelemetry::SpanId::from_hex(span_id.as_ref()).ok();
            if let (Some(trace_id), Some(span_id)) = (trace_id, span_id) {
                let trace_flags = event.trace_flags.map(opentelemetry::TraceFlags::new);
                record.set_trace_context(trace_id, span_id, trace_flags);
            }
        }
        if let Some(trace_context) = event.trace_context.as_ref() {
            if let Some(parent_span_id) = trace_context.parent_span_id.as_ref() {
                record.add_attribute(
                    OTEL_ATTR_TRACE_PARENT_SPAN_ID,
                    parent_span_id.as_ref().to_owned(),
                );
            }
            if let Some(trace_state) = trace_context.trace_state.as_ref() {
                record.add_attribute(
                    OTEL_ATTR_TRACE_STATE,
                    trace_state.as_static_ref().as_ref().to_owned(),
                );
            }
        }
        if let Some(body) = event.body.as_ref() {
            record.add_attribute(OTEL_ATTR_EVENT_BODY, otel_value_to_any_value(body));
        }
        for attr in &event.attributes {
            record.add_attribute(
                scoped_otel_attr_key(attr.key.as_ref()),
                otel_value_to_any_value(&attr.value),
            );
        }
        logger.emit(record);
    }
}

fn otel_log_severity(number: diagweave::adapters::OtelSeverityNumber) -> OtelLogSeverity {
    match number.as_u8() {
        1 => OtelLogSeverity::Trace,
        5 => OtelLogSeverity::Debug,
        9 => OtelLogSeverity::Info,
        13 => OtelLogSeverity::Warn,
        17 => OtelLogSeverity::Error,
        21 => OtelLogSeverity::Fatal,
        _ => OtelLogSeverity::Info,
    }
}

fn otel_value_to_any_value(value: &diagweave::adapters::OtelValue<'_>) -> AnyValue {
    match value {
        diagweave::adapters::OtelValue::String(v) => AnyValue::from(v.as_ref().to_owned()),
        diagweave::adapters::OtelValue::Int(v) => AnyValue::from(*v),
        diagweave::adapters::OtelValue::U64(v) => AnyValue::from(*v as i64),
        diagweave::adapters::OtelValue::Double(v) => AnyValue::from(*v),
        diagweave::adapters::OtelValue::Bool(v) => AnyValue::from(*v),
        diagweave::adapters::OtelValue::Bytes(v) => AnyValue::Bytes(Box::new(v.clone())),
        diagweave::adapters::OtelValue::Array(values) => AnyValue::ListAny(Box::new(
            values.iter().map(otel_value_to_any_value).collect(),
        )),
        diagweave::adapters::OtelValue::KvList(values) => AnyValue::Map(Box::new(
            values
                .iter()
                .map(|attr| {
                    (
                        Key::new(attr.key.as_ref().to_owned()),
                        otel_value_to_any_value(&attr.value),
                    )
                })
                .collect(),
        )),
    }
}

fn unix_nanos_to_system_time(unix_nanos: u64) -> Option<SystemTime> {
    UNIX_EPOCH.checked_add(Duration::from_nanos(unix_nanos))
}

const OTEL_ATTR_NAMESPACE: &str = "diagweave.otel";
const OTEL_ATTR_SCENARIO_NAME: &str = "diagweave.otel.scenario.name";
const OTEL_ATTR_ENVELOPE_RECORD_COUNT: &str = "diagweave.otel.envelope.record.count";
const OTEL_ATTR_ENVELOPE_RECORD_INDEX: &str = "diagweave.otel.envelope.record.index";
const OTEL_ATTR_EVENT_NAME: &str = "diagweave.otel.event.name";
const OTEL_ATTR_EVENT_BODY: &str = "diagweave.otel.event.body";
const OTEL_ATTR_TRACE_PARENT_SPAN_ID: &str = "diagweave.otel.trace_context.parent_span_id";
const OTEL_ATTR_TRACE_STATE: &str = "diagweave.otel.trace_context.trace_state";

fn scoped_otel_attr_key(key: &str) -> String {
    if key.starts_with(OTEL_ATTR_NAMESPACE) {
        key.to_owned()
    } else {
        format!("diagweave.otel.event.attr.{key}")
    }
}
