use std::io;

use diagweave::prelude::{
    AttachmentValue, Compact, GlobalContext, HasSeverity, ParentSpanId, Pretty, Report,
    ReportRenderOptions, ResultReportExt, Severity, SpanId, TraceEventAttribute, TraceEventLevel,
    TraceId, register_global_injector, set, union,
};
use diagweave::render::{Json, PrettyIndent, REPORT_JSON_SCHEMA_VERSION};

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

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_target(true)
        .without_time()
        .try_init();
}

fn init_global_context() {
    const REQUEST_ID: &str = "req-20260327-0001";
    const SPAN_ID: &str = "00f067aa0ba902b7";
    const PARENT_SPAN_ID: &str = "1111111111111111";
    const TRACE_ID: &str = "4bf92f3577b34da6a3ce929d0e0e4736";

    let _ = register_global_injector(|| {
        let mut ctx = GlobalContext::default();
        ctx.context.insert("request_id", REQUEST_ID);
        ctx.system.insert("service.name", "cloud-native-stack");
        ctx.system.insert("deployment.environment", "staging");
        ctx.trace = Some(diagweave::report::GlobalTraceContext {
            trace_id: TraceId::new(TRACE_ID).ok(),
            span_id: SpanId::new(SPAN_ID).ok(),
            parent_span_id: ParentSpanId::new(PARENT_SPAN_ID).ok(),
            ..diagweave::report::GlobalTraceContext::default()
        });
        Some(ctx)
    });
}

fn main() {
    init_tracing();
    init_global_context();
    println!("diagweave report json schema version = {REPORT_JSON_SCHEMA_VERSION}");

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
        match scenario.run() {
            Ok(value) => println!("[{label}] OK: {value}"),
            Err(report) => render_report(label, report),
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

fn render_report(label: &str, report: ScenarioReport) {
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
}
