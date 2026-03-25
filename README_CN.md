# diagweave

`diagweave` 是一个面向 Rust 的错误处理库，把“集合代数式错误建模”和“运行时诊断报告”整合在一起。

它由三部分组成：
- `set!`：定义结构化错误集合。
- `union!`：把多个错误集合/错误类型组合成一个 union。
- `Report`：承载上下文、附件、source 链，并支持多种渲染方式。

## 英文文档

- 英文文档：[`README.md`](README.md)

## 为什么使用 diagweave

- 减少手写嵌套错误枚举和重复 `From` 样板。
- 保持错误数据结构化（命名字段/元组字段），同时具备可读消息。
- 在失败点附近链式补充诊断元信息。
- 内置 `Compact` / `Pretty` / `Json` 渲染，也支持自定义 renderer。

## 安装

宏由 `diagweave` 统一 re-export，直接依赖即可：

```toml
[dependencies]
diagweave = "0.1"
```

如果不需要 JSON 渲染：

```toml
[dependencies]
diagweave = { version = "0.1", default-features = false }
```

当关闭默认 feature 时，支持 `no_std + alloc`。

## 快速开始

```rust
use diagweave::prelude::{set, Diagnostic, Report};

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
        .diag_context("request_id", "req-001")
        .with_context("retry", 0)
        .with_note("auth gate rejected")
        .expect_err("demo");

    println!("{}", report);           // 紧凑输出
    println!("{}", report.pretty());  // 结构化输出
}
```

## `set!` 使用说明

### 构造器生成

`set!` 会为每个 variant 生成 snake_case 构造器：
- 命名字段：`ErrorSet::invalid_token(user_id)`
- 元组字段：`ErrorSet::permission_denied(role)`
- 单元变体：`ErrorSet::timeout()`

同时会生成 `*_report(...)` 构造器，直接返回 `Report<ErrorSet>`。

你也可以配置构造器前缀：

```rust
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

你也可以为生成的 `*_report(...)` 构造器解耦 report 路径：

```rust
set! {
    #[diagweave(report_path = "crate::custom_runtime::Bag")]
    AuthError = {
        #[display("invalid token")]
        InvalidToken,
    }
}
```

### 派生 (Derive) 支持

每个错误集都可以独立配置其 `derive` 属性。如果未显式提供 `Debug`，宏会自动添加它以满足 `std::error::Error` 的要求。

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

### `display` 模板规则

- 命名字段使用字段名占位符，例如 `{user_id}`。
- 元组字段使用从 0 开始的索引占位符，例如 `{0}`、`{1}`。
- 字面量大括号使用 `{{` 与 `}}` 转义。

### 透明委托与 `#[from]`

变体可以委托其 `Display` 和 `From` 实现。

```rust
set! {
    WrapperError = {
        #[from]
        #[display(transparent)]
        Io(std::io::Error),

        #[display("配置错误: {0}")]
        Config(&'static str),
    }
}
```

- `#[display(transparent)]`：要求变体恰好有一个字段，并将 `Display` 委托给该字段的实现。
- `#[from]`：为包裹的类型自动生成 `From<T>` 实现，同样要求恰好有一个字段。

## `union!` 使用说明

`union!` 用于将多个错误类型（或通过 `set!` 定义的错误集）组合成一个统一的枚举。它极大简化了复杂应用中的错误传播与处理。

### 基础用法

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
    /// API 层的统一错误类型
    #[derive(Clone)]
    pub enum ApiError = 
        // 外部类型：默认 variant 名为 AuthError
        AuthError | 
        // 外部类型：显式别名为 Db
        DbError as Db | 
        // 标准库类型
        std::io::Error |
        // 内置变体
        {
            #[display("rate limited; retry after {retry_after}s")]
            RateLimited { retry_after: u64 },
        }
}
```

### 核心特性

1.  **自动 `From` 转换**：`union!` 会为所有列出的外部类型自动实现 `From<T>`。这允许你直接使用 `?` 操作符将子模块的错误传播到 union 类型中。
2.  **Display 托管**：对于外部类型，`fmt::Display` 会自动委托给其内部错误的实现；对于内置变体，你可以使用与 `set!` 相同的 `#[display]` 模板。
3.  **Error Trait**：生成的枚举会自动实现 `std::error::Error`。
4.  **自动派生 Debug**：如果未提供 `#[derive(Debug)]`，宏会自动添加它。
5.  **变体命名规则**：
    *   对于外部类型（如 `path::to::MyError`），默认 variant 名取路径最后一段（即 `MyError`）。
    *   使用 `as Alias` 覆盖默认名称（例如 `DbError as Db`）。
    *   内置变体使用其声明的标识符名。
6.  **属性透传**：作用在 `union!` 上的任何属性（如 `#[derive(...)]` 或文档注释）都会透传给生成的枚举。


## 独立 `#[derive(Error)]`

`diagweave` 还提供了一个独立的 `#[derive(Error)]` 宏，它可以为标准结构体和枚举自动实现 `std::error::Error` 和 `Display`。其用法与 `thiserror` 极其相似。

### 基础用法

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

### 核心特性

1.  **Display 模板**：使用与 `set!` 相同的模板语法（`#[display("...")]`）。
2.  **透明委托 (Transparent)**：使用 `#[display(transparent)]`。
3.  **自动 From 实现**：使用 `#[from]` 自动生成 `From<T>` 实现。
4.  **Error::source 支持**：`#[from]` 和 `#[source]` 都会被正确识别以返回 `source`。
5.  **Report 集成**：生成的类型会自动实现 `diag()`、`source()` 和 `diag_with::<C>()` 方法，无缝接入 `Report` 诊断系统。


## Report 与链式 API

### Result 转换

- `Diagnostic::diag()`：把 `Result<T, E>` 转成 `Result<T, Report<E>>`。
- `ReportResultExt`（作用于 `Result<T, Report<E>>`）提供：
  - `attach(key, value)` / `with_context(key, value)`
  - `attach_printable(msg)` / `with_note(msg)`
  - `attach_payload(name, value, media_type)` / `with_payload(...)`
  - `with_error_code(code)` / `with_severity(severity)` / `with_category(category)` / `with_retryable(bool)`
  - `with_source(err)` / `with_event(message)` / `with_causes(iter)`
  - `context_lazy(key, || value)`
  - `note_lazy(|| msg)`
  - `wrap(outer)` / `wrap_with(|inner| outer)`

`Diagnostic` 已为所有 `Result<T, E>` 实现，因此 `diag()/diag_with()/diag_context()/diag_note()` 可直接使用，例如：

```rust
let report = verify(7)
    .diag_context("request_id", "req-001")
    .with_note("auth gate rejected")
    .expect_err("demo");
```

`Report` 内置治理元数据（`error_code`、`severity`、`category`、`retryable`）。
`Attachment` 的 value 支持标量、数组、对象、bytes 与 redacted 标记。

### 全局上下文注入器（仅 `std`）

如果你希望自动附加常见元数据（如 `request_id`、`trace_id`），
可以一次性注册全局注入器：

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

说明：
- 仅在 `feature = "std"` 下可用。
- 注册是一次性的（`OnceLock`）；重复注册会返回 `RegisterGlobalContextError`。
- 调度/分发/并发策略由用户在注入器闭包中自行维护，库本身不提供分配机制。

### 泛型 Cause Store 示例

当你希望使用非默认 cause 存储策略时，使用 `diag_with::<Store>()`。

`LocalCauseStore`（本地错误对象，不强制 `Send`/`Sync`）：

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

`EventOnlyStore`（以事件为主，不保留 typed error source 链）：

```rust
use diagweave::prelude::{Diagnostic, ReportResultExt};`r`nuse diagweave::report::EventOnlyStore;

let report = Err::<(), &str>("network path unavailable")
    .diag_with::<EventOnlyStore>()
    .with_event("fallback route selected")
    .with_source(std::io::Error::other("socket closed"))
    .expect_err("demo");
```

## 渲染能力

### 内置 renderer

```rust
use diagweave::render::{Compact, Pretty, ReportRenderOptions};

let _ = report.render(Compact).to_string();
let _ = report.render(Pretty::new(ReportRenderOptions::default())).to_string();
```

也可直接导出结构化 IR（AST 风格中间表示）供机器消费：

```rust
let ir = report.to_diagnostic_ir(ReportRenderOptions::default());
```

可直接映射到 tracing/OTel 适配对象：

```rust
let tracing_fields = ir.to_tracing_fields();
let otel = ir.to_otel_envelope(); // { attributes, events }
```

`tracing_subscriber` + `report.emit_tracing(...)` 最小示例：

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

自定义 `TracingExporterTrait` 的极简示例：

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

`Json` renderer 仅在 `json` feature 启用时可用：
JSON 输出包含 `schema_version: "v0.1.0"`，用于兼容性追踪。
机器可校验能力已公开：
- schema 文件：`diagweave/schemas/report-v0.1.0.schema.json`
- 说明文档：[`docs/report-json-schema-v0.1.0.md`](docs/report-json-schema-v0.1.0.md)
- 导出类型：`ReportJsonDocument`、`ReportJsonError`、`ReportJsonMetadata`、`ReportJsonStackTrace`、`ReportJsonStackFrame`、`ReportJsonStackTraceFormat`、`ReportJsonCauseChain`、`ReportJsonCauseNode`、`ReportJsonCauseKind`、`ReportJsonContext`、`ReportJsonAttachment`
- 导出常量/函数：`REPORT_JSON_SCHEMA_VERSION`、`REPORT_JSON_SCHEMA_DRAFT`、`report_json_schema()`

```rust
#[cfg(feature = "json")]
{
    use diagweave::render::{Json, ReportRenderOptions};
    let _ = report.render(Json::new(ReportRenderOptions::default())).to_string();
}
```

### 自定义 renderer

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

## Feature

- `std`（默认开启）：启用标准库能力。
- `json`（默认开启）：通过 `serde` + `serde_json` 提供 `Json` 渲染器（支持 `alloc` 模式）。
- `trace`（默认开启）：启用 trace 数据模型 API（`ReportTrace`、`TraceContext`、`TraceEvent`）。
- `tracing`：启用 `TracingExporterTrait`、默认实现 `TracingExporter` 与 `emit_tracing*` API。

如果需要 `no_std + alloc`，请关闭默认 feature：

```toml
[dependencies]
diagweave = { version = "0.1", default-features = false }
```

## 仓库结构

- `diagweave/`：运行时 API 与宏 re-export。
- `diagweave-macros/`：过程宏实现。
- `diagweave-example/`：`publish = false` 的最佳实践示例工程。

## 测试

运行工作区测试：

```bash
cargo test --workspace
```

运行 feature 组合测试：

```bash
bash scripts/test-feature-matrix.sh
```

## 许可证

MIT 或 Apache-2.0 双许可证。

