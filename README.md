# diagweave

Type-safe error-set algebra and diagweaveed runtime diagnostics for Rust.

`diagweave` gives you three pieces that work together:
- `set!`: define structured error sets with concise display templates.
- `union!`: combine multiple error sets/types into one union enum.
- `Report`: carry context, attachments, and source chains with multiple renderers.

## Chinese Guide

- Chinese guide: [`README_CN.md`](README_CN.md)


## Why diagweave

- Avoid hand-written nested error enums and repetitive `From` boilerplate.
- Keep error data structured (named fields / tuple fields) and still human-readable.
- Add diagnostic metadata close to failure points with chain-style APIs.
- Render errors in `Compact`, `Pretty`, or `Json` style, or bring your own renderer.

## Installation

`diagweave` is a normal dependency (macros are re-exported).

```toml
[dependencies]
diagweave = "0.1"
```

If you do not need JSON rendering:

```toml
[dependencies]
diagweave = { version = "0.1", default-features = false }
```

`no_std` mode is supported with `alloc` when `default-features = false`.

## Quick Start

```rust
use diagweave::prelude::{set, Diagnostic, Report};

set! {
    AuthError = {
        #[display("invalid token for user {user_id}")]
        InvalidToken { user_id: u64 },

        #[display("permission denied for role {0}")]
        PermissionDenied(&'static str),
    }
}

fn verify(user_id: u64) -> Result<(), AuthError> {
    Err(AuthError::invalid_token(user_id))
}

fn main() {
    let report: Report<AuthError> = verify(7)
        .diag_context("request_id", "req-001")
        .with_context("retry", 0)
        .with_note("auth gate rejected")
        .expect_err("demo");

    println!("{}", report);           // compact Display
    println!("{}", report.pretty());  // structured pretty output
}
```

## `set!` Guide

### Constructors

`set!` generates constructor helpers per variant in snake_case:
- named variant: `ErrorSet::invalid_token(user_id)`
- tuple variant: `ErrorSet::permission_denied(role)`
- unit variant: `ErrorSet::timeout()`

It also generates `*_report(...)` helpers returning `Report<ErrorSet>`.

You can optionally configure a constructor prefix:

```rust
set! {
    #[diagweave(constructor_prefix = "new")]
    AuthError = {
        #[display("invalid token for user {user_id}")]
        InvalidToken { user_id: u64 },
    }
}

let e = AuthError::new_invalid_token(7);
let r = AuthError::new_invalid_token_report(7);
```

You can decouple the report type path used by generated `*_report(...)` constructors:

```rust
set! {
    #[diagweave(report_path = "crate::custom_runtime::Bag")]
    AuthError = {
        #[display("invalid token")]
        InvalidToken,
    }
}
```

### Derive Support

Each error set can independently configure its `derive` attributes. `Debug` is always added automatically if not present.

```rust
set! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    AuthError = {
        NotFound,
        InvalidToken,
    }

    #[derive(Clone)]
    ApiError = AuthError | {
        Internal,
        RateLimited,
    }
}
```

### Display Template Rules

- Named fields: use field names, for example `{user_id}`.
- Tuple fields: use zero-based indices, for example `{0}`, `{1}`.
- Escape braces with `{{` and `}}`.

### Transparent Delegation and `#[from]`

Variants can delegate their `Display` and `From` implementation.

```rust
set! {
    WrapperError = {
        #[from]
        #[display(transparent)]
        Io(std::io::Error),

        #[display("config error: {0}")]
        Config(&'static str),
    }
}
```

- `#[display(transparent)]`: requires exactly one field; delegates `Display` to that field's implementation.
- `#[from]`: generates `From<T>` for the wrapped type; requires exactly one field.

## `union!` Guide

`union!` is used to combine multiple error types (or error sets defined by `set!`) into a single unified enum. It simplifies error propagation and handling in complex applications.

### Basic Usage

```rust
use diagweave::prelude::{set, union};

set! {
    AuthError = {
        #[display("invalid token")]
        InvalidToken,
    }
}

#[derive(Debug, Clone)]
pub enum DbError {
    ConnectionLost,
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectionLost => write!(f, "database connection lost"),
        }
    }
}

impl std::error::Error for DbError {}

union! {
    /// Combined error type for the API layer
    #[derive(Clone)]
    pub enum ApiError = 
        // External type with implicit variant name (AuthError)
        AuthError | 
        // External type with explicit alias (Db)
        DbError as Db | 
        // Standard library type
        std::io::Error |
        // Inline variants
        {
            #[display("rate limited, retry after {retry_after}s")]
            RateLimited { retry_after: u64 },
        }
}
```

### Key Features

1.  **Automatic `From` Conversion**: `union!` automatically implements `From<T>` for all external types listed. This allows using the `?` operator to propagate errors from sub-modules directly into the union type.
2.  **Display Delegation**: For external types, `fmt::Display` is automatically delegated to the inner error's implementation. For inline variants, you can use the same `#[display]` templates as in `set!`.
3.  **Error Trait**: The generated enum automatically implements `std::error::Error`.
4.  **Automatic Debug**: If `#[derive(Debug)]` is not present, it is added automatically.
5.  **Variant Naming**:
    *   For external types like `path::to::MyError`, the variant name defaults to `MyError`.
    *   Use `as Alias` to override the variant name (e.g., `DbError as Db`).
    *   Inline variants use their declared names.
6.  **Attributes Passthrough**: Any attributes (like `#[derive(...)]` or doc comments) applied to the `union!` block are passed through to the generated enum.


## Standalone `#[derive(Error)]`

`diagweave` provides a standalone `#[derive(Error)]` macro that automatically implements `std::error::Error` and `Display` for your structs and enums. It is designed to be a lightweight and familiar alternative to crates like `thiserror`.

### Basic Usage

```rust
use diagweave::Error;

#[derive(Error, Debug)]
pub enum MyError {
    #[display("io error: {0}")]
    Io(#[from] std::io::Error),

    #[display("custom error: {msg}")]
    Custom { msg: String },

    #[display(transparent)]
    Other(#[source] anyhow::Error),
}
```

### Key Features

1.  **Display Templates**: Use the same template syntax as in `set!` (via `#[display("...")]`).
2.  **Transparent Delegation**: Use `#[display(transparent)]` to delegate both `Display` and `Error::source`.
3.  **Automatic From Conversion**: Use `#[from]` to generate `From<T>` implementations automatically.
4.  **Error::source Support**: Both `#[from]` and `#[source]` are recognized to provide the `source()` method.
5.  **Report Integration**: Derived types automatically get `diag()`, `source()`, and `diag_with::<C>()` methods for seamless integration with the `Report` system.


## Report and Chaining APIs

### Result conversion

- `Diagnostic::diag()` converts `Result<T, E>` to `Result<T, Report<E>>`.
- `ReportResultExt` adds chain methods on `Result<T, Report<E>>`:
  - `attach(key, value)` / `with_context(key, value)`
  - `attach_printable(msg)` / `with_note(msg)`
  - `attach_payload(name, value, media_type)` / `with_payload(...)`
  - `with_error_code(code)` / `with_severity(severity)` / `with_category(category)` / `with_retryable(bool)`
  - `with_source(err)` / `with_event(message)` / `with_causes(iter)`
  - `context_lazy(key, || value)`
  - `note_lazy(|| msg)`
  - `wrap(outer)` / `wrap_with(|inner| outer)`

`Diagnostic` is implemented for all `Result<T, E>`, so `diag()/diag_with()/diag_context()/diag_note()` are always available:

```rust
let report = verify(7)
    .diag_context("request_id", "req-001")
    .with_note("auth gate rejected")
    .expect_err("demo");
```

`Report` governance metadata is first-class (`error_code`, `severity`, `category`, `retryable`).
`Attachment` values support scalar values, arrays, objects, bytes, and redacted markers.

### Global Context Injector (`std` only)

If you want common metadata (for example `request_id` or `trace_id`) to be attached automatically,
you can register a global injector once:

```rust
use diagweave::report::{GlobalContext, register_global_injector};

let _ = register_global_injector(|| {
    let mut ctx = GlobalContext::default();
    ctx.context.push(("request_id".to_owned(), "req-001".into()));
    #[cfg(feature = "trace")]
    {
        ctx.trace_id = Some("trace-abc".to_owned());
        ctx.span_id = Some("span-def".to_owned());
    }
    Some(ctx)
});
```

Notes:
- Available under `feature = "std"`.
- Registration is one-time (`OnceLock`); repeated registration returns `RegisterGlobalContextError`.
- Scheduling/dispatch/concurrency strategy is intentionally owned by user code inside the injector.

### Generic Cause Store examples

Use `diag_with::<Store>()` when you want a non-default cause storage strategy.

`LocalCauseStore` (non-`Send`/`Sync` local error objects):

```rust
use diagweave::prelude::{Diagnostic, ReportResultExt};`r`nuse diagweave::report::LocalCauseStore;

fn parse() -> Result<(), std::num::ParseIntError> {
    "x".parse::<i32>().map(|_| ())
}

let report = parse()
    .diag_with::<LocalCauseStore>()
    .with_local_source(std::fmt::Error)
    .expect_err("demo");
```

`EventOnlyStore` (event-focused diagnostics, no typed error source chain):

```rust
use diagweave::prelude::{Diagnostic, ReportResultExt};`r`nuse diagweave::report::EventOnlyStore;

let report = Err::<(), &str>("network path unavailable")
    .diag_with::<EventOnlyStore>()
    .with_event("fallback route selected")
    .with_source(std::io::Error::other("socket closed"))
    .expect_err("demo");
```

## Rendering

### Built-in renderers

```rust
use diagweave::render::{Compact, Pretty, ReportRenderOptions};

let _ = report.render(Compact).to_string();
let _ = report.render(Pretty::new(ReportRenderOptions::default())).to_string();
```

Structured IR (AST-like intermediate form) is available for direct machine consumption:

```rust
let ir = report.to_diagnostic_ir(ReportRenderOptions::default());
```

Direct adapters for logging/telemetry platforms:

```rust
let tracing_fields = ir.to_tracing_fields();
let otel = ir.to_otel_envelope(); // { attributes, events }
```

Minimal `tracing_subscriber` + `report.emit_tracing(...)` usage:

```rust
#[cfg(feature = "tracing")]
{
    use diagweave::render::ReportRenderOptions;
    use tracing_subscriber::FmtSubscriber;

    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::TRACE)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    report.emit_tracing(ReportRenderOptions::default());
}
```

Minimal custom `TracingExporterTrait` implementation:

```rust
#[cfg(feature = "tracing")]
{
    use diagweave::render::{DiagnosticIr, ReportRenderOptions};
    use diagweave::tracing_export::TracingExporterTrait;

    struct MyExporter;

    impl TracingExporterTrait for MyExporter {
        fn export_ir(&self, ir: &DiagnosticIr) {
            tracing::info!(
                target: "my_app::diag",
                error_message = %ir.error.message,
                stack_trace_present = ir.metadata.stack_trace.is_some(),
                "custom exporter"
            );
        }
    }

    report.emit_tracing_with(&MyExporter, ReportRenderOptions::default());
}
```

`Json` renderer exists when `json` feature is enabled.
The JSON payload includes `schema_version: "v0.1.0"` for compatibility tracking.
Machine-readable schema and Rust type definitions are exposed:
- schema file: `diagweave/schemas/report-v0.1.0.schema.json`
- docs: [`docs/report-json-schema-v0.1.0.md`](docs/report-json-schema-v0.1.0.md)
- exported types: `ReportJsonDocument`, `ReportJsonError`, `ReportJsonMetadata`, `ReportJsonStackTrace`, `ReportJsonStackFrame`, `ReportJsonStackTraceFormat`, `ReportJsonCauseChain`, `ReportJsonCauseNode`, `ReportJsonCauseKind`, `ReportJsonContext`, `ReportJsonAttachment`
- exported constants/functions: `REPORT_JSON_SCHEMA_VERSION`, `REPORT_JSON_SCHEMA_DRAFT`, `report_json_schema()`

```rust
#[cfg(feature = "json")]
{
    use diagweave::render::{Json, ReportRenderOptions};
    let _ = report.render(Json::new(ReportRenderOptions::default())).to_string();
}
```

### Custom renderer

```rust
use std::fmt::{Formatter, Result as FmtResult};
use diagweave::prelude::Report;
use diagweave::render::ReportRenderer;

struct OneLine;

impl<E: std::fmt::Display> ReportRenderer<E> for OneLine {
    fn render(&self, report: &Report<E>, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "ERR: {}", report.inner())
    }
}
```

## Feature Flags

- `std` (default): enables the standard library integration.
- `json` (default): enables `Json` renderer via `serde` + `serde_json` (`alloc` mode supported).
- `trace` (default): enables trace data model APIs (`ReportTrace`, `TraceContext`, `TraceEvent`).
- `tracing`: enables `TracingExporterTrait`, default `TracingExporter`, and `emit_tracing*` APIs.

For `no_std + alloc`, disable default features:

```toml
[dependencies]
diagweave = { version = "0.1", default-features = false }
```

## Workspace Layout

- `diagweave/`: runtime API and macro re-exports.
- `diagweave-macros/`: proc-macro implementation.
- `diagweave-example/`: `publish = false` best-practice sample crate.

## Testing

Run all tests:

```bash
cargo test --workspace
```

Feature matrix:

```bash
bash scripts/test-feature-matrix.sh
```

## License

Licensed under either MIT or Apache-2.0.

