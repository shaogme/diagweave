use std::io;

use diagweave::prelude::{
    GlobalContext, Report, ReportRenderOptions, ReportResultExt, SpanId, TraceId,
    register_global_injector, set, union,
};
use diagweave::render::Json;

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

    pub fn charge(amount_cents: u64) -> Result<(), Report<PaymentError>> {
        let _ = amount_cents;

        if amount_cents == 0 {
            return Err(Report::new(PaymentError::Declined)
                .with_note("payment provider declined")
                .with_context("payment_stage", "charge"));
        }
        if amount_cents == 1 {
            return Err(Report::new(PaymentError::from(NetworkError::Timeout(250)))
                .with_note("payment provider timeout")
                .with_context("payment_stage", "charge"));
        }

        match low_level_io() {
            Ok(()) => Ok(()),
            Err(io_err) => {
                let err = NetworkError::Io(io_err);
                Err(Report::new(PaymentError::from(err))
                    .with_note("payment provider network error")
                    .with_context("payment_stage", "charge"))
            }
        }
    }
}

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

    pub fn create(order_id: u64) -> Result<(), Report<OrderError>> {
        if order_id == 0 {
            return Err(Report::new(OrderError::invalid_order(order_id))
                .with_note("order validation failed")
                .with_context("order_id", order_id));
        }

        payment::charge(18800)
            .with_context("order_id", order_id)
            .with_note("order pipeline entered payment stage")
            .wrap_with(|_err| OrderError::payment_failed(order_id))?;

        Ok(())
    }
}

mod gateway {
    use super::*;

    union! {
        #[derive(Debug)]
        pub enum ApiError =
            order::OrderError as Order |
            payment::PaymentError as Payment |
            {
                #[display("bad request: {reason}")]
                BadRequest { reason: String },
            }
    }

    pub fn handle_request(request_id: &str) -> Result<String, Report<ApiError>> {
        if request_id == "bad-request" {
            return Err(
                Report::new(ApiError::bad_request("missing auth header".to_owned()))
                    .with_note("gateway rejected request")
                    .with_context("route", "/v1/order"),
            );
        }
        if request_id == "payment-only" {
            payment::charge(0)
                .with_context("route", "/v1/charge")
                .with_note("gateway forwarding to payment")
                .wrap_with(ApiError::Payment)?;
            return Ok("OK".to_owned());
        }

        order::create(9001)
            .with_context("route", "/v1/order")
            .with_note("gateway forwarding to order service")
            .wrap_with(ApiError::Order)?;

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
    const TRACE_ID: &str = "4bf92f3577b34da6a3ce929d0e0e4736";

    let _ = register_global_injector(|| {
        let mut ctx = GlobalContext::default();
        ctx.context.push(("request_id".into(), REQUEST_ID.into()));
        ctx.context.push(("span_id".into(), SPAN_ID.into()));
        ctx.trace_id = Some(TraceId::new(TRACE_ID).unwrap());
        ctx.span_id = Some(SpanId::new(SPAN_ID).unwrap());
        Some(ctx)
    });
}

fn main() {
    init_tracing();
    init_global_context();

    let scenarios = [
        ("api:bad_request", Scenario::Api("bad-request")),
        ("api:payment_error", Scenario::Api("payment-only")),
        ("order:invalid", Scenario::Order(0)),
        ("payment:declined", Scenario::Payment(0)),
        ("payment:timeout", Scenario::Payment(1)),
        ("api:success_path", Scenario::Api("req-20260327-0001")),
    ];

    for (label, scenario) in scenarios {
        match scenario.run() {
            ScenarioResult::Ok(value) => println!("[{label}] OK: {value}"),
            ScenarioResult::Api(report) => render_report(label, report),
            ScenarioResult::Order(report) => render_report(label, report),
            ScenarioResult::Payment(report) => render_report(label, report),
        }
    }
}

fn find_attr<'a>(
    attrs: &'a [diagweave::adapters::OtelAttribute<'a>],
    key: &str,
) -> Option<&'a diagweave::adapters::OtelValue<'a>> {
    attrs.iter().find_map(|attr| {
        if attr.key == key {
            Some(&attr.value)
        } else {
            None
        }
    })
}

enum Scenario<'a> {
    Api(&'a str),
    Order(u64),
    Payment(u64),
}

impl<'a> Scenario<'a> {
    fn run(self) -> ScenarioResult {
        match self {
            Scenario::Api(request_id) => match gateway::handle_request(request_id) {
                Ok(value) => ScenarioResult::Ok(value),
                Err(report) => ScenarioResult::Api(report),
            },
            Scenario::Order(order_id) => match order::create(order_id) {
                Ok(()) => ScenarioResult::Ok("OK".to_owned()),
                Err(report) => ScenarioResult::Order(report),
            },
            Scenario::Payment(amount_cents) => match payment::charge(amount_cents) {
                Ok(()) => ScenarioResult::Ok("OK".to_owned()),
                Err(report) => ScenarioResult::Payment(report),
            },
        }
    }
}

enum ScenarioResult {
    Ok(String),
    Api(Report<gateway::ApiError>),
    Order(Report<order::OrderError>),
    Payment(Report<payment::PaymentError>),
}

fn render_report(
    label: &str,
    report: Report<impl std::error::Error + std::fmt::Display + 'static>,
) {
    let json_opts = ReportRenderOptions {
        json_pretty: true,
        ..ReportRenderOptions::default()
    };

    println!("\n--- {label}: JSON (ELK) ---");
    println!("{}", report.render(Json::new(json_opts)));

    let ir = report.to_diagnostic_ir();
    let otel = ir.to_otel_envelope();
    let report_record = otel.records.first().expect("report record should exist");

    println!("--- {label}: OTel Envelope ---");
    println!("records_count={}", otel.records.len());
    println!(
        "trace_id={:?}",
        find_attr(&report_record.attributes, "trace_id")
            .map(|v: &diagweave::adapters::OtelValue<'_>| v.to_string())
    );
    println!(
        "span_id={:?}",
        find_attr(&report_record.attributes, "span_id")
            .map(|v: &diagweave::adapters::OtelValue<'_>| v.to_string())
    );
}
