# Core Development Reference (AI-Oriented)

## 1. `set!` Macro

### Overview
Used to define a series of structured error enums (Error Sets). It automatically implements composition logic between sets, `From` conversions, snake_case named constructors, and report semantics.

### Syntax Definition
```text
set! {
    [#[diagweave(Meta)]]
    Ident = { [VariantDecls] } [ | OtherSet ]
    ...
}
```

### Declaration Parameters (Meta)
| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `report_path` | `String` | `"::diagweave::report::Report"` | Path to the `Report` type returned by `*_report` constructors |
| `constructor_prefix` | `String` | `""` | Prefix for generated constructor function names (e.g., `new_`) |

### Supported Attributes
| Attribute | Scope | Parameters | Description |
| :--- | :--- | :--- | :--- |
| `#[display("...")]`| Variant | Format string | Use `{field}` or `{0}` to reference named fields or anonymous tuple fields |
| `#[display(transparent)]` | Variant | None | Delegate `Display` directly to the inner field (requires exactly 1 field) |
| `#[from]` | Variant | None | Mark that this variant can be directly converted from its single field type |

### Core Usage
```rust
use diagweave::set;

set! {
    AuthError = {
        #[display("user {id} not found")]
        UserNotFound { id: u64 },
        
        #[display(transparent)]
        Io(#[from] std::io::Error),
    }

    ServiceError = AuthError | {
        #[display("unexpected error")]
        Unknown
    }
}
```

### Generated Methods (Example: `AuthError`)
| Declaration | Return Type | Description |
| :--- | :--- | :--- |
| `AuthError::user_not_found(id: u64)` | `AuthError` | Snake_case constructor |
| `AuthError::user_not_found_report(id: u64)` | `Report<AuthError>` | Returns a report object containing the current error |
| `AuthError::diag(self)` | `Report<AuthError>` | Convers error instance into a report |
| `From<AuthError> for ServiceError` | `ServiceError` | Automatic mapping from subset to superset |

---

## 2. `union!` Macro

### Overview
Used at architecture boundaries to combine unrelated error types, other error sets, or inline-defined variants.

### Syntax Definition
```text
union! {
    [Attributes]
    [vis] enum Ident = Item1 | Item2 | ...
}
```

### Declaration Items (UnionItem)
| Item Type | Syntax | Description |
| :--- | :--- | :--- |
| External Type | `Path` | Auto-implements `From<Path>` and delegates `Display` |
| External Type Alias | `Path as Ident` | Wraps Path content in a variant named Ident |
| Inline Variant | `{ VariantDecls }` | Defines local variants directly in the union, supporting `#[display]` |

### Core Usage
```rust
use diagweave::union;
use std::fmt;

#[derive(Debug)]
struct AuthError;

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "auth error")
    }
}

impl std::error::Error for AuthError {}

union! {
    pub enum AppError = 
        AuthError |                     // Uses AuthError as variant name automatically
        std::io::Error as Io |          // Explicitly named as Io
        {                               // Inline definition
            #[display("fatal system failure")]
            Fatal
        }
}
```

### Feature Descriptions
- **Auto `Display`**: For external types, generates `match` branches calling `inner.fmt(f)`; for inline variants, generates rendering logic based on `#[display]`.
- **Auto `Error`**: If `Debug` is not provided, `#[derive(Debug)]` is automatically attached.
- **From Injection**: Injects `impl From<T> for Union` for every external member type.

---

## 3. `#[derive(Error)]` Derive Macro

### Overview
Provides convenient implementations of `Display` and `std::error::Error` traits for existing independent `struct` or `enum` types, bridging them into the `diagweave` diagnostic system.

### Supported Attributes
| Attribute | Scope | Parameters | Description |
| :--- | :--- | :--- | :--- |
| `#[display]` | Variant/Struct | `"template"` / `transparent` | Same rendering logic as in `set!` |
| `#[from]` | Field | None | Auto-implements `From<FieldType>`, constructing Self containing this field |
| `#[source]` | Field | None | Marks the field as the return value for `Error::source()` |

### Generated Member Methods
Any type deriving `Error` automatically gains the following helper methods:
| Method Declaration | Return Type | Description |
| :--- | :--- | :--- |
| `pub fn diag(self)` | `Report<Self>` | Converts to a basic report object |
| `pub fn source(&self)` | `Option<&dyn Error>` | Convenient access to the underlying error source |

### Usage Example
```rust
#[derive(diagweave::Error, Debug)]
#[display("system failure")] // Struct-level display template
struct GlobalError {
    #[source] // Manually specify source
    inner: std::io::Error,
    
    msg: String,
}

#[derive(diagweave::Error, Debug)]
enum FileError {
    #[display("read error: {0}")]
    Read(#[from] std::io::Error), // Auto From impl and source
}
```

---

## 4. `Report<E>` Diagnostic Report

### Overview
The core diagnostic container, wrapping the original error `E` and holding optional "cold data" (metadata, attachments, display-cause chain, trace info). Uses a lazy allocation strategy, only allocating heap memory when auxiliary information is added.

### Declaration and Definition
```rust
struct ColdData;

pub struct Report<E> {
    inner: E,
    cold: Option<Box<ColdData>>,
}
```

### Core Construction and Conversion
| Method Declaration | Description |
| :--- | :--- |
| `Report::new(err: E)` | Creates a report |
| `report.inner()` | Gets a reference to the inner error |
| `report.into_inner()` | Consumes the report and returns the original error |
| `report.attachments()` | Returns a list of all associated attachments (`&[Attachment]`) |
| `report.metadata()` | Returns a reference to the raw metadata (`&ReportMetadata`) |
| `report.error_code()` | Reads metadata error code (`Option<&ErrorCode>`) |
| `report.severity()` | Reads metadata severity (`Option<Severity>`) |
| `report.category()` | Reads metadata category (`Option<&str>`) |
| `report.retryable()` | Reads metadata retryability (`Option<bool>`) |
| `report.stack_trace()` | Gets associated stack trace info (`Option<&StackTrace>`) |
| `report.trace()` | Gets associated trace information (`Option<&ReportTrace>`) |
| `report.visit_display_causes(visit)` | Streams display causes with default options |
| `report.visit_display_causes_with(options, visit)` | Streams display causes with custom options |
| `report.visit_source_errors(visit)` | Streams source errors with default options |
| `report.visit_source_errors_with(options, visit)` | Streams source errors with custom options |
| `report.wrap(outer: Outer)` | Wraps current report into another error and links it into the error source chain |
| `report.wrap_with(map: FnOnce(E) -> Outer)`| Maps internal error while preserving all diagnostic info |

### `ErrorCode` Design and Conversions
- Internal model:
  - `ErrorCode::Integer(i64)` for compact numeric codes
  - `ErrorCode::String(Cow<'static, str>)` for symbolic or oversized numeric codes
- Input conversion (`impl Into<ErrorCode>`):
  - Integer inputs (`i8..i128`, `u8..u128`, `isize`, `usize`) attempt `TryInto<i64>`
  - On success: stored as `Integer`
  - On overflow: stored as `String(v.to_string())`
- Output conversion:
  - `TryFrom<ErrorCode>` / `TryFrom<&ErrorCode>` to integer types (`i8..i128`, `u8..u128`, `isize`, `usize`)
  - `From<ErrorCode> for String` and `From<&ErrorCode> for String`
  - `Display` / `to_string()` outputs canonical text form
- Integer extraction errors:
  - `ErrorCodeIntError::InvalidIntegerString`
  - `ErrorCodeIntError::OutOfRange`

### Global Injection
Used for automatic cross-layer context injection (e.g., RequestID, SessionID).
- **Register**: `register_global_injector(f: fn() -> Option<GlobalContext>)`
- **Timing**: Automatically executed every time a new `Report` instance is created.

| GlobalContext Field | Description |
| :--- | :--- |
| `context` | `Vec<(Cow<'static, str>, AttachmentValue)>` globally associated key-value pairs |
| `trace_id` | `Option<Cow<'static, str>>` Automatically bound Trace ID |
| `span_id` | `Option<Cow<'static, str>>` Automatically bound Span ID |

### Chained Configuration Methods
| Method | Parameter Type | Description |
| :--- | :--- | :--- |
| `with_context` / `attach` | `(Ident, impl Into<AttachmentValue>)` | Add context key-value pairs |
| `with_note` / `attach_printable` | `impl Display` | Add remarks or resolution suggestions |
| `with_payload` / `attach_payload` | `(Ident, Value, Option<impl Into<Cow<'static, str>>>)` | Attach named payload (supports media types) |
| `with_severity` | `Severity` | Set severity (Debug, Info, Warn, Error, Fatal) |
| `with_error_code` | `impl Into<ErrorCode>` | Set stable error code (e.g., "E001") |
| `with_category` | `impl Into<Cow<'static, str>>` | Set error category (for monitoring metrics) |
| `with_retryable` | `bool` | Mark if the error is suggested to be retried |
| `with_display_cause` | `impl Display` | Add one display-cause string |
| `with_display_causes` | `impl IntoIterator<Item = impl Display>` | Add multiple display-cause strings |
| `with_source_error` | `impl Error + 'static` | Add one explicit error source object |
| `with_stack_trace` | `StackTrace` | Manually associate existing stack trace info |
| `capture_stack_trace` | None | (std) Capture current stack trace (skip if already exists) |
| `force_capture_stack` | None | (std) Force re-capture stack trace |
| `clear_stack_trace` | None | Remove associated stack trace info |

### Shortcut Rendering Entrypoints
| Method | Return Type | Description |
| :--- | :--- | :--- |
| `compact()` | `impl Display` | Output original error message only |
| `pretty()` | `impl Display` | Output human-friendly segmented detailed diagnostics (default) |
| `json()` | `impl Display` | Output schema-compliant JSON string |
| `render(R)` | `impl Display` | Render using the specified renderer |

### Usage Example
```rust
use diagweave::prelude::*;
use std::fmt;

#[derive(Debug)]
enum MyError {
    Timeout,
}

impl fmt::Display for MyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "timeout")
    }
}

impl std::error::Error for MyError {}

let report = Report::new(MyError::Timeout)
    .with_severity(Severity::Fatal)
    .with_context("request_id", "req-123")
    .with_note("please check the network connection")
    .with_retryable(true)
    .with_payload("data", vec![1, 2, 3], Some("application/octet-stream"));
#[cfg(feature = "std")]
let report = report.capture_stack_trace();
```

---

## 5. `Result` Extension Traits (`Diagnostic` / `ReportResultExt` / `ReportResultInspectExt`)

### Overview
Provides pipelines for seamless diagnostic info injection on error paths by implementing extension traits for `Result<T, E>` and `Result<T, Report<E>>`.

### Core Traits
#### 1. `Diagnostic` (on `Result<T, E>`)
- `diag()`: Lifts `Err(E)` to `Err(Report<E>)`.
- `diag_context(k, v)`: Lifts and injects context.
- `diag_note(msg)`: Lifts and injects note.

#### 2. `ReportResultExt` (on `Result<T, Report<E>>`)
Proxy versions of all `Report` chained configuration methods:
- **Metadata**: `with_severity`, `with_error_code`, `with_category`, `with_retryable`
- **Attachments**: `attach`/`with_context`, `attach_printable`/`with_note`, `attach_payload`/`with_payload`
- **Lazy Loading**: `context_lazy(key, f)`, `note_lazy(f)` (closure runs only on Err)
- **Display Causes**: `with_display_cause(c)`, `with_display_causes(cc)`
- **Source Errors**: `with_source_error(err)`
- **Stack Trace**: `capture_stack_trace()`, `clear_stack_trace()`, `with_stack_trace(st)`
- **Wrapping**: `wrap(outer)`, `wrap_with(map)`

#### 3. `ReportResultInspectExt` (on `Result<T, Report<E>>`)
Read-only helpers for error-path inspection without manually matching `Err`:
- `report_ref()`, `report_metadata()`, `report_attachments()`
- `report_error_code()`, `report_severity()`, `report_category()`, `report_retryable()`


### Usage Example
```rust
use diagweave::prelude::*;
use std::{fs, io};
use std::time::SystemTime;

fn process() -> Result<(), Report<io::Error>> {
    fs::read_to_string("config.toml")
        .diag_context("file", "config.toml") // Converts and attaches context
        .with_severity(Severity::Warn)
        .context_lazy("timestamp", || format!("{:?}", SystemTime::now()).into())
        .attach_printable("failed to load system config")?;
        
    Ok(())
}
```

---

## 6. Display Cause Collection

### Overview
Manages the chain of triggers for a diagnostic. `diagweave` supports not only `std::error::Error` chains but also cross-thread/cross-process event messages.

### Display Cause Data
| Type Name | Description |
| :--- | :--- |
| `Vec<Cow<'static, str>>` | Stores display-cause messages directly; converted into display-cause chain metadata during rendering. |

### Core Data Conversion: `AttachmentValue`
Strongly typed values supported by `Report` attachments, converted automatically from base types:
| Type | Rust Implementation Type | Description |
| :--- | :--- | :--- |
| `Null` | `None` / `()` | Null value |
| `String` | `&str`, `String` | UTF-8 Text |
| `Integer` | `i8..i64` | Signed Integer |
| `Unsigned` | `u8..u64` | Unsigned Integer |
| `Float` | `f32`, `f64` | Floating Point |
| `Bool` | `bool` | Boolean |
| `Array` | `Vec<AttachmentValue>` | List/Sequence |
| `Object` | `BTreeMap<String, AttachmentValue>` | Key-Value mapping |
| `Bytes` | `Vec<u8>` | Binary data content |
| `Redacted` | `{ kind, reason }` | Placeholder for sensitive data |

---

## 7. Rendering and Output

### Overview
Converts `Report` with rich metadata into displayable strings or structured data.

### Rendering Configuration (`ReportRenderOptions`)
| Parameter | Default | Description |
| :--- | :--- | :--- |
| `show_type_name` | `true`| Whether to show full Rust type name of the error |
| `max_source_depth` | `16` | Limit for recursive collection of `source()` |
| `detect_source_cycle` | `true`| Whether to detect and terminate circular cause chains |
| `pretty_indent` | `Spaces(2)` | Indentation style for `Pretty` rendering (supports `Tab`) |
| `json_pretty` | `false`| Whether JSON output has formatted indentation |
| `show_empty_sections` | `true`| Whether to show empty segments (e.g., when Trace is empty) |
| `show_cause_chains_section` | `true`| Whether to show Cause Chain section |
| `show_context_section`| `true`| Whether to show Context K-V section |
| `show_attachments_section`| `true`| Whether to show Attachments (Payload/Note) section |
| `show_stack_trace_section`| `true`| Whether to show Stack Trace section |
| `show_trace_section` | `true`| Whether to show Distributed Tracing (TraceID/Event) section |
| `stack_trace_max_lines` | `24` | Maximum lines for raw stack trace rendering |


### Diagnostic Intermediate Representation (`DiagnosticIr`)
Renderers don't process `Report` directly, but first convert it via `to_diagnostic_ir(options)` to a stable IR structure. The IR keeps header/metadata plus aggregate counters for attachment-related sections.
```rust
use diagweave::render::{
    DiagnosticIrError, DiagnosticIrMetadata,
};
#[cfg(feature = "trace")]
use diagweave::report::ReportTrace;
#[cfg(feature = "json")]
use std::borrow::Cow;

pub struct DiagnosticIr<'a> {
    #[cfg(feature = "json")]
    pub schema_version: Cow<'static, str>,
    pub error: DiagnosticIrError<'a>,
    pub metadata: DiagnosticIrMetadata<'a>,
    #[cfg(feature = "trace")]
    pub trace: Option<&'a ReportTrace>,
    pub context_count: usize,
    pub attachment_count: usize,
}
```

Per-item context/note/payload traversal is exposed via `Report::visit_attachments(...)`.

Use them like this:
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
#     .attach("request_id", "req-42")
#     .attach_printable("note")
#     .attach_payload("body", AttachmentValue::from("ok"), Some("text/plain"))
#     .with_display_cause("retry later")
#     .with_source_error(std::io::Error::other("upstream"));

let ir = report.to_diagnostic_ir(ReportRenderOptions::default());

let context_count = ir.context_count;
let attachment_count = ir.attachment_count;
println!("context_count={context_count}, attachment_count={attachment_count}");
```

`display_causes` / `source_errors` are exported as `DiagnosticIrCauseChainSummary { count, truncated, cycle_detected }`.

### Usage Example
```rust
use diagweave::prelude::{Pretty, Report, ReportRenderOptions};
use diagweave::render::PrettyIndent;

let inner = std::io::Error::new(std::io::ErrorKind::Other, "oops");
let report = Report::new(inner);

// 1. Print Pretty format directly (Stdout)
println!("{}", report.pretty());

// 2. Custom Pretty layout
println!("{}", report.render(Pretty {
    options: ReportRenderOptions {
        pretty_indent: PrettyIndent::Tab,
        max_source_depth: 5,
        ..Default::default()
    }
}));

// 3. Generate JSON
#[cfg(feature = "json")]
let json_str = report.json().to_string();
```

---

## 8. Log System Integration (`Tracing`)

### Overview
Exports diagnostic reports to monitoring systems or log streams.
- **`trace` feature**: Provides the data model and `TracingExporterTrait` for custom exporters.
- **`tracing` feature**: Provides the default implementation for the `tracing` crate and the `emit_tracing` shortcut.

### Core API
| Method | Description |
| :--- | :--- |
| `emit_tracing(&self, options)` | Triggers an `info` level event under current Span, carrying all Report fields as attributes |
| `with_trace_ids(tid, sid)` | Manually binds tracing context (Trace ID / Span ID) |

### Export Behavior
- **Attribute Mapping**: `Context` is mapped as named fields for the `tracing` event.
- **Display Causes**: Display-cause messages are concatenated into an `error.causes` string.
- **Trace ID Binding**: If Report contains `TraceContext`, it is automatically associated, or associated via injector from current Span environment.

### Usage Example
```rust
use diagweave::prelude::{Report, ReportRenderOptions};
use std::fmt;

#[cfg(feature = "trace")]
use diagweave::trace::TracingExporterTrait;

#[derive(Debug)]
struct MyError;

impl fmt::Display for MyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error")
    }
}

impl std::error::Error for MyError {}

#[cfg(feature = "trace")]
struct MyCustomExporter;

#[cfg(feature = "trace")]
impl TracingExporterTrait for MyCustomExporter {
    fn export_ir(&self, _ir: &diagweave::render::DiagnosticIr) {}
}

let report = Report::new(MyError);
let options = ReportRenderOptions::default();

// Export to current tracing span with default options
#[cfg(feature = "tracing")]
report.emit_tracing(ReportRenderOptions::default());

// Use a custom exporter
#[cfg(feature = "trace")]
report.emit_tracing_with(&MyCustomExporter, options);
```

---

## 9. Cloud-Native Adaptation (OpenTelemetry)

### Overview
`diagweave` provides adapters deeply integrated with OpenTelemetry (OTel) specifications, supporting conversion of rich diagnostic data into standard Envelope structures.

### Conversion API
| Method Declaration | Return Type | Description |
| :--- | :--- | :--- |
| `ir.to_otel_envelope()` | `OtelEnvelope` | OTel payload containing attributes and events |
| `ir.to_tracing_fields()` | `Vec<TracingField>` | Converts to KV pairs for Tracing/Logging fields |

### OTel Mapping Logic
1. **Attributes**: Core error fields (message, code, type), severity/retry flags, cause-chain summaries, and report-level counters are mapped.
2. **Events**: Internal `TraceEvent` from `Report` is converted into OTel event sequences.
3. **TraceContext**: TraceID and SpanID are automatically filled into the top level of the Envelope.

---

## 10. Advanced Patterns

### 1. Complex Attachments: Structured JSON Correlation
Leverage `serde_json` macro to inject structured data directly.
```rust
use diagweave::prelude::*;
use std::fmt;

#[cfg(feature = "json")]
use serde_json::json;

#[derive(Debug)]
struct MyError;

impl fmt::Display for MyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error")
    }
}

impl std::error::Error for MyError {}

#[cfg(feature = "json")]
let _report = Report::new(MyError).with_payload(
    "request_meta",
    json!({ "version": "v1", "retry": 3 }),
    Some("application/json")
);
```

### 2. Multi-Level Wrapping Across Layers
Preserve the full error source chain when passing through architectural layers.
```rust
use diagweave::prelude::*;
use std::fmt;

#[derive(Debug)]
struct DatabaseError;

impl fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "db error")
    }
}

impl std::error::Error for DatabaseError {}

#[derive(Debug)]
enum AppError {
    Db(DatabaseError),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Db(_) => write!(f, "app db error"),
        }
    }
}

impl std::error::Error for AppError {}

fn db_operation() -> Result<(), DatabaseError> {
    Err(DatabaseError)
}

fn service_layer() -> Result<(), Report<AppError>> {
    db_operation()
        .diag_context("db", "primary")
        .wrap_with(AppError::Db)?; // Wraps DatabaseError as AppError, preserving DB-layer context
    Ok(())
}
```

### 3. Custom Renderer Implementation
Customize output format (e.g., output to HTML or Web UI) by implementing the `ReportRenderer` trait.
```rust
use diagweave::prelude::*;
use std::fmt::{self, Display, Formatter};

struct MyHtmlRenderer;
impl<E: Display + std::error::Error + 'static> ReportRenderer<E> for MyHtmlRenderer {
    fn render(&self, report: &Report<E>, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", report.pretty())
    }
}
```

---

## 11. Feature Flags

| Feature | Default | Description |
| :--- | :--- | :--- |
| `std` | Yes | Standard library integrations (capture stack trace, global injector, etc.) |
| `json` | No | `Json` renderer support (requires `serde` and `serde_json`) |
| `trace` | No | Trace data model (`ReportTrace`, etc.) and pluggable exporter trait (`TracingExporterTrait`, `emit_tracing_with`) |
| `tracing` | No | Default `tracing` crate integration (`TracingExporter`, `emit_tracing`). Automatically enables `trace`. |

### Requirements Matrix
- **`no_std`**: Supported by disabling default features. Requires `alloc`.
- **`json`**: Requires `serde` with `derive` and `alloc` features, plus `serde_json` with `alloc`.
- **`trace`**: Zero-dependency trace data structures.
- **`tracing`**: Requires `tracing` crate.





