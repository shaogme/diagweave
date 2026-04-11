# diagweave

<div align="center">

**Type-safe error algebra and runtime diagnostic reports for Rust**

[![crates.io](https://img.shields.io/crates/v/diagweave.svg)](https://crates.io/crates/diagweave)
[![docs.rs](https://img.shields.io/docsrs/diagweave)](https://docs.rs/diagweave)
[![license](https://img.shields.io/crates/l/diagweave)](#license)
[![build](https://img.shields.io/github/actions/workflow/status/shaogme/diagweave/ci.yml?branch=main)](https://github.com/shaogme/diagweave/actions)

[English](./README.md) · [简体中文](./README_CN.md)

</div>

---

`diagweave` unifies three layers that are often split across different crates:

- **Type layer**: `set!` / `union!` for composable, strongly-typed error modeling
- **Propagation layer**: `Report` for context, attachments, events, stack trace, and source chain
- **Presentation layer**: `Compact` / `Pretty` / `Json`, plus tracing/telemetry export

## Table of Contents

- [diagweave](#diagweave)
  - [Table of Contents](#table-of-contents)
  - [Why diagweave](#why-diagweave)
  - [Installation](#installation)
  - [Quick Start](#quick-start)
  - [Core Concepts](#core-concepts)
    - [`set!`](#set)
    - [`union!`](#union)
    - [`Report`](#report)
  - [`set!`](#set-1)
  - [`union!`](#union-1)
  - [Standalone `#[derive(Error)]`](#standalone-deriveerror)
  - [`Report` and chain APIs](#report-and-chain-apis)
  - [Rendering and export](#rendering-and-export)
    - [OTEL schema](#otel-schema)
  - [Advanced patterns from `showcase`](#advanced-patterns-from-showcase)
  - [Comparison with other crates](#comparison-with-other-crates)
  - [Feature flags](#feature-flags)
  - [Workspace layout](#workspace-layout)
  - [Testing](#testing)
  - [When to use](#when-to-use)
  - [License](#license)

## Why diagweave

In many Rust projects, error modeling, propagation context, and rendering are handled by separate tools. `diagweave` keeps them on one consistent data model:

1. what failed
2. what runtime context came with the failure
3. how to render/export it for humans and systems

Benefits:

- less manual nested enum boilerplate
- structured diagnostics instead of string-only errors
- chain-friendly context enrichment near the failure site
- one output pipeline for text, JSON, and observability sinks

## Installation

```toml
[dependencies]
diagweave = "0.1"
```

If you do not need default features:

```toml
[dependencies]
diagweave = { version = "0.1", default-features = false }
```

With `default-features = false`, `diagweave` supports `no_std + alloc`.

## Quick Start

```rust
use diagweave::prelude::{set, Diagnostic, Report, ResultReportExt};

set! {
    AuthError = {
        #[display("user {user_id} token is invalid")]
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
        .to_report()
        .and_then_report(|r| {
            r.with_ctx("request_id", "req-001")
                .with_ctx("retry", 0)
                .attach_note("auth gate rejected")
        })
        .expect_err("demo");

    // Or equivalently using `diag` as a shorthand for the two-step chain
    let diag_report = verify(7).diag(|r| {
        r.with_ctx("request_id", "req-001")
            .with_ctx("retry", 0)
            .attach_note("auth gate rejected")
    });

    println!("{}", report);          // compact output
    println!("{}", report.pretty()); // structured output
}
```

## Core Concepts

### `set!`

Define structured error sets for module/domain-local modeling.

### `union!`

Compose multiple sets and external error types into one boundary error.

### `Report`

Wrap an error value and enrich it with runtime diagnostics.

## `set!`

Basic example:

```rust
use diagweave::prelude::set;

set! {
    AuthError = {
        #[display("user {user_id} token is invalid")]
        InvalidToken { user_id: u64 },

        #[display("permission denied for role {0}")]
        PermissionDenied(&'static str),

        #[display("request timed out")]
        Timeout,
    }
}
```

Generated constructors:

- `AuthError::invalid_token(user_id)`
- `AuthError::permission_denied(role)`
- `AuthError::timeout()`
- report helpers: `*_report(...)`

Custom constructor prefix:

```rust
use diagweave::prelude::set;

set! {
    #[diagweave(constructor_prefix = "new")]
    AuthError = {
        #[display("user {user_id} token is invalid")]
        InvalidToken { user_id: u64 },
    }
}

let e = AuthError::new_invalid_token(7);
let r = AuthError::new_invalid_token_report(7);
```

Custom report path:

```rust,ignore
use diagweave::prelude::set;
# mod custom_runtime {
#     pub struct Bag<T>(pub T);
# }

set! {
    #[diagweave(report_path = "crate::custom_runtime::Bag")]
    AuthError = {
        #[display("invalid token")]
        InvalidToken,
    }
}
```

`#[display(transparent)]` and `#[from]` on tuple variants are supported and require exactly one field.

Additional notes:
- enum visibility follows the `set!` declaration (`pub`, `pub(crate)`, or private)
- top-level attributes on the `set!` enum are preserved
- auto helpers: `to_report()`, `source()`, and `diag()` on the enum

## `union!`

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
    pub enum ApiError =
        AuthError |
        DbError as Db |
        std::io::Error |
        {
            #[display("rate limited; retry after {retry_after}s")]
            RateLimited { retry_after: u64 },
        }
}
```

Highlights:

- auto-`From<T>` for listed external types
- display delegation for wrapped external errors
- `as Alias` for variant naming override
- auto `Error` implementation and auto `Debug` backfill
- generated constructors and `*_report` helpers (same as `set!`)
- supports `#[diagweave(constructor_prefix = "...", report_path = "...")]`
- auto helpers: `to_report()`, `source()`, and `diag()` on the enum

## Standalone `#[derive(Error)]`

```rust
use diagweave::Error;

#[derive(Error, Debug)]
pub enum MyError {
    #[display("io error: {0}")]
    Io(#[from] std::io::Error),

    #[display("custom error: {msg}")]
    Custom { msg: String },

    #[display(transparent)]
    Other(#[source] std::io::Error),
}
```

Supports `#[display(...)]`, `#[display(transparent)]`, `#[from]`, and `#[source]`, plus `to_report()` integration.

## `Report` and chain APIs

From `Result<T, E>`:

- `to_report()`
- `to_report_note(message)`

Common enrichers on `Result<T, Report<E>>`:

- `and_then_report(|r| r.with_ctx(key, value).with_severity(...))` — apply any chain of `Report` methods on the error path

Hot-path string fields like `category`, `trace_state`, and trace event names are stored with `StaticRefStr` after capture.
Attachment keys, payload names, payload media types, global context keys, and other stored string metadata also use `StaticRefStr`.
The matching setters accept `impl Into<StaticRefStr>`, so callers can pass owned shared strings without an extra copy.

`map_err()` is the recommended entry point for error type transformation; whether it accumulates the origin `source` chain is controlled by `ReportOptions` (debug: enabled, release: disabled by default).

Read APIs on `Report<E>`:

- `attachments()`, `metadata()`, `stack_trace()`
- `context() -> &ContextMap`, `system() -> &ContextMap`
- `error_code()`, `severity()`, `category()`, `retryable()`
- `visit_causes(visit)` / `visit_causes_ext(options, visit)`
- `visit_origin_sources(visit)` / `visit_origin_src_ext(options, visit)`
- `visit_diag_sources(visit)` / `visit_diag_srcs_ext(options, visit)`
- `iter_origin_sources()` / `iter_origin_src_ext(options)`
- `iter_diag_sources()` / `iter_diag_srcs_ext(options)`
- `options()` — read current `ReportOptions`
- `set_options(options: ReportOptions)` — replace report options
- `set_accumulate_src_chain(accumulate: bool)` — quick toggle for `map_err()` source chain accumulation

Attachment note access:

- `Attachment::as_note() -> Option<String>` (materialized text view)
- `Attachment::as_note_display() -> Option<&(dyn Display + Send + Sync + 'static)>` (zero-allocation display view)

Read APIs on `Result<T, Report<E>>` via `InspectReportExt`:

- `report_ref()`, `report_metadata()`, `report_attachments()`
- `report_error_code()`, `report_severity()`, `report_category()`, `report_retryable()`

`ErrorCode` design:

- dual representation: `Integer(i64)` or `String(StaticRefStr)`
- write path: `set_error_code(x)` or `with_error_code(x)` accepts `impl Into<ErrorCode>`
- `set_error_code(x)` replaces existing value; `with_error_code(x)` only sets if not already set
- integer inputs that fit in `i64` are stored as `Integer`; overflow falls back to decimal `String`
- read path: `TryFrom<ErrorCode>` / `TryFrom<&ErrorCode>` to integer types (`i8..i128`, `u8..u128`, `isize`, `usize`)
- string path: `Into<String>` and `to_string()` are both supported

`AttachmentValue::String` also uses `StaticRefStr` internally, so repeated report wrapping can reuse string payloads without copying.
- integer parse failures return `ErrorCodeIntError::{InvalidIntegerString, OutOfRange}`

Cause semantics:

- `with_display_cause` / `with_display_causes` accept `impl Display + Send + Sync + 'static` and append display-cause strings (for rendering/IR).
- `with_diag_src_err` appends explicit error objects into the **diagnostic** source chain, requiring `impl Error + Send + Sync + 'static`.
- Origin source propagation is maintained by `map_err()` and `Error::source()`; whether `map_err()` continues to chain the old inner error is controlled by `ReportOptions.accumulate_src_chain`.

Global context injector (`std`):

```rust
#[cfg(feature = "std")]
{
    use diagweave::report::{GlobalContext, register_global_injector};

    let _ = register_global_injector(|| {
        let mut ctx = GlobalContext::default();
        ctx.context.insert("request_id", "req-001");
        Some(ctx)
    });
}
```

Trace context uses validated IDs:
- `TraceId::new("32-hex")` / `SpanId::new("16-hex")` / `ParentSpanId::new("16-hex")`
- `unsafe { TraceId::new_unchecked(...) }` to skip validation

## Rendering and export

Built-in renderers:

```rust
use diagweave::render::{Compact, Pretty, ReportRenderOptions, StackTraceFilter};
# use diagweave::prelude::set;
# use diagweave::report::Report;
# set! {
#     AuthError = {
#         #[display("invalid token")]
#         InvalidToken,
#     }
# }
# let report = Report::new(AuthError::invalid_token());

let _ = report.render(Compact::summary()).to_string();
let _ = report.render(Pretty::new(ReportRenderOptions::default())).to_string();
```

Rendering presets:

```rust
use diagweave::render::ReportRenderOptions;

let dev = ReportRenderOptions::developer();     // full details, unfiltered stack traces
let prod = ReportRenderOptions::production();   // trace event details, app-only frames
let minimal = ReportRenderOptions::minimal();   // core info only, focused stack traces
```

Stack trace filtering (`StackTraceFilter`):

- `All` — show every frame (default)
- `AppOnly` — filter out `std::` / `core::` / `alloc::` / `backtrace::` frames
- `AppFocused` — additionally filter out `diagweave::` and diagnostic-internal frames

IR and adapters:

```rust
# use diagweave::prelude::set;
# use diagweave::render::ReportRenderOptions;
# use diagweave::report::{Severity, Report};
# set! {
#     AuthError = {
#         #[display("invalid token")]
#         InvalidToken,
#     }
# }
# let report = Report::new(AuthError::invalid_token())
#     .with_severity(Severity::Error);

let ir = report.to_diagnostic_ir();
#[cfg(feature = "trace")]
let tracing_fields = ir.to_tracing_fields();
#[cfg(feature = "trace")]
assert!(!tracing_fields.is_empty());
#[cfg(feature = "otel")]
let otel = ir.to_otel_envelope();
```

`DiagnosticIr` and the tracing/OTEL adapter outputs are borrow-first views: string fields use `RefStr<'a>` where possible and only materialize owned strings when a projected value cannot safely borrow from the source report. OTEL export is intentionally gated to `DiagnosticIr<'_, HasSeverity>`, so reports must set an explicit `severity` before producing an OTEL envelope.

`DiagnosticIr` keeps render-stable header/metadata plus aggregate counters:

```rust
use diagweave::render::ReportRenderOptions;

# use diagweave::prelude::{AttachmentValue, Report};
# #[derive(Debug)]
# struct DemoError;
# impl core::fmt::Display for DemoError {
#     fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
#         write!(f, "demo error")
#     }
# }
# impl std::error::Error for DemoError {}
# let report = Report::new(DemoError)
#     .with_ctx("request_id", "req-42")
#     .attach_printable("note")
#     .attach_payload("body", AttachmentValue::from("ok"), Some("text/plain"))
#     .with_display_cause("retry later")
#     .with_diag_src_err(std::io::Error::other("upstream"));

let ir = report.to_diagnostic_ir();

let context_count = ir.context_count;
let attachment_count = ir.attachment_count;
println!("context_count={context_count}, attachment_count={attachment_count}");
```

Use `Report::visit_attachments(...)` if you need streaming access to per-item context/note/payload entries.

JSON renderer (`json` feature):

```rust
#[cfg(feature = "json")]
{
    use diagweave::render::{Json, ReportRenderOptions};
#    use diagweave::prelude::set;
#    use diagweave::report::Report;
#    set! {
#        AuthError = {
#            #[display("invalid token")]
#            InvalidToken,
#        }
#    }
#    let report = Report::new(AuthError::invalid_token());
    let _ = report.render(Json::new(ReportRenderOptions::default())).to_string();
}
```

JSON output includes `schema_version: "v0.1.0"`.

- Schema: `diagweave/schemas/report-v0.1.0.schema.json`
- Doc: [`docs/report-json-schema-v0.1.0.md`](docs/report-json-schema-v0.1.0.md)

### OTEL schema

OpenTelemetry envelope output is documented separately and requires the `otel` feature.

- Schema: `diagweave/schemas/report-otel-v0.1.0.schema.json`
- Doc: [`docs/report-otel-schema-v0.1.0.md`](docs/report-otel-schema-v0.1.0.md)

The OTEL adapter keeps the report tree structured where possible:

- the main `exception` record carries a structured `body` instead of a plain string
- `exception.stacktrace` is exported as a `KvList`
- `diagnostic_bag.origin_source_errors / diagnostic_bag.diagnostic_source_errors` preserve `message`; both `origin.type` and `diagnostic.type` are `string | null` in the JSON contract
- empty `trace` / `context` / `attachments` sections are omitted

Tracing export:

```rust
#[cfg(feature = "tracing")]
{
#    use diagweave::prelude::set;
#    use diagweave::report::{Severity, Report};
#    set! {
#        AuthError = {
#            #[display("invalid token")]
#            InvalidToken,
#        }
#    }
#    let report = Report::new(AuthError::invalid_token())
#        .with_severity(Severity::Error);
    report.emit_tracing();
}
```

## Advanced patterns from `showcase`

See [`examples/showcase/src/main.rs`](examples/showcase/src/main.rs) for a runnable showcase including:

- `set!` composition and `union!` API boundary
- custom constructor prefixes
- custom `ReportRenderer`
- custom `TracingExporterTrait`
- unified display causes list
- manual and captured stack trace
- global injector for context/trace propagation

Run it with:

```bash
cargo run -p showcase
```

## Comparison with other crates

| Capability | `thiserror` | `anyhow` | `miette` | `diagweave` |
| --- | --- | --- | --- | --- |
| Typed error definitions | Strong | Weak | Medium | Strong |
| Composable error modeling | Weak | Weak | Weak | Strong |
| Propagation-time context | Weak | Strong | Medium | Strong |
| Structured payloads | Weak | Medium | Medium | Strong |
| Human-readable rendering | Weak | Medium | Strong | Strong |
| Machine-consumable JSON | Weak | Weak | Medium | Strong |
| Tracing/observability export | Weak | Weak | Medium | Strong |

## Feature flags

- `std` (default): std integrations
- `json`: `Json` renderer (`serde` / `serde_json`)
- `trace`: trace data model (`ReportTrace`, etc.), prepared emission typestate (`PreparedTracingEmission`), and pluggable exporter trait (`TracingExporterTrait`)
- `otel`: OTLP envelope model (`OtelEnvelope`, `OtelEvent`, `OtelValue`) and `to_otel_envelope()` on `DiagnosticIr<'_, HasSeverity>`
- `tracing`: default `tracing` crate integration (`TracingExporter`, `prepare_tracing`, `emit_tracing`)

## Workspace layout

- `diagweave/`: runtime APIs + macro re-export
- `diagweave-macros/`: proc-macro implementation
- `examples/showcase/`: runnable best-practice sample (`publish = false`)

## Testing

```bash
cargo test --workspace
```

```bash
bash scripts/test-feature-matrix.sh
```

```powershell
powershell -File scripts/test-feature-matrix.ps1
```

## When to use

`diagweave` is a good fit when you need both typed boundaries and rich runtime diagnostics for services, libraries, or frameworks.

If you only need minimal display derivation or quick app-level propagation, a lighter stack may be enough.

## License

Dual-licensed under MIT OR Apache-2.0.






