use std::fmt::{Display, Formatter};
use std::io;

use diagweave::prelude::{
    AttachmentValue, Compact, ContextValue, Diagnostic, Error, GlobalContext, HasSeverity,
    ParentSpanId, Pretty, Report, ReportRenderOptions, ReportRenderer, ResultReportExt, Severity,
    SeverityState, SpanId, TraceEventAttribute, TraceEventLevel, TraceId, register_global_injector,
    set, union,
};
use diagweave::render::{Json, PrettyIndent, REPORT_JSON_SCHEMA_VERSION};
use diagweave::report::{StackTrace, StackTraceFormat};
use diagweave::trace::{EmitStats, PreparedTracingEmission, TracingExporterTrait};

// =============================================================================
// Part 1: Error Definitions using diagweave macros
// =============================================================================

set! {
    #[diagweave(report_path = "::diagweave::report::Report")]

    /// Core errors used across the system
    #[derive(Debug, Clone, PartialEq, Eq)]
    BaseError = {
        #[display("resource {id} not found")]
        NotFound { id: String },

        #[display("operation timed out after {0}ms")]
        Timeout(u64),
    }

    /// Authentication specific errors
    #[derive(Debug, Clone)]
    AuthError = {
        #[display("invalid token provided")]
        InvalidToken,

        #[display("session expired for user {user_id}")]
        SessionExpired { user_id: u64 },
    }

    /// Networking errors wrapping standard IO
    #[derive(Debug)]
    NetworkError = {
        #[from]
        #[display(transparent)]
        Io(io::Error),
    }

    /// Composition: A large set combining multiple sub-sets
    #[derive(Debug)]
    pub AppError = BaseError | AuthError | NetworkError | {
        #[display("internal application error: {msg}")]
        Internal { msg: String },
    }
}

set! {
    #[diagweave(constructor_prefix = "new")]
    CtorDemoError = {
        #[display("user {user_id} is locked")]
        UserLocked { user_id: u64 },
    }
}

/// A standalone error using the independent derive macro
#[derive(Debug, Error)]
pub enum DatabaseError {
    #[display("database connection lost: {0}")]
    ConnectionLost(#[source] io::Error),

    #[display("unique constraint violation on {table}.{column}")]
    ConstraintViolation { table: String, column: String },
}

/// A struct error using the independent derive macro
#[derive(Debug, Error)]
#[display("validation failed: {field} - {reason}")]
pub struct ValidationError {
    pub field: String,
    pub reason: String,
}

// Combine everything into a top-level Union for the API layer
union! {
    /// The final error type returned by our API
    #[derive(Debug)]
    pub enum ApiError =
        AppError as App |
        DatabaseError as Db |
        ValidationError |
        {
            #[display("service currently unavailable, retry in {0}s")]
            RetryLater(u32),

            #[display("deprecated endpoint: {path}")]
            Deprecated { path: String },
        }
}

// =============================================================================
// Part 2: Custom Renderers & Exporters
// =============================================================================

struct EmojiRenderer;

impl<E, State> ReportRenderer<E, State> for EmojiRenderer
where
    E: Display,
    State: SeverityState,
{
    fn render(&self, report: &Report<E, State>, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "🚨 ERROR: {} 🚨", report.inner())
    }
}

struct ConsoleExporter;

impl TracingExporterTrait for ConsoleExporter {
    fn export_prepared(&self, emission: PreparedTracingEmission<'_>) -> EmitStats {
        let stats = emission.stats();
        let ir = emission.ir();
        println!(
            "[Tracing Exporter] error={}, severity={:?}, required_severity={:?}, context_count={}, attachment_count={}, stack_trace={}",
            ir.error.message,
            ir.metadata.severity(),
            ir.metadata.required_severity(),
            ir.context_count,
            ir.attachment_count,
            ir.metadata.stack_trace().is_some()
        );
        stats
    }
}

// =============================================================================
// Part 3: Application Logic Simulation
// =============================================================================

fn low_level_io() -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::ConnectionRefused,
        "refused by peer",
    ))
}

fn db_operation() -> Result<(), DatabaseError> {
    low_level_io().map_err(DatabaseError::ConnectionLost)
}

fn service_layer(user_id: u64) -> Result<(), Report<AppError>> {
    db_operation().diag(|r| {
        r.with_ctx("user_id", user_id)
            .attach_note("failing over to secondary database")
            .with_display_cause("db operation failed")
            .with_display_cause("query plan fallback selected")
            .with_diag_src_err(io::Error::other("replica lag detected"))
            .capture_stack_trace()
            .map_err(|db_err| match db_err {
                DatabaseError::ConnectionLost(io) => AppError::Io(io),
                DatabaseError::ConstraintViolation { .. } => AppError::Internal {
                    msg: "db constraint".into(),
                },
            })
    })?;

    Ok(())
}

fn api_handler(request_id: &'static str) -> Result<String, Report<ApiError, HasSeverity>> {
    let (trace_id, span_id, parent_span_id) = parse_trace_ids()?;

    service_layer(1001).and_then_report(|r| {
        r.with_ctx("request_id", request_id)
            .attach_payload(
                "request_meta",
                serde_json::json!({
                    "version": "v1",
                    "retry": 3
                }),
                Some("application/json"),
            )
            .with_error_code("ERR_AUTH_001")
            .with_severity(Severity::Fatal)
            .with_category("auth")
            .with_retryable(false)
            .with_trace_ids(trace_id, span_id)
            .with_parent_span_id(parent_span_id)
            .with_trace_sampled(true)
            .with_trace_state("service=api")
            .with_trace_flags(1)
            .push_trace_event_with(
                "api.handler",
                Some(TraceEventLevel::Error),
                Some(1_713_337_000_000_000_000),
                vec![
                    TraceEventAttribute {
                        key: "http.route".into(),
                        value: AttachmentValue::from("/v1/session"),
                    },
                    TraceEventAttribute {
                        key: "component".into(),
                        value: AttachmentValue::from("gateway"),
                    },
                ],
            )
            .map_err(ApiError::App)
    })?;

    Ok("Success".into())
}

/// Parses trace IDs from hardcoded strings.
/// Returns an error report if any ID is invalid.
fn parse_trace_ids() -> Result<(TraceId, SpanId, ParentSpanId), Report<ApiError, HasSeverity>> {
    let trace_id = TraceId::new("4bf92f3577b34da6a3ce929d0e0e4736")
        .map_err(|_| Report::new(ApiError::retry_later(1)).with_severity(Severity::Error))?;
    let span_id = SpanId::new("00f067aa0ba902b7")
        .map_err(|_| Report::new(ApiError::retry_later(1)).with_severity(Severity::Error))?;
    let parent_span_id = ParentSpanId::new("1111111111111111")
        .map_err(|_| Report::new(ApiError::retry_later(1)).with_severity(Severity::Error))?;
    Ok((trace_id, span_id, parent_span_id))
}

fn print_render_outputs<E, State>(report: &Report<E, State>)
where
    E: std::error::Error + Display + 'static,
    State: SeverityState,
{
    println!("--- Compact Rendering ---");
    println!("{}\n", report.render(Compact::summary()));

    let pretty_opts = ReportRenderOptions {
        pretty_indent: PrettyIndent::Spaces(2),
        show_type_name: true,
        show_empty_sections: true,
        stack_trace_max_lines: 12,
        ..ReportRenderOptions::default()
    };
    println!("--- Pretty Rendering ---");
    println!("{}\n", report.render(Pretty::new(pretty_opts)));

    let json_opts = ReportRenderOptions {
        json_pretty: true,
        ..ReportRenderOptions::default()
    };
    let json = report.render(Json::new(json_opts)).to_string();
    println!("--- JSON Rendering ---");
    println!("{}\n", json);

    let parsed: serde_json::Value = serde_json::from_str(&json)
        .map_err(|e| {
            println!("JSON deserialization failed: {e}");
            e
        })
        .unwrap_or(serde_json::Value::Null);
    println!(
        "JSON check: schema_version={}, causes_present={}\n",
        parsed["schema_version"],
        parsed["diagnostic_bag"]["display_causes"].is_object()
            || parsed["diagnostic_bag"]["origin_source_errors"].is_object()
            || parsed["diagnostic_bag"]["diagnostic_source_errors"].is_object()
    );

    let lean_pretty_opts = ReportRenderOptions {
        show_governance_section: false,
        show_trace_section: false,
        show_stack_trace_section: false,
        show_empty_sections: false,
        ..ReportRenderOptions::default()
    };
    println!("--- Pretty Rendering (Lean Profile) ---");
    println!("{}\n", report.render(Pretty::new(lean_pretty_opts)));
}

fn print_display_causes<E, State>(report: &Report<E, State>)
where
    E: Display + std::error::Error + 'static,
    State: SeverityState,
{
    println!("Display Causes:");
    if report.display_causes().is_empty() {
        println!("  (none)");
    } else {
        let mut idx = 0usize;
        let _ = report.visit_causes(|cause| {
            idx += 1;
            println!("  {}. {}", idx, cause);
            Ok(())
        });
        let state = report
            .visit_causes(|_| Ok(()))
            .unwrap_or_else(|_| diagweave::report::CauseTraversalState::default());
        println!(
            "  summary: count={}, truncated={}, cycle_detected={}",
            report.display_causes().len(),
            state.truncated,
            state.cycle_detected
        );
    }
}

fn print_source_errors<E, State>(report: &Report<E, State>)
where
    E: std::error::Error + 'static,
    State: SeverityState,
{
    println!("Origin Source Errors:");
    if report.iter_origin_sources().next().is_none() {
        println!("  (none)");
    } else {
        let mut idx = 0usize;
        let _ = report.visit_origin_sources(|source| {
            idx += 1;
            println!("  {}. {}", idx, source.error);
            Ok(())
        });
        let mut source_count = 0usize;
        let state = report
            .visit_origin_sources(|_| {
                source_count += 1;
                Ok(())
            })
            .unwrap_or_else(|_| diagweave::report::CauseTraversalState::default());
        println!(
            "  summary: count={}, truncated={}, cycle_detected={}",
            source_count, state.truncated, state.cycle_detected
        );
    }

    println!("Diagnostic Source Errors:");
    if report.iter_diag_sources().next().is_none() {
        println!("  (none)");
    } else {
        let mut idx = 0usize;
        let _ = report.visit_diag_sources(|source| {
            idx += 1;
            println!("  {}. {}", idx, source.error);
            Ok(())
        });
        let mut source_count = 0usize;
        let state = report
            .visit_diag_sources(|_| {
                source_count += 1;
                Ok(())
            })
            .unwrap_or_else(|_| diagweave::report::CauseTraversalState::default());
        println!(
            "  summary: count={}, truncated={}, cycle_detected={}",
            source_count, state.truncated, state.cycle_detected
        );
    }
}

fn print_ir_and_adapters<E>(report: &Report<E, HasSeverity>)
where
    E: std::error::Error + Display + 'static,
{
    let ir = report.to_diagnostic_ir();
    println!("--- Diagnostic IR (Metadata) ---");
    println!("Error Code: {:?}", ir.metadata.error_code());
    println!("Severity: {:?}", ir.metadata.severity());
    println!("Severity: {:?}", ir.metadata.severity());
    println!(
        "StackTrace Present: {}",
        ir.metadata.stack_trace().is_some()
    );

    print_display_causes(report);
    print_source_errors(report);
    println!();

    let tracing_fields = ir.to_tracing_fields();
    let otel = ir.to_otel_envelope();
    println!("Tracing fields count: {}", tracing_fields.len());
    println!("OTel records: {}\n", otel.records.len());

    report.prepare_tracing().emit_with(&ConsoleExporter);
    println!();
}

fn demo_specialized_stores() {
    println!("--- Unified Display Causes ---");

    let report = Result::<(), _>::Err(BaseError::not_found("item_1".into()))
        .diag(|r| {
            r.with_display_cause("cache invalidated")
                .with_display_cause(io::Error::other("hardware failure"))
                .attach_note("local processing delayed")
        })
        .expect_err("demo");
    println!("Report:\n{}\n", report.pretty());
}

fn demo_manual_stack_trace() {
    println!("--- Manual StackTrace API ---");
    let manual =
        StackTrace::new(StackTraceFormat::Raw).with_raw("manual::frame_a\nmanual::frame_b");
    let report = Report::new(BaseError::Timeout(42)).with_stack_trace(manual);
    println!("With stack trace: {}", report);
    let cleared = report.clear_stack_trace();
    println!("After clear: {}\n", cleared);
}

fn demo_type_conversion() {
    let auth = AuthError::InvalidToken;
    let app: AppError = auth.into();
    let _api: ApiError = ApiError::App(app);
    println!("Automatic conversion sequence: Auth -> App -> Api works!");
}

fn demo_context_and_payloads() {
    let report = Report::new(BaseError::Timeout(100))
        .with_ctx("tags", vec!["auth", "slow", "v2"])
        .with_ctx("score", 0.95)
        .with_ctx("byte_values", vec![0xDEu64, 0xAD, 0xBE, 0xEF])
        .with_ctx(
            "secret",
            ContextValue::Redacted {
                kind: Some("password".into()),
                reason: Some("masked".into()),
            },
        );

    println!("--- Context Values ---");
    println!("{}\n", report.pretty());
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_target(true)
        .without_time()
        .try_init();
}

fn init_global_context() {
    let _ = register_global_injector(|| {
        let mut ctx = GlobalContext::default();
        ctx.context.insert("global_request_id", "req-global-001");
        ctx.trace = Some(diagweave::report::GlobalTraceContext {
            trace_id: TraceId::new("4bf92f3577b34da6a3ce929d0e0e4736").ok(),
            span_id: SpanId::new("00f067aa0ba902b7").ok(),
            ..diagweave::report::GlobalTraceContext::default()
        });
        Some(ctx)
    });
}

fn demo_new_capabilities() {
    println!("--- New Capabilities Showcase ---");

    let ctor = CtorDemoError::new_user_locked(7);
    let ctor_report = CtorDemoError::new_user_locked_report(7);
    println!("constructor_prefix: {}", ctor);
    println!("constructor_prefix report: {}", ctor_report);

    let variant_report = AuthError::SessionExpired { user_id: 1001 }.to_report();
    println!("Variant.to_report(): {}", variant_report);

    let auto_ctx = Report::new(BaseError::Timeout(1500));
    println!(
        "global injector auto context: {}\n",
        auto_ctx
            .context()
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .next()
            .unwrap_or_else(|| "(none)".to_owned())
    );
}

fn main() {
    init_tracing();
    init_global_context();

    println!("=== Diagweave Best-Practice Showcase ===\n");
    println!("Schema version: {}\n", REPORT_JSON_SCHEMA_VERSION);

    let base = BaseError::not_found("user_123".into());
    println!("Base constructor: {}", base);
    let report_ctor = AuthError::session_expired_report(1001);
    println!("Report helper constructor: {}\n", report_ctor);

    demo_new_capabilities();

    let request_id = "req-8888";
    let api_result = api_handler(request_id);
    if let Err(report) = api_result {
        print_render_outputs(&report);

        println!("--- Custom Emoji Renderer ---");
        println!("{}\n", report.render(EmojiRenderer));

        print_ir_and_adapters(&report);
    }

    demo_specialized_stores();
    demo_type_conversion();
    demo_manual_stack_trace();
    demo_context_and_payloads();
}
