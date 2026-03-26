use std::fmt::{Display, Formatter};
use std::io;

use diagweave::prelude::{
    AttachmentValue, Compact, Diagnostic, Error, GlobalContext, Pretty, Report,
    ReportRenderOptions, ReportRenderer, ReportResultExt, Severity, TraceEvent,
    TraceEventAttribute, TraceEventLevel, register_global_injector, set, union,
};
use diagweave::render::{
    DiagnosticIr, Json, PrettyIndent, REPORT_JSON_SCHEMA_VERSION, ReportJsonDocument,
};
use diagweave::report::{EventOnlyStore, LocalCauseStore, StackTrace, StackTraceFormat};
use diagweave::trace::TracingExporterTrait;

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

        #[display("permission denied for {role}")]
        PermissionDenied { role: String },

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

        #[display("host {host} unreachable: {reason}")]
        Unreachable { host: String, reason: String },
    }

    /// Composition: A large set combining multiple sub-sets
    #[derive(Debug)]
    AppError = BaseError | AuthError | NetworkError | {
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

impl<E: Display> ReportRenderer<E> for EmojiRenderer {
    fn render(&self, report: &Report<E>, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "🚨 ERROR: {} 🚨", report.inner())
    }
}

struct ConsoleExporter;

impl TracingExporterTrait for ConsoleExporter {
    fn export_ir(&self, ir: &DiagnosticIr) {
        let display_causes = ir.metadata.display_causes.as_ref();
        let error_sources = ir.metadata.error_sources.as_ref();
        let display_cause_count = display_causes.map(|c| c.items.len()).unwrap_or(0);
        let error_source_count = error_sources.map(|c| c.items.len()).unwrap_or(0);
        println!(
            "[Tracing Exporter] error={}, severity={:?}, display_causes={}, error_sources={}, stack_trace={}",
            ir.error.message,
            ir.metadata.severity,
            display_cause_count,
            error_source_count,
            ir.metadata.stack_trace.is_some()
        );
        if let Some(causes) = display_causes {
            for (idx, cause) in causes.items.iter().enumerate() {
                println!("  display_cause[{idx}] {}: {}", cause.kind, cause.message);
            }
        }
        if let Some(sources) = error_sources {
            for (idx, source) in sources.items.iter().enumerate() {
                println!("  error_source[{idx}] {}: {}", source.kind, source.message);
            }
        }
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
    db_operation()
        .diag_context("user_id", user_id)
        .with_note("failing over to secondary database")
        .with_cause("db operation failed")
        .with_cause("query plan fallback selected")
        .with_error_source(io::Error::other("replica lag detected"))
        .capture_stack_trace()
        .wrap_with(|db_err| match db_err {
            DatabaseError::ConnectionLost(io) => AppError::Io(io),
            DatabaseError::ConstraintViolation { .. } => AppError::Internal {
                msg: "db constraint".into(),
            },
        })?;

    Ok(())
}

fn api_handler(request_id: &str) -> Result<String, Report<ApiError>> {
    service_layer(1001)
        .with_context("request_id", request_id)
        .with_payload(
            "request_meta",
            serde_json::json!({ "version": "v1", "retry": 3 }),
            Some("application/json".to_owned()),
        )
        .with_error_code("ERR_AUTH_001")
        .with_severity(Severity::Fatal)
        .with_category("auth")
        .with_retryable(false)
        .with_trace_ids("4bf92f3577b34da6a3ce929d0e0e4736", "00f067aa0ba902b7")
        .with_parent_span_id("1111111111111111")
        .with_trace_sampled(true)
        .with_trace_state("service=api")
        .with_trace_flags(1)
        .with_trace_event(TraceEvent {
            name: "api.handler".to_owned(),
            level: Some(TraceEventLevel::Error),
            timestamp_unix_nano: Some(1_713_337_000_000_000_000),
            attributes: vec![
                TraceEventAttribute {
                    key: "http.route".to_owned(),
                    value: AttachmentValue::from("/v1/session"),
                },
                TraceEventAttribute {
                    key: "component".to_owned(),
                    value: AttachmentValue::from("gateway"),
                },
            ],
        })
        .wrap_with(ApiError::App)?;

    Ok("Success".into())
}

fn print_render_outputs<E, C>(report: &Report<E, C>)
where
    E: std::error::Error + Display + 'static,
    C: diagweave::report::CauseStore,
{
    println!("--- Compact Rendering ---");
    println!("{}\n", report.render(Compact));

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

    let parsed: ReportJsonDocument = serde_json::from_str(&json)
        .map_err(|e| {
            println!("JSON deserialization failed: {e}");
            e
        })
        .unwrap_or_default();
    println!(
        "JSON check: schema_version={}, causes_present={}\n",
        parsed.schema_version,
        parsed.metadata.display_causes.is_some() || parsed.metadata.error_sources.is_some()
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

fn print_ir_and_adapters<E, C>(report: &Report<E, C>)
where
    E: std::error::Error + Display + 'static,
    C: diagweave::report::CauseStore,
{
    let ir = report.to_diagnostic_ir(ReportRenderOptions::default());
    println!("--- Diagnostic IR (Metadata) ---");
    println!("Error Code: {:?}", ir.metadata.error_code);
    println!("Severity: {:?}", ir.metadata.severity);
    println!("StackTrace Present: {}", ir.metadata.stack_trace.is_some());
    println!("Display Causes:");
    if let Some(display_causes) = &ir.metadata.display_causes {
        for (idx, cause) in display_causes.items.iter().enumerate() {
            println!("  {}. {}: {}", idx + 1, cause.kind, cause.message);
        }
    } else {
        println!("  (none)");
    }
    println!("Error Causes (Error Sources Chain):");
    if let Some(error_sources) = &ir.metadata.error_sources {
        for (idx, source) in error_sources.items.iter().enumerate() {
            println!("  {}. {}: {}", idx + 1, source.kind, source.message);
        }
    } else {
        println!("  (none)");
    }
    println!();

    let tracing_fields = ir.to_tracing_fields();
    let otel = ir.to_otel_envelope();
    println!("Tracing fields count: {}", tracing_fields.len());
    println!(
        "OTel attributes/events: {}/{}\n",
        otel.attributes.len(),
        otel.events.len()
    );

    report.emit_tracing_with(&ConsoleExporter, ReportRenderOptions::default());
    println!();
}

fn demo_specialized_stores() {
    println!("--- Specialized Cause Stores ---");

    let event_report: Report<BaseError, EventOnlyStore> =
        Result::<(), _>::Err(BaseError::not_found("item_1".into()))
            .diag_with::<EventOnlyStore>()
            .with_cause("cache invalidated")
            .with_cause(io::Error::other("hardware failure"))
            .expect_err("demo");
    println!("EventOnlyStore Report:\n{}\n", event_report.pretty());

    let local_report: Report<BaseError, LocalCauseStore> =
        Result::<(), _>::Err(BaseError::Timeout(3000))
            .diag_with::<LocalCauseStore>()
            .with_note("local processing delayed")
            .expect_err("demo");
    println!("LocalCauseStore Report:\n{}\n", local_report.pretty());
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

fn demo_attachments() {
    let report = Report::new(BaseError::Timeout(100))
        .attach("tags", vec!["auth", "slow", "v2"])
        .attach("score", 0.95)
        .attach("raw_bytes", vec![0xDE, 0xAD, 0xBE, 0xEF])
        .attach(
            "secret",
            AttachmentValue::Redacted {
                kind: Some("password".into()),
                reason: Some("masked".into()),
            },
        );

    println!("--- Diverse Attachments ---");
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
        ctx.context
            .push(("global_request_id".to_owned(), "req-global-001".into()));
        ctx.trace_id = Some("trace-global-abc".to_owned());
        ctx.span_id = Some("span-global-def".to_owned());
        Some(ctx)
    });
}

fn demo_new_capabilities() {
    println!("--- New Capabilities Showcase ---");

    let ctor = CtorDemoError::new_user_locked(7);
    let ctor_report = CtorDemoError::new_user_locked_report(7);
    println!("constructor_prefix: {}", ctor);
    println!("constructor_prefix report: {}", ctor_report);

    let variant_report = AuthError::SessionExpired { user_id: 1001 }.diag();
    println!("Variant.diag(): {}", variant_report);

    let auto_ctx = Report::new(BaseError::Timeout(1500));
    println!(
        "global injector auto context: {}\n",
        auto_ctx
            .attachments()
            .iter()
            .find_map(|a| a.as_context().map(|(k, v)| format!("{k}={v}")))
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
    demo_attachments();
}
