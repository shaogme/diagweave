# Core Development Reference (AI-Oriented)

## 1. `set!` Macro

### Overview
Used to define a series of structured error enums (Error Sets). It automatically implements composition logic between sets, `From` conversions, snake_case named constructors, report semantics, and enum helpers (`to_report()`/`source()`/`diag()`).

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
| `AuthError::to_report(self)` | `Report<AuthError>` | Converts error instance into a report |
| `AuthError::source(&self)` | `Option<&dyn Error>` | Access to the underlying error source |
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
- **Constructors**: Generates snake_case constructors and `*_report` helpers for inline and external variants.
- **Options**: Supports `#[diagweave(constructor_prefix = "...", report_path = "...")]` on the union enum.
- **Helpers**: Generates `to_report()`, `source()`, and `diag()` on the union enum.

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
| `pub fn to_report(self)` | `Report<Self>` | Converts to a basic report object |
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
The core diagnostic container, wrapping the original error `E` and holding optional "cold data" (metadata, attachments, display-cause chain, trace info) plus per-report `ReportOptions`. Uses a lazy allocation strategy, only allocating heap memory when auxiliary information is added.
Hot path strings such as `category`, `trace_state`, trace event names, and stack trace raw text are stored with shared `StaticRefStr` handles once captured.
`map_err()` is now the recommended entry point for error type transformation; whether it continues to accumulate the origin `source` chain is controlled by `ReportOptions`.

### Declaration and Definition

The `Report` struct is a high-level diagnostic container with lazy allocation for auxiliary data. All three fields are **private** and cannot be directly accessed from outside the module.

```text
pub struct Report<E, State: SeverityState = MissingSeverity> {
    inner: E, // private - wrapped error value
    metadata: ReportMetadata<State>, // private - metadata including severity
    cold: Option<Box<ColdData>>, // private - lazily allocated storage
}
```

**Key Points:**
- `inner`: The wrapped error value (private)
- `metadata`: Contains severity typestate and optional error code/category/retryable (private)
- `cold`: Lazily allocated storage for all optional diagnostic data (private)
- `ReportOptions` is stored inside `ColdData`, which is only allocated when needed
- Access to fields is provided through methods like `inner()`, `severity()`, `options()`, etc.

### Core Construction and Conversion
| Method Declaration | Description |
| :--- | :--- |
| `Report::new(err: E)` | Creates a report |
| `report.inner()` | Gets a reference to the inner error |
| `report.into_inner()` | Consumes the report and returns the original error |
| `report.attachments()` | Returns a list of all associated attachments (`&[Attachment]`) |
| `report.metadata()` | Returns a reference to the raw metadata (`&ReportMetadata`) |
| `report.error_code()` | Reads metadata error code (`Option<&ErrorCode>`) |
| `report.severity()` | Reads severity from the typestate (`Option<Severity>`) |
| `report.category()` | Reads metadata category (`Option<&str>`) |
| `report.retryable()` | Reads metadata retryability (`Option<bool>`) |
| `report.stack_trace()` | Gets associated stack trace info (`Option<&StackTrace>`) |
| `report.trace()` | Gets associated trace information (`Option<&ReportTrace>`) |
| `report.visit_causes(visit)` | Streams display causes with default options |
| `report.visit_causes_ext(options, visit)` | Streams display causes with custom options |
| `report.visit_origin_sources(visit)` | Streams origin source errors with default options |
| `report.visit_origin_src_ext(options, visit)` | Streams origin source errors with custom options |
| `report.visit_diag_sources(visit)` | Streams diagnostic source errors with default options |
| `report.visit_diag_srcs_ext(options, visit)` | Streams diagnostic source errors with custom options |
| `report.iter_origin_sources()` | Iterates origin source errors with default options |
| `report.iter_origin_src_ext(options)` | Iterates origin source errors with custom options |
| `report.iter_diag_sources()` | Iterates diagnostic source errors with default options |
| `report.iter_diag_srcs_ext(options)` | Iterates diagnostic source errors with custom options |
| `report.options()` | Reads the current `ReportOptions` configuration |
| `report.set_options(options: ReportOptions)` | Replaces the current report options |
| `report.set_accumulate_source_chain(accumulate: bool)` | Quick toggle for `map_err()` origin `source` chain accumulation |
| `report.map_err(map: FnOnce(E) -> Outer)`| Maps internal error type while preserving diagnostics; when source chain accumulation is enabled, the old inner error is attached to the new error's `source` chain |

`ReportMetadata<State>` now contains severity typestate and optional metadata fields. The severity is stored as a direct field (not wrapped in Option or Box), while error_code/category/retryable are stored in an inner `Option<Box<MetadataInner>>` for lazy allocation. Read access goes through methods like `error_code()`, `category()`, `retryable()`, and `severity()`. Composition uses builder methods such as `with_error_code(...)` and `set_severity(...)`.

### `ReportOptions` and `GlobalConfig`

`ReportOptions` controls error source chain accumulation and cause collection behavior for an individual `Report`. All fields are `Option<T>` type, and configuration values are resolved with the following priority:

**Configuration Priority**: ReportOptions > GlobalConfig > Profile defaults

#### Profile-Dependent Defaults

| Option | Debug Build | Release Build |
|--------|-------------|---------------|
| `accumulate_source_chain` | `true` | `false` |
| `detect_cycle` | `true` | `false` |
| `max_depth` | `16` | `16` |

#### Configuration Options

- `accumulate_source_chain`: When `Some(true)`, `map_err()` preserves and extends the origin `source` chain; when `Some(false)`, it only transforms the error type without touching the source chain. When `None`, inherits from `GlobalConfig` or profile defaults.
- `max_depth`: Maximum depth of causes to collect during source error traversal. Higher values provide more complete error context but may impact performance for very deep error chains. When `None`, inherits from `GlobalConfig` or defaults to 16.
- `detect_cycle`: When `Some(true)`, the error chain traversal will track visited errors and mark cycles when detected. When `Some(false)`, cycle detection is skipped for better performance. When `None`, inherits from `GlobalConfig` or profile defaults.

#### Builder Methods

| Method | Description |
|--------|-------------|
| `ReportOptions::new()` | Creates options with all fields as `None` (inherits from GlobalConfig or defaults) |
| `.with_accumulate_source_chain(bool)` | Sets source chain accumulation (wraps in `Some(...)`) |
| `.with_max_depth(usize)` | Sets cause collection depth limit |
| `.with_cycle_detection(bool)` | Enables/disables cycle detection |

#### Resolution Methods

| Method | Description |
|--------|-------------|
| `.resolve_accumulate_source_chain()` | Resolves the actual accumulation setting (Priority: ReportOptions > GlobalConfig > Profile default) |
| `.resolve_max_depth()` | Resolves the actual depth limit |
| `.resolve_detect_cycle()` | Resolves the actual cycle detection setting |

#### `GlobalConfig` Global Configuration

`GlobalConfig` provides application-level default configuration. When `ReportOptions` fields are `None`, values are inherited from `GlobalConfig`.

| Method | Description |
|--------|-------------|
| `GlobalConfig::new()` | Creates global config with profile-dependent defaults |
| `.with_accumulate_source_chain(bool)` | Sets default accumulation behavior |
| `.with_max_depth(usize)` | Sets default depth limit |
| `.with_cycle_detection(bool)` | Sets default cycle detection |
| `set_global_config(config)` | Sets global config (can only be called once) |

#### Example Usage

```rust
use diagweave::prelude::*;
use diagweave::report::{GlobalConfig, ReportOptions, set_global_config};

// Set global defaults (call once at application startup)
let config = GlobalConfig::new()
    .with_accumulate_source_chain(true)
    .with_max_depth(32)
    .with_cycle_detection(true);
set_global_config(config).expect("Global config already set");

// Use profile-dependent defaults
let error = std::io::Error::new(std::io::ErrorKind::Other, "test error");
let report = Report::new(error);

// Configure for performance-critical paths
let error2 = std::io::Error::new(std::io::ErrorKind::Other, "test error");
let report2 = Report::new(error2).set_options(
    ReportOptions::new()
        .with_max_depth(8)
        .with_cycle_detection(false)
);

// Enable full diagnostics for debugging
let error3 = std::io::Error::new(std::io::ErrorKind::Other, "test error");
let report3 = Report::new(error3).set_options(
    ReportOptions::new()
        .with_accumulate_source_chain(true)
        .with_max_depth(32)
        .with_cycle_detection(true)
);
```

### `ErrorCode` Design and Conversions
- Internal model:
  - `ErrorCode::Integer(i64)` for compact numeric codes
  - `ErrorCode::String(StaticRefStr)` for symbolic or oversized numeric codes
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

`AttachmentValue::String` also uses `StaticRefStr` internally, so repeated report wrapping can reuse string payloads without copying. Stored attachment keys, payload names/media types, global context keys, and trace/category metadata follow the same storage rule.

### Cause Semantics

- `with_display_cause` / `with_display_causes` accept `impl Display + Send + Sync + 'static` and append display-cause strings (for rendering/IR).
- `with_diag_src_err` appends explicit error objects into the **diagnostic** source chain, requiring `impl Error + Send + Sync + 'static`.
- The origin source chain is maintained by `map_err()` and `Error::source()`; whether the old inner error continues to be chained onto the new error's `source` is decided by `ReportOptions`.

### Global Injection
Used for automatic cross-layer context injection (e.g., RequestID, SessionID).
- **Register**: `register_global_injector(f: fn() -> Option<GlobalContext>)`
- **Global Config**: `set_global_config(config: GlobalConfig)` - Sets application-level default options
- **Timing**: Automatically executed every time a new `Report` instance is created.

| GlobalContext Field | Description |
| :--- | :--- |
| `context` | `ContextMap` globally injected business context |
| `system` | `ContextMap` globally injected system context (namespaced keys recommended, e.g., `service.name`, `deployment.environment`) |
| `error` | `Option<GlobalErrorMeta>` metadata override (`error_code` / `category` / `retryable` / `severity`) |
| `trace` (`trace` feature) | `Option<GlobalTraceContext>` globally injected trace context |

**Note**: `GlobalConfig` and `set_global_config` are a separate global configuration system for setting default `ReportOptions` values; `register_global_injector` is used for injecting context information. The two can be used together.

`TraceId` / `SpanId` / `ParentSpanId` are hex-validated identifiers. Construct them with:
- `TraceId::new("32-hex")` / `SpanId::new("16-hex")` / `ParentSpanId::new("16-hex")`
- `unsafe { TraceId::new_unchecked(...) }` to skip validation

### Chained Configuration Methods

**API Naming Convention**:
- `set_*` methods always replace existing values
- `with_*` methods only set values when not already set (conditional/preserving semantics)

| Method | Parameter Type | Description |
| :--- | :--- | :--- |
| `with_ctx` | `(impl Into<StaticRefStr>, impl Into<ContextValue>)` | Add business context key-value pairs |
| `set_ctx` | `(ContextMap)` | Replace the business context map |
| `with_system` | `(impl Into<StaticRefStr>, impl Into<ContextValue>)` | Add a system context key-value pair |
| `set_system` | `(ContextMap)` | Replace the system context map |
| `set_options` | `ReportOptions` | Replace the current report options |
| `set_accumulate_source_chain` | `bool` | Quick toggle for `map_err()` origin `source` chain accumulation |
| `attach_note` / `attach_printable` | `impl Display + Send + Sync + 'static` | Add remarks or resolution suggestions |
| `attach_payload` / `attach_payload` | `(impl Into<StaticRefStr>, Value, Option<impl Into<StaticRefStr>>)` | Attach named payload (supports media types) |
| `set_severity` | `Severity` | Set severity (Debug, Info, Warn, Error, Fatal), replacing existing value |
| `with_severity` | `Severity` | Set severity only if not already set (preserves underlying diagnostic info) |
| `set_error_code` | `impl Into<ErrorCode>` | Set stable error code (e.g., "E001"), replacing existing value |
| `with_error_code` | `impl Into<ErrorCode>` | Set error code only if not already set (preserves underlying diagnostic info) |
| `set_category` | `impl Into<StaticRefStr>` | Set error category (for monitoring metrics), replacing existing value |
| `with_category` | `impl Into<StaticRefStr>` | Set category only if not already set (preserves underlying diagnostic info) |
| `set_retryable` | `bool` | Mark if the error is suggested to be retried, replacing existing value |
| `with_retryable` | `bool` | Set retryable flag only if not already set (preserves underlying diagnostic info) |
| `with_display_cause` | `impl Display + Send + Sync + 'static` | Add one display-cause string |
| `with_display_causes` | `impl IntoIterator<Item = impl Display + Send + Sync + 'static>` | Add multiple display-cause strings |
| `with_diag_src_err` | `impl Error + Send + Sync + 'static` | Add one explicit error source object |
| `set_stack_trace` | `StackTrace` | Manually associate existing stack trace info, replacing any existing value |
| `with_stack_trace` | `StackTrace` | Manually associate existing stack trace info only if not already present |
| `set_trace` | `ReportTrace` | Set trace information, replacing any existing value |
| `with_trace` | `ReportTrace` | Set trace information only if not already present |
| `set_trace_ids` | `(TraceId, SpanId)` | Set trace and span IDs, replacing any existing values |
| `with_trace_ids` | `(TraceId, SpanId)` | Set trace and span IDs only if not already set |
| `set_parent_span_id` | `ParentSpanId` | Set parent span ID, replacing any existing value |
| `with_parent_span_id` | `ParentSpanId` | Set parent span ID only if not already set |
| `set_trace_sampled` | `bool` | Set whether the trace is sampled, replacing any existing value |
| `with_trace_sampled` | `bool` | Set whether the trace is sampled only if not already set |
| `set_trace_state` | `impl Into<StaticRefStr>` | Set trace state for correlation metadata, replacing any existing value |
| `with_trace_state` | `impl Into<StaticRefStr>` | Set trace state only if not already set |
| `set_trace_flags` | `impl Into<TraceFlags>` | Set trace flags, replacing any existing value |
| `with_trace_flags` | `impl Into<TraceFlags>` | Set trace flags only if not already set |
| `with_trace_event` | `TraceEvent` | Add a trace event to the report |
| `push_trace_event` | `impl Into<StaticRefStr>` | Append a trace event with default fields |
| `push_trace_event_with` | `(impl Into<StaticRefStr>, Option<TraceEventLevel>, Option<u64>, impl IntoIterator<Item = TraceEventAttribute>)` | Append a fully specified trace event |
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
    .with_ctx(
        "request_id",
        "req-123",
    )
    .attach_note("please check the network connection")
    .with_retryable(true)
    .attach_payload("data", vec![1, 2, 3], Some("application/octet-stream"));
#[cfg(feature = "std")]
let report = report.capture_stack_trace();
```

---

## 5. `Result` Extension Traits (`Diagnostic` / `ResultReportExt` / `InspectReportExt`)

### Overview
Provides pipelines for seamless diagnostic info injection on error paths by implementing extension traits for `Result<T, E>` and `Result<T, Report<E>>`.

### Core Traits
#### 1. `Diagnostic` (on `Result<T, E>`)
- `to_report()`: Lifts `Err(E)` to `Err(Report<E>)`.
- `to_report_note(msg)`: Lifts and injects note.
- `diag(...)`: Short-hand for chaining a transformation on the error path. Generic signature:
  `diag<E2, State2>(self, f: impl FnOnce(Report<E>) -> Report<E2, State2>) -> Result<T, Report<E2, State2>>`.
  The closure receives a `Report<E>` and returns a `Report<E2, State2>`. When only adding metadata,
  no explicit type annotations are needed; when transforming the error type (e.g., via `map_err`),
  the return type must be annotated. If you need to control whether the origin source chain continues
  to accumulate, configure the report options first via `set_accumulate_source_chain()`.

#### 2. `ResultReportExt` (on `Result<T, Report<E>>`)
Instead of duplicating every `Report` method, this trait provides a single combinator:
- `and_then_report(|r| r.with_ctx(...).with_severity(...))` — applies any chain of `Report` builder methods on the error path only

The closure receives an owned `Report` and must return an owned `Report`. On the `Ok` path the closure is never invoked, providing natural lazy semantics.

#### 3. `InspectReportExt` (on `Result<T, Report<E>>`)
Read-only helpers for error-path inspection without manually matching `Err`:
- `report_ref()`, `report_metadata()`, `report_attachments()`
- `report_error_code()`, `report_severity()`, `report_category()`, `report_retryable()`


### Usage Example
```rust
use diagweave::prelude::*;
use std::{fs, io};
use std::time::SystemTime;

fn process() -> Result<(), Report<io::Error, HasSeverity>> {
    let file_key = "file";
    let timestamp_key = "timestamp";
    fs::read_to_string("config.toml")
        .to_report()
        .and_then_report(|r| {
            r.with_ctx(file_key, "config.toml")
                .with_severity(Severity::Warn)
                .with_ctx(timestamp_key, ContextValue::String(format!("{:?}", SystemTime::now()).into()))
                .attach_printable("failed to load system config")
        })?;
        
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
| `DisplayCauseChain` | Runtime chain summary with `items: Vec<Arc<dyn Display + Send + Sync + 'static>>`, plus `truncated` and `cycle_detected`. |

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

Attachment note access:
- `Attachment::as_note() -> Option<String>` returns a materialized note string.
- `Attachment::as_note_display() -> Option<&(dyn Display + Send + Sync + 'static)>` returns a zero-allocation display reference.

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
| `show_trace_event_details` | `true`| Whether to show trace event level, timestamp, and attributes in Pretty/JSON output |
| `stack_trace_max_lines` | `24` | Maximum lines for raw stack trace rendering |
| `stack_trace_include_raw` | `true` | Whether to include raw stack trace output when rendering stack traces |
| `stack_trace_include_frames` | `true` | Whether to include parsed stack frames when rendering stack traces |
| `stack_trace_filter` | `All` | Stack frame filtering strategy: `All` (all frames), `AppOnly` (filter std/runtime frames), `AppFocused` (also filter diagnostic-internal frames) |

Preset configurations:
| Preset | Description |
| :--- | :--- |
| `ReportRenderOptions::developer()` | Developer mode: full trace event details, unfiltered stack traces, up to 50 lines |
| `ReportRenderOptions::production()` | Production incident mode: trace event details, app-only frames, up to 15 lines |
| `ReportRenderOptions::minimal()` | Minimal mode: hides trace event details, focused frames, up to 5 lines, hides empty sections and type name |
| `stack_trace_filter` | `All` | Stack frame filtering strategy: `All` (all frames), `AppOnly` (filter std/runtime frames), `AppFocused` (also filter diagnostic-internal frames) |

Preset configurations:
| Preset | Description |
| :--- | :--- |
| `ReportRenderOptions::developer()` | Developer mode: full trace event details, unfiltered stack traces, up to 50 lines |
| `ReportRenderOptions::production()` | Production incident mode: trace event details, app-only frames, up to 15 lines |
| `ReportRenderOptions::minimal()` | Minimal mode: hides trace event details, focused frames, up to 5 lines, hides empty sections and type name |


### Diagnostic Intermediate Representation (`DiagnosticIr`)
Renderers don't process `Report` directly, but first convert it via `to_diagnostic_ir()` to a stable IR structure. The IR keeps the error node, metadata, trace reference, attachments, display causes, origin source errors, diagnostic source errors, and aggregate counters for attachment-related sections.
```rust
use diagweave::render::{
    DiagnosticIrError, DiagnosticIrMetadata,
};
use diagweave::report::{
    Attachment, CauseTraversalState, MissingSeverity, SourceErrorChain,
};
use std::fmt::Display;
use std::sync::Arc;
#[cfg(feature = "trace")]
use diagweave::report::ReportTrace;
#[cfg(feature = "json")]
use diagweave::StaticRefStr;

pub struct DiagnosticIr<'a, State = MissingSeverity> {
    #[cfg(feature = "json")]
    pub schema_version: StaticRefStr,
    pub error: DiagnosticIrError<'a>,
    pub metadata: DiagnosticIrMetadata<'a, State>,
    #[cfg(feature = "trace")]
    pub trace: Option<&'a ReportTrace>,
    pub attachments: &'a [Attachment],
    pub display_causes: &'a [Arc<dyn Display + Send + Sync + 'static>],
    pub display_causes_state: CauseTraversalState,
    pub origin_source_errors: Option<SourceErrorChain>,
    pub diagnostic_source_errors: Option<SourceErrorChain>,
    pub context_count: usize,
    pub attachment_count: usize,
}
```

`DiagnosticIrMetadata` now keeps its internal fields private and exposes read access through methods such as `error_code()`, `severity()`, `category()`, `retryable()`, and `stack_trace()`.

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
#     .with_ctx(
#         "request_id",
#         "req-42",
#     )
#     .attach_printable("note")
#     .attach_payload("body", AttachmentValue::from("ok"), Some("text/plain"))
#     .with_display_cause("retry later")
#     .with_diag_src_err(std::io::Error::other("upstream"));

let ir = report.to_diagnostic_ir();

let context_count = ir.context_count;
let attachment_count = ir.attachment_count;
println!("context_count={context_count}, attachment_count={attachment_count}");
```

`DiagnosticIr` keeps `display_causes` plus both source chains as structured data. In the JSON contract, both `origin_source_errors.type` and `diagnostic_source_errors.type` are `string | null`; `origin` commonly hits `null` due to natural `Error::source()` lossiness.
The IR and adapter layers are borrow-first: error/type/trace string projections prefer `RefStr<'a>` so `to_tracing_fields()` and `to_otel_envelope()` avoid unnecessary `String` materialization on hot paths. OTEL export is intentionally limited to `DiagnosticIr<'a, HasSeverity>`.

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
- **`trace` feature**: Provides the data model, `PreparedTracingEmission`, and `TracingExporterTrait` for custom exporters.
- **`tracing` feature**: Provides the default implementation for the `tracing` crate plus `prepare_tracing` / `emit_tracing` helpers.

### Core API
| Method | Description |
| :--- | :--- |
| `prepare_tracing(&self)` | Available on `Report<_, HasSeverity>` / `DiagnosticIr<_, HasSeverity>`; resolves final report/event levels and returns a prepared emission typestate |
| `emit_tracing(&self)` | Convenience wrapper for `prepare_tracing().emit()` |
| `with_trace_ids(tid, sid)` | Manually binds tracing context (Trace ID / Span ID). Parameters are `TraceId` / `SpanId`. |

### Export Behavior
- **Attribute Mapping**: `Context` is mapped as named fields for the `tracing` event.
- **Structured Fields**: `report_display_causes`, `report_origin_source_errors / report_diagnostic_source_errors`, `report_stack_trace`, `report_context`, and `report_attachments` are emitted as structured debug fields.
- **Empty Sections**: Empty `trace`, `context`, and `attachments` sections are omitted.
- **Trace ID Binding**: If Report contains `TraceContext`, it is automatically associated, or associated via injector from current Span environment.

### Usage Example
```rust
use diagweave::prelude::Report;
use std::fmt;

#[cfg(feature = "trace")]
use diagweave::prelude::{Severity, SpanId, TraceId};
#[cfg(feature = "trace")]
use diagweave::trace::{EmitStats, PreparedTracingEmission, TracingExporterTrait};

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
    fn export_prepared(&self, emission: PreparedTracingEmission<'_>) -> EmitStats {
        emission.stats()
    }
}

let report = Report::new(MyError);
#[cfg(feature = "trace")]
let report = report.with_severity(Severity::Error);

// Bind trace/span ids
#[cfg(feature = "trace")]
let report = report.with_trace_ids(
    TraceId::new("4bf92f3577b34da6a3ce929d0e0e4736").unwrap(),
    SpanId::new("00f067aa0ba902b7").unwrap(),
);

// Export to current tracing span with default options
#[cfg(feature = "tracing")]
report.prepare_tracing().emit();

// Use a custom exporter
#[cfg(feature = "trace")]
report
    .prepare_tracing()
    .emit_with(&MyCustomExporter);
```

---

## 9. Cloud-Native Adaptation (OpenTelemetry)

### Overview
`diagweave` provides adapters deeply integrated with OpenTelemetry (OTel) specifications, supporting conversion of rich diagnostic data into log/event records that follow the OTLP log data model. This area requires the `otel` feature.

### Conversion API
| Method Declaration | Return Type | Description |
| :--- | :--- | :--- |
| `ir.to_otel_envelope()` | `OtelEnvelope<'a>` | Available on `DiagnosticIr<'a, HasSeverity>`; converts to an OTLP-style batch of log/event records |
| `ir.to_tracing_fields()` | `Vec<TracingField<'a>>` | Converts to KV pairs for Tracing/Logging fields |

### OTel Mapping Logic
1. **Record fields**: The primary report becomes a log record with severity, timestamp-ready metadata, trace correlation fields, and a structured `body` error node.
2. **Attributes**: Core error fields, retry/category flags, cause-chain summaries, and attachment/context data are emitted as structured OTEL attributes.
3. **Trace events**: Internal `TraceEvent` values become additional OTLP-style log/event records with their own top-level timestamp, severity, and trace correlation fields.
4. **Structure preservation**: `exception.stacktrace` and `diagnostic_bag.origin_source_errors / diagnostic_bag.diagnostic_source_errors` remain structured instead of string-flattened.

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
let _report = Report::new(MyError).attach_payload(
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
        .to_report()
        .and_then_report(|r| {
            r.with_ctx("db", "primary")
                .set_accumulate_source_chain(true)
                .map_err(AppError::Db)
        })?;
    Ok(())
}
```

### 3. Custom Renderer Implementation
Customize output format (e.g., output to HTML or Web UI) by implementing the `ReportRenderer` trait.
```rust
use diagweave::prelude::*;
use std::fmt::{self, Display, Formatter};

struct MyHtmlRenderer;
impl<E, State> ReportRenderer<E, State> for MyHtmlRenderer
where
    E: Display + std::error::Error + 'static,
    State: SeverityState,
{
    fn render(&self, report: &Report<E, State>, f: &mut Formatter<'_>) -> fmt::Result {
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
| `trace` | No | Trace data model (`ReportTrace`, etc.), prepared emission typestate (`PreparedTracingEmission`), and pluggable exporter trait (`TracingExporterTrait`) |
| `otel` | No | OTLP envelope model (`OtelEnvelope`, `OtelEvent`, `OtelValue`) and `to_otel_envelope()` on `DiagnosticIr<'_, HasSeverity>` |
| `tracing` | No | Default `tracing` crate integration (`TracingExporter`, `prepare_tracing`, `emit_tracing`). Automatically enables `trace`. |

### Requirements Matrix
- **`no_std`**: Supported by disabling default features. Requires `alloc`.
- **`json`**: Requires `serde` with `derive` and `alloc` features, plus `serde_json` with `alloc`.
- **`trace`**: Zero-dependency trace data structures.
- **`otel`**: Requires no extra dependency by itself; enabled explicitly for OTLP envelope export.
- **`tracing`**: Requires `tracing` crate.









