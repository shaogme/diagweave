# diagweave

<div align="center">

**面向 Rust 的结构化错误建模与运行时诊断报告库**

[![crates.io](https://img.shields.io/crates/v/diagweave.svg)](https://crates.io/crates/diagweave)
[![docs.rs](https://img.shields.io/docsrs/diagweave)](https://docs.rs/diagweave)
[![license](https://img.shields.io/crates/l/diagweave)](#许可证)
[![build](https://img.shields.io/github/actions/workflow/status/shaogme/diagweave/ci.yml?branch=main)](https://github.com/shaogme/diagweave/actions)

[English](./README.md) · [简体中文](./README_CN.md)

</div>

---

`diagweave` 把 Rust 错误处理里常被拆开的三层能力整合到同一数据模型中：

- **类型层**：`set!` / `union!` 负责强类型、可组合的错误建模
- **传播层**：`Report` 负责在传播过程中追加上下文、附件、事件、堆栈与 source 错误链
- **呈现层**：统一渲染为 `Compact` / `Pretty` / `Json`，并可导出到 tracing / 观测系统

## 目录

- [diagweave](#diagweave)
  - [目录](#目录)
  - [为什么使用 diagweave](#为什么使用-diagweave)
  - [安装](#安装)
  - [快速开始](#快速开始)
  - [核心概念](#核心概念)
    - [`set!`](#set)
    - [`union!`](#union)
    - [`Report`](#report)
  - [`set!`](#set-1)
  - [`union!`](#union-1)
  - [独立 `#[derive(Error)]`](#独立-deriveerror)
  - [`Report` 与链式 API](#report-与链式-api)
  - [渲染与导出](#渲染与导出)
    - [OTEL schema](#otel-schema)
  - [来自 `showcase` 的高级模式](#来自-showcase-的高级模式)
  - [与其他库的对比](#与其他库的对比)
  - [Feature](#feature)
  - [仓库结构](#仓库结构)
  - [测试](#测试)
  - [适用场景](#适用场景)
  - [许可证](#许可证)

## 为什么使用 diagweave

在 Rust 项目里，错误“定义、传播、展示”常由不同库分别承担。`diagweave` 的目标是把这三件事建立在一套一致模型上：

1. 错误是什么
2. 这次失败带了哪些现场信息
3. 如何把它输出给人和系统

这带来的收益：

- 减少手写嵌套枚举和重复 `From` 样板
- 错误数据保持结构化，而非退化为字符串
- 在失败点附近链式补充上下文与附件
- 用统一出口渲染文本、JSON 或观测事件

## 安装

```toml
[dependencies]
diagweave = "0.1"
```

如果不需要默认 feature：

```toml
[dependencies]
diagweave = { version = "0.1", default-features = false }
```

关闭默认 feature 后支持 `no_std + alloc`。

## 快速开始

```rust,no_run
use diagweave::prelude::{set, Diagnostic, Report, ReportResultExt};

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

    println!("{}", report);          // 紧凑输出
    println!("{}", report.pretty()); // 结构化输出
}
```

## 核心概念

### `set!`

定义结构化错误集合，适合模块内或领域内错误建模。

### `union!`

组合多个错误集合与外部错误类型，形成统一边界错误。

### `Report`

包装错误值，并在运行时持续补充诊断信息。

## `set!`

基础示例：

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

自动构造器：

- `AuthError::invalid_token(user_id)`
- `AuthError::permission_denied(role)`
- `AuthError::timeout()`
- 以及 report 构造器：`*_report(...)`

自定义前缀：

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

自定义 report 路径：

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

`#[display(transparent)]` 与 `#[from]` 均支持，且都要求“恰好一个字段”。

补充说明：
- 枚举可见性遵循 `set!` 声明（`pub` / `pub(crate)` / 私有）
- `set!` 顶层属性会保留在生成的 enum 上
- 自动生成 `diag()` 与 `source()` 方法

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

核心特性：

- 为列出的外部类型自动实现 `From<T>`
- 外部类型自动委托 `Display`
- 支持 `as Alias` 覆盖默认变体名
- 自动实现 `Error`，缺少 `Debug` 时自动补充
- 自动生成构造器与 `*_report`（同 `set!`）
- 支持 `#[diagweave(constructor_prefix = \"...\", report_path = \"...\")]`
- 自动生成 `diag()` 与 `source()` 方法

## 独立 `#[derive(Error)]`

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

支持 `#[display("...")]`、`#[display(transparent)]`、`#[from]`、`#[source]`，并可直接接入 `diag()`。

## `Report` 与链式 API

从 `Result<T, E>` 转换：

- `diag()`
- `diag_context(key, value)`
- `diag_note(message)`

常用链式增强（`Result<T, Report<E>>`）：

- `with_context`、`with_note`、`with_payload`
- `with_error_code`、`with_severity`、`with_category`、`with_retryable`
- `with_display_cause`、`with_display_causes`、`with_source_error`
- `context_lazy`、`note_lazy`
- `wrap`、`wrap_with`

`category`、`trace_state` 和 trace 事件名等高频字符串在捕获后会以 `StaticRefStr` 共享存储。
附件 key、payload 名称、payload media type、全局上下文 key 等持久化字符串也统一使用 `StaticRefStr`。
对应的设置接口也接受 `impl Into<StaticRefStr>`，可以直接传入共享字符串而不再额外拷贝。

`Report<E>` 的读取接口：

- `attachments()`、`metadata()`、`stack_trace()`
- `error_code()`、`severity()`、`category()`、`retryable()`
- `visit_causes(visit)` / `visit_causes_ext(options, visit)`
- `visit_sources(visit)` / `visit_sources_ext(options, visit)`
- `iter_sources()` / `iter_sources_ext(options)`

Note 附件读取：

- `Attachment::as_note() -> Option<Cow<'_, str>>`（物化后的文本视图）
- `Attachment::as_note_display() -> Option<&(dyn Display + 'static)>`（零分配显示视图）

`Result<T, Report<E>>` 的只读扩展（`ReportResultInspectExt`）：

- `report_ref()`、`report_metadata()`、`report_attachments()`
- `report_error_code()`、`report_severity()`、`report_category()`、`report_retryable()`

`ErrorCode` 设计：

- 双表示：`Integer(i64)` 或 `String(StaticRefStr)`
- 写入路径：`with_error_code(x)` 接收 `impl Into<ErrorCode>`
- 整型输入若可放入 `i64` 则存为 `Integer`；超范围自动降级为十进制字符串 `String`
- 读取路径：支持 `TryFrom<ErrorCode>` / `TryFrom<&ErrorCode>` 到整型（`i8..i128`、`u8..u128`、`isize`、`usize`）
- 字符串路径：同时支持 `Into<String>` 与 `to_string()`
- 整型解析失败错误：`ErrorCodeIntError::{InvalidIntegerString, OutOfRange}`

`AttachmentValue::String` 也使用 `StaticRefStr` 作为内部存储，重复包装同一份 report 时可以减少字符串拷贝。

原因语义说明：

- `with_display_cause` / `with_display_causes` 接收 `impl Display`，并追加到展示原因字符串链（用于渲染与 IR）。
- `with_source_error` 用于显式追加错误对象到 source 链元数据。
- 真正的错误传播链由 `with_source_error`、`wrap` / `wrap_with` 与 `Error::source()` 共同维护。

全局上下文注入（`std`）：

```rust
#[cfg(feature = "std")]
{
    use diagweave::report::{GlobalContext, register_global_injector};

    let _ = register_global_injector(|| {
        let mut ctx = GlobalContext::default();
        ctx.context.push(("request_id".into(), "req-001".into()));
        Some(ctx)
    });
}
```

Trace 上下文使用已校验的 ID：
- `TraceId::new("32位hex")` / `SpanId::new("16位hex")` / `ParentSpanId::new("16位hex")`
- `unsafe { TraceId::new_unchecked(...) }` 可跳过校验

## 渲染与导出

内置渲染器：

```rust
use diagweave::render::{Compact, Pretty, ReportRenderOptions};
# use diagweave::prelude::set;
# use diagweave::report::Report;
# set! {
#     AuthError = {
#         #[display("invalid token")]
#         InvalidToken,
#     }
# }
# let report = Report::new(AuthError::invalid_token());

let _ = report.render(Compact).to_string();
let _ = report.render(Pretty::new(ReportRenderOptions::default())).to_string();
```

IR 与适配器：

```rust
# use diagweave::prelude::set;
# use diagweave::render::ReportRenderOptions;
# use diagweave::report::Report;
# set! {
#     AuthError = {
#         #[display("invalid token")]
#         InvalidToken,
#     }
# }
# let report = Report::new(AuthError::invalid_token());

let ir = report.to_diagnostic_ir();
#[cfg(feature = "trace")]
let tracing_fields = ir.to_tracing_fields();
#[cfg(feature = "trace")]
assert!(!tracing_fields.is_empty());
#[cfg(feature = "otel")]
let otel = ir.to_otel_envelope();
```

`DiagnosticIr` 以及 tracing/OTEL 适配器输出现在优先采用借用视图：能借用 report 内部字符串时使用 `RefStr<'a>`，只有在无法安全借用的投影值上才物化 owned 字符串。

`DiagnosticIr` 主要包含稳定的头部/元数据和聚合计数：

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

let ir = report.to_diagnostic_ir();

let context_count = ir.context_count;
let attachment_count = ir.attachment_count;
println!("context_count={context_count}, attachment_count={attachment_count}");
```

如果需要逐项流式读取上下文/note/payload，可使用 `Report::visit_attachments(...)`。

JSON 渲染（`json` feature）：

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

JSON 输出固定包含 `schema_version: "v0.1.0"`：

- Schema：`diagweave/schemas/report-v0.1.0.schema.json`
- 文档：[`docs/report-json-schema-v0.1.0.md`](docs/report-json-schema-v0.1.0.md)

### OTEL schema

OpenTelemetry 输出的 envelope 单独记录在这里，需要 `otel` feature：

- Schema：`diagweave/schemas/report-otel-v0.1.0.schema.json`
- 文档：[`docs/report-otel-schema-v0.1.0.md`](docs/report-otel-schema-v0.1.0.md)

OTEL 适配器会尽量保留树状结构：

- 主 `exception` 记录的 `body` 保持为结构化值，而不是纯字符串
- `exception.stacktrace` 以 `KvList` 形式输出
- `diagnostic_bag.source_errors` 同时保留 `message` 和 `type`
- 空的 `trace` / `context` / `attachments` 部分默认会省略

tracing 导出：

```rust
#[cfg(feature = "tracing")]
{
#    use diagweave::prelude::set;
#    use diagweave::report::Report;
#    set! {
#        AuthError = {
#            #[display("invalid token")]
#            InvalidToken,
#        }
#    }
#    let report = Report::new(AuthError::invalid_token());
    report.emit_tracing();
}
```

## 来自 `showcase` 的高级模式

参考 [`examples/showcase/src/main.rs`](examples/showcase/src/main.rs) 可运行样例，包含：

- `set!` 组合与 `union!` 边界
- 自定义构造器前缀
- 自定义 `ReportRenderer`
- 自定义 `TracingExporterTrait`
- 统一的展示原因列表
- 手动与自动堆栈追踪
- 全局注入器实现上下文/trace 注入

运行方式：

```bash
cargo run -p showcase
```

## 与其他库的对比

| 维度 | `thiserror` | `anyhow` | `miette` | `diagweave` |
| --- | --- | --- | --- | --- |
| 强类型错误定义 | 强 | 弱 | 中 | 强 |
| 组合式错误建模 | 弱 | 弱 | 弱 | 强 |
| 传播期上下文 | 弱 | 强 | 中 | 强 |
| 结构化附件 / payload | 弱 | 中 | 中 | 强 |
| 人类可读渲染 | 弱 | 中 | 强 | 强 |
| 机器可消费 JSON | 弱 | 弱 | 中 | 强 |
| tracing / 观测导出 | 弱 | 弱 | 中 | 强 |

## Feature

- `std`（默认）：标准库能力
- `json`：`Json` 渲染器（`serde` / `serde_json`）
- `trace`：trace 数据模型（`ReportTrace` 等）与可插拔导出器 Trait（`TracingExporterTrait`、`emit_tracing_with`）
- `otel`：OTLP envelope 模型（`OtelEnvelope`、`OtelEvent`、`OtelValue`）与 `to_otel_envelope()`
- `tracing`：默认 `tracing` 生态集成（`TracingExporter`、`emit_tracing`）

## 仓库结构

- `diagweave/`：运行时 API 与宏 re-export
- `diagweave-macros/`：过程宏实现
- `examples/showcase/`：可运行最佳实践样例（`publish = false`）

## 测试

```bash
cargo test --workspace
```

```bash
bash scripts/test-feature-matrix.sh
```

```powershell
powershell -File scripts/test-feature-matrix.ps1
```

## 适用场景

当你需要“强类型错误边界 + 丰富运行时诊断 + 统一机器消费输出”时，`diagweave` 很合适。

如果你只需要非常轻量的 `Display` 派生或一次性应用层传播，可能有更轻量的选择。

## 许可证

MIT 或 Apache-2.0 双许可证。

