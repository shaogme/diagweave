# 核心开发参考 (面向 AI)

## 1. `set!` 宏

### 概览
用于定义一系列结构化的错误枚举（Error Set），自动实现集合间的组合逻辑、`From` 转换、蛇形命名构造器及报告语义。

### 语法定义
```text
set! {
    [#[diagweave(Meta)]]
    Ident = { [VariantDecls] } [ | OtherSet ]
    ...
}
```

### 声明参数 (Meta)
| 参数 | 类型 | 默认值 | 说明 |
| :--- | :--- | :--- | :--- |
| `report_path` | `String` | `"::diagweave::report::Report"` | 指定 `*_report` 构造器返回的报告类型路径 |
| `constructor_prefix` | `String` | `""` | 给生成的构造器函数名添加前缀（如 `new_`） |

### 支持属性 (Attributes)
| 属性 | 位置 | 参数 | 说明 |
| :--- | :--- | :--- | :--- |
| `#[display("...")]` | 变体 | 格式化字符串 | 使用 `{field}` 或 `{0}` 引用命名字段或匿名元组字段 |
| `#[display(transparent)]` | 变体 | 无 | 直接将内部字段的 `Display` 委托给该变体 (需恰好 1 个字段) |
| `#[from]` | 变体 | 无 | 标记该变体可从其单字段类型直接转换 (需恰好 1 个字段) |

### 核心用法
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

### 生成方法 (以 `AuthError` 为例)
| 声明 | 返回类型 | 说明 |
| :--- | :--- | :--- |
| `AuthError::user_not_found(id: u64)` | `AuthError` | 蛇形命名构造器 |
| `AuthError::user_not_found_report(id: u64)` | `Report<AuthError>` | 返回包含当前错误的报告对象 |
| `AuthError::diag(self)` | `Report<AuthError>` | 将错误实例转换为报告 |
| `From<AuthError> for ServiceError` | `ServiceError` | 自动实现子集到超集的映射 |

---

## 2. `union!` 宏

### 概览
用于在架构边界组合多个不相关的错误类型、其他错误集合或内联定义的变体。

### 语法定义
```text
union! {
    [Attributes]
    [vis] enum Ident = Item1 | Item2 | ...
}
```

### 声明项 (UnionItem)
| 项类型 | 语法 | 说明 |
| :--- | :--- | :--- |
| 外部类型 | `Path` | 自动实现 `From<Path>` 并委托 `Display` |
| 外部类型别名 | `Path as Ident` | 将 Path 的内容包装在名为 Ident 的变体中 |
| 内联变体 | `{ VariantDecls }` | 直接在 union 中定义本地变体，支持 `#[display]` |

### 核心用法
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
        AuthError |                     // 自动使用 AuthError 作为变体名
        std::io::Error as Io |          // 显式起名为 Io
        {                               // 内联定义
            #[display("fatal system failure")]
            Fatal
        }
}
```

### 特性描述
- **自动实现 `Display`**：对于外部类型，生成 `match` 分支调用 `inner.fmt(f)`；对于内联变体，基于 `#[display]` 模板生成渲染逻辑。
- **自动实现 `Error`**：如果未提供 `Debug`，会自动附加 `#[derive(Debug)]`。
- **From 注入**：为每一个外部成员类型注入 `impl From<T> for Union`。

---

## 3. `#[derive(Error)]` 派生宏

### 概览
为已有的独立 `struct` 或 `enum` 类型提供 `Display` 和 `std::error::Error` 特质的便捷实现，并桥接到 `diagweave` 诊断体系。

### 支持属性 (Attributes)
| 属性 | 位置 | 参数 | 说明 |
| :--- | :--- | :--- | :--- |
| `#[display]` | 变体/结构体 | `"template"` / `transparent` | 同 `set!` 中的渲染逻辑 |
| `#[from]` | 字段 | 无 | 自动实现 `From<FieldType>`，生成的实现会构造包含该字段的 Self |
| `#[source]` | 字段 | 无 | 标记该字段为 `Error::source()` 的返回值 |

### 生成成员方法
任何派生了 `Error` 的类型会自动获得以下辅助方法：
| 方法声明 | 返回类型 | 说明 |
| :--- | :--- | :--- |
| `pub fn diag(self)` | `Report<Self>` | 转换为基础报告对象 |
| `pub fn source(&self)` | `Option<&dyn Error>` | 便捷访问底层 Error 源 |

### 示例用法
```rust
#[derive(diagweave::Error, Debug)]
#[display("system failure")] // Struct 级别的 display 模板
struct GlobalError {
    #[source] // 手动指定 source
    inner: std::io::Error,
    
    msg: String,
}

#[derive(diagweave::Error, Debug)]
enum FileError {
    #[display("read error: {0}")]
    Read(#[from] std::io::Error), // 自动实现 From 并作为 source
}
```

---

## 4. `Report<E>` 诊断报告

### 概览
核心诊断容器，封装原始错误 `E` 并持有可选的“冷数据”（元数据、附件、展示原因链、追踪信息）。采用延迟分配策略，仅在添加辅助信息时才分配堆内存。

### 声明定义
```rust
struct ColdData;

pub struct Report<E> {
    inner: E,
    cold: Option<Box<ColdData>>,
}
```

### 核心构造与转换
| 方法声明 | 说明 |
| :--- | :--- |
| `Report::new(err: E)` | 创建报告 |
| `report.inner()` | 获取内部错误引用 |
| `report.into_inner()` | 消费报告并返回原始错误 |
| `report.attachments()` | 返回关联的所有附件列表 (`&[Attachment]`) |
| `report.metadata()` | 返回原始元数据引用 (`&ReportMetadata`) |
| `report.error_code()` | 读取元数据错误码 (`Option<&ErrorCode>`) |
| `report.severity()` | 读取元数据严重级别 (`Option<Severity>`) |
| `report.category()` | 读取元数据分类 (`Option<&str>`) |
| `report.retryable()` | 读取元数据重试标记 (`Option<bool>`) |
| `report.stack_trace()` | 获取关联的堆栈信息 (`Option<&StackTrace>`) |
| `report.trace()` | 获取关联的追踪信息 (`Option<&ReportTrace>`) |
| `report.visit_display_causes(visit)` | 使用默认选项流式遍历展示原因 |
| `report.visit_display_causes_with(options, visit)` | 使用自定义选项流式遍历展示原因 |
| `report.visit_source_errors(visit)` | 使用默认选项流式遍历错误源链 |
| `report.visit_source_errors_with(options, visit)` | 使用自定义选项流式遍历错误源链 |
| `report.wrap(outer: Outer)` | 将当前报告包装进另一个错误，并接入错误 `source` 链 |
| `report.wrap_with(map: FnOnce(E) -> Outer)` | 映射内部错误并保留所有诊断信息 |

### `ErrorCode` 设计与转换规则
- 内部模型：
  - `ErrorCode::Integer(i64)`：紧凑数值错误码
  - `ErrorCode::String(Cow<'static, str>)`：符号型错误码或超范围数值错误码
- 输入转换（`impl Into<ErrorCode>`）：
  - 整型输入（`i8..i128`、`u8..u128`、`isize`、`usize`）先尝试 `TryInto<i64>`
  - 成功则存为 `Integer`
  - 溢出则存为 `String(v.to_string())`
- 输出转换：
  - 支持 `TryFrom<ErrorCode>` / `TryFrom<&ErrorCode>` 到整型（`i8..i128`、`u8..u128`、`isize`、`usize`）
  - 支持 `From<ErrorCode> for String` 与 `From<&ErrorCode> for String`
  - 支持 `Display` / `to_string()` 输出标准文本形态
- 整型提取错误：
  - `ErrorCodeIntError::InvalidIntegerString`
  - `ErrorCodeIntError::OutOfRange`

### 全局注入 (Global Injection)
用于跨层级自动注入上下文（如 RequestID、SessionID）。
- **注册器**: `register_global_injector(f: fn() -> Option<GlobalContext>)`
- **注入时机**: 每次创建一个新的 `Report` 实例时自动执行。

| GlobalContext 字段 | 说明 |
| :--- | :--- |
| `context` | `Vec<(Cow<'static, str>, AttachmentValue)>` 全局关联的键值对 |
| `trace_id` | `Option<Cow<'static, str>>` 自动绑定的 Trace ID |
| `span_id` | `Option<Cow<'static, str>>` 自动绑定的 Span ID |

### 链式配置方法
| 方法 | 参数类型 | 说明 |
| :--- | :--- | :--- |
| `with_context` / `attach` | `(Ident, impl Into<AttachmentValue>)` | 添加上下文键值对 |
| `with_note` / `attach_printable` | `impl Display` | 添加备注或解决建议 |
| `with_payload` / `attach_payload` | `(Ident, Value, Option<Cow<'static, str>>)` | 附加命名负载 (支持媒体类型) |
| `with_severity` | `Severity` | 设置严重程度 (Debug, Info, Warn, Error, Fatal) |
| `with_error_code` | `impl Into<ErrorCode>` | 设置稳定的错误代码 (如 "E001") |
| `with_category` | `impl Into<Cow<'static, str>>` | 设置错误分类 (用于监控指标) |
| `with_retryable` | `bool` | 标记该错误是否建议重试 |
| `with_display_cause` | `impl Display` | 添加单个展示原因字符串 |
| `with_display_causes` | `impl IntoIterator<Item = impl Display>` | 批量添加展示原因字符串 |
| `with_source_error` | `impl Error + 'static` | 添加单个显式错误源对象 |
| `with_stack_trace` | `StackTrace` | 手动关联已存在的堆栈信息 |
| `capture_stack_trace` | 无 | (std) 捕获当前堆栈 (若已存在则跳过) |
| `force_capture_stack` | 无 | (std) 强制重新捕获堆栈 |
| `clear_stack_trace` | 无 | 移除已关联的堆栈信息 |

### 快捷渲染入口
| 方法 | 返回类型 | 说明 |
| :--- | :--- | :--- |
| `compact()` | `impl Display` | 仅输出原始错误消息 |
| `pretty()` | `impl Display` | 输出人类友好的分段详细诊断 (默认配置) |
| `json()` | `impl Display` | 输出符合 Schema 的 JSON 字符串 |
| `render(R)` | `impl Display` | 使用指定的渲染器渲染 |

### 用法示例
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

## 5. `Result` 扩展特质 (`Diagnostic` / `ReportResultExt` / `ReportResultInspectExt`)

### 概览
通过为 `Result<T, E>` 和 `Result<T, Report<E>>` 实现扩展特质，提供在错误路径上无缝注入诊断信息的管道。

### 核心特质
#### 1. `Diagnostic` (作用于 `Result<T, E>`)
- `diag()`: 提升 `Err(E)` 为 `Err(Report<E>)`。
- `diag_context(k, v)`: 提升并注入上下文。
- `diag_note(msg)`: 提升并注入备注。

#### 2. `ReportResultExt` (作用于 `Result<T, Report<E>>`)
所有 `Report` 的链式配置方法均有对应的代理版本：
- **元数据**: `with_severity`, `with_error_code`, `with_category`, `with_retryable`
- **附件**: `attach`/`with_context`, `attach_printable`/`with_note`, `attach_payload`/`with_payload`
- **延迟加载**: `context_lazy(key, f)`, `note_lazy(f)` (仅在 Err 时执行闭包)
- **展示原因**: `with_display_cause(c)`, `with_display_causes(cc)`
- **错误源**: `with_source_error(err)`
- **堆栈**: `capture_stack_trace()`, `clear_stack_trace()`, `with_stack_trace(st)`
- **包装**: `wrap(outer)`, `wrap_with(map)`

#### 3. `ReportResultInspectExt` (作用于 `Result<T, Report<E>>`)
用于在错误路径做只读查询，避免手动 `match Err(report)`：
- `report_ref()`、`report_metadata()`、`report_attachments()`
- `report_error_code()`、`report_severity()`、`report_category()`、`report_retryable()`


### 用法示例
```rust
use diagweave::prelude::*;
use std::{fs, io};
use std::time::SystemTime;

fn process() -> Result<(), Report<io::Error>> {
    fs::read_to_string("config.toml")
        .diag_context("file", "config.toml") // 转换并附加 context
        .with_severity(Severity::Warn)
        .context_lazy("timestamp", || format!("{:?}", SystemTime::now()).into())
        .attach_printable("failed to load system config")?;
        
    Ok(())
}
```

---

## 6. 展示原因收集

### 概览
负责管理诊断发生的诱因链。`diagweave` 的优势在于它不仅支持 `std::error::Error` 链，还支持跨线程/跨进程的事件消息。

### 展示原因数据
| 类型名 | 说明 |
| :--- | :--- |
| `Vec<Cow<'static, str>>` | 直接存储展示原因字符串，在渲染阶段转换为展示原因链元数据。 |

### 核心数据转换：`AttachmentValue`
`Report` 附件支持的强类型值，支持自动从基础类型转换：
| 类型 | Rust 实现类型 | 说明 |
| :--- | :--- | :--- |
| `Null` | `None` / `()` | 空值 |
| `String` | `&str`, `String` | UTF-8 文本 |
| `Integer` | `i8..i64` | 有符号整数 |
| `Unsigned` | `u8..u64` | 无符号整数 |
| `Float` | `f32`, `f64` | 浮点数 |
| `Bool` | `bool` | 布尔值 |
| `Array` | `Vec<AttachmentValue>` | 列表/序列 |
| `Object` | `BTreeMap<String, AttachmentValue>`| 键值对映射 |
| `Bytes` | `Vec<u8>` | 二进制数据内容 |
| `Redacted` | `{ kind, reason }` | 脱敏数据占位符 |

---

## 7. 渲染与输出 (Rendering)

### 概览
将包含丰富元数据的 `Report` 转换为可展示的字符串或结构化数据。

### 渲染配置 (`ReportRenderOptions`)
| 配置项 | 默认值 | 说明 |
| :--- | :--- | :--- |
| `show_type_name` | `true` | 是否显示错误的 Rust 类型全名 |
| `max_source_depth`| `16` | 递归收集 `source()` 的深度限制 |
| `detect_source_cycle`| `true` | 是否检测并终止循环原因链 |
| `pretty_indent` | `Spaces(2)`| `Pretty` 渲染的缩进风格 (支持 `Tab`) |
| `json_pretty` | `false` | JSON 输出是否带格式化缩进 |
| `show_empty_sections` | `true` | 是否展示没有内容的片段 (如 Trace 为空时) |
| `show_cause_chains_section` | `true` | 是否显示原因链 (Causes) 部分 |
| `show_context_section`| `true` | 是否显示上下文关联词部分 |
| `show_attachments_section`| `true` | 是否显示附件 (Payload/Note) 部分 |
| `show_stack_trace_section`| `true` | 是否显示堆栈轨迹部分 |
| `show_trace_section` | `true` | 是否显示分布式追踪 (TraceID/Event) 部分 |
| `stack_trace_max_lines` | `24` | 原始堆栈渲染的最大行数截断 |


### 诊断中间表示 (`DiagnosticIr`)
渲染器不直接处理 `Report`，而是先通过 `to_diagnostic_ir(options)` 转换为稳定的 IR 结构。该 IR 会把上下文、附件和原因链保留为延迟借用视图。
```rust
use diagweave::render::{
    DiagnosticIrAttachment, DiagnosticIrAttachments, DiagnosticIrContext,
    DiagnosticIrContexts, DiagnosticIrDisplayCauseChain, DiagnosticIrError,
    DiagnosticIrMetadata, DiagnosticIrSourceErrorChain,
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
    pub context: DiagnosticIrContexts<'a>,
    pub attachments: DiagnosticIrAttachments<'a>,
}
```

`DiagnosticIrContexts` 和 `DiagnosticIrAttachments` 是基于借用的可迭代视图，而不是已经分配好的 `Vec`。

这样使用：
```rust
use diagweave::render::{DiagnosticIrAttachment, ReportRenderOptions};

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

let context_count = ir.context.len();
for ctx in &ir.context {
    println!("context {} = {}", ctx.key, ctx.value);
}

let attachment_count = ir.attachments.len();
for attachment in &ir.attachments {
    match attachment {
        DiagnosticIrAttachment::Note { message } => println!("note: {message}"),
        DiagnosticIrAttachment::Payload { name, value, media_type } => {
            println!("payload {name} ({:?}): {value}", media_type);
        }
    }
}
```

`DiagnosticIrContext` 是上下文项的借用结构体，`DiagnosticIrAttachment` 是 note/payload 项的借用结构体。`display_causes` / `source_errors` 则通过 `items` 暴露延迟借用链。

### 用法示例
```rust
use diagweave::prelude::{Pretty, Report, ReportRenderOptions};
use diagweave::render::PrettyIndent;

let inner = std::io::Error::new(std::io::ErrorKind::Other, "oops");
let report = Report::new(inner);

// 1. 直接打印 Pretty 格式 (Stdout)
println!("{}", report.pretty());

// 2. 自定义 Pretty 布局
println!("{}", report.render(Pretty {
    options: ReportRenderOptions {
        pretty_indent: PrettyIndent::Tab,
        max_source_depth: 5,
        ..Default::default()
    }
}));

// 3. 生成 JSON
#[cfg(feature = "json")]
let json_str = report.json().to_string();
```

---

## 8. 日志系统集成 (`Tracing`)

### 概览
将诊断报告导出到监控系统或日志流。
- **`trace` 特性**：提供数据模型与 `TracingExporterTrait` 用于自定义导出器。
- **`tracing` 特性**：提供针对 `tracing` crate 的默认实现及 `emit_tracing` 快捷方法。

### 核心 API
| 方法 | 说明 |
| :--- | :--- |
| `emit_tracing(&self, options)` | 在当前 Span 下触发一个 `info` 级别的事件，携带所有 Report 字段作为属性 |
| `with_trace_ids(tid, sid)` | 手动绑定追踪上下文 (Trace ID / Span ID) |

### 导出行为
- **属性映射**：`Context` 会被映射为 `tracing` 事件的命名字段。
- **展示原因**：展示原因会被拼接为 `error.causes` 字符串。
- **Trace ID 绑定**：若 Report 包含 `TraceContext`，导出时会自动关联，或通过注入器自动关联当前 Span 环境信息。

### 用法示例
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

// 使用默认选项导出到当前 tracing span
#[cfg(feature = "tracing")]
report.emit_tracing(ReportRenderOptions::default());

// 使用自定义导出器
#[cfg(feature = "trace")]
report.emit_tracing_with(&MyCustomExporter, options);
```

---

## 9. 云原生适配 (OpenTelemetry)

### 概览
`diagweave` 提供与 OpenTelemetry (OTel) 规范深度集成的适配器，支持将丰富的诊断数据转换为标准的 Envelope 结构。

### 转换 API
| 方法声明 | 返回类型 | 说明 |
| :--- | :--- | :--- |
| `ir.to_otel_envelope()` | `OtelEnvelope` | 包含 attributes 和 events 的 OTel 载荷 |
| `ir.to_tracing_fields()` | `Vec<TracingField>`| 转换为 KV 形式的 Tracing/Logging 字段 |

### OTel 映射逻辑
1. **Attributes (属性)**: 错误核心字段（消息、代码、类型）、严重程度、重试标记、上下文 KV 全量映射。
2. **Events (事件)**: `Report` 中的 `Attachments` (Note/Payload) 和内部 `TraceEvent` 转换为 OTel 事件序列。
3. **TraceContext**: TraceID 和 SpanID 自动填充到 Envelope 顶层。

---

## 10. 高阶模式 (Advanced Patterns)

### 1. 复杂附件：结构化 JSON 关联
利用 `serde_json` 宏直接注入结构化数据。
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

### 2. 多层包装与错误链透传 (Wrap)
在架构各层之间传递时保留完整的 `source` 错误链。
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
        .wrap_with(AppError::Db)?; // 将 DatabaseError 包装为 AppError，同时保留 DB 层的 context
    Ok(())
}
```

### 3. 自定义渲染器实现
通过实现 `ReportRenderer` 特质来自定义输出格式（如输出到 HTML 或 Web UI）。
```rust
use diagweave::prelude::*;
use std::fmt::{self, Display, Formatter};

struct MyHtmlRenderer;
impl<E: Display + std::error::Error + 'static> ReportRenderer<E> for MyHtmlRenderer {
    fn render(&self, report: &Report<E>, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "<div>{}</div>", report.pretty())
    }
}
```

---

## 11. Feature Flags (特性开关)

| Feature | 默认开启 | 说明 |
| :--- | :--- | :--- |
| `std` | 是 | 标准库集成 (捕获堆栈、全局注入器等) |
| `json` | 否 | `Json` 渲染器支持 (依赖 `serde` 和 `serde_json`) |
| `trace` | 否 | Trace 数据模型 (`ReportTrace` 等) 与可插拔导出器 Trait (`TracingExporterTrait`、`emit_tracing_with`) |
| `tracing` | 否 | 默认 `tracing` 生态集成 (`TracingExporter`、`emit_tracing`)。会自动开启 `trace`。 |

### 依赖矩阵
- **`no_std`**: 通过关闭默认特性支持。需要 `alloc`。
- **`json`**: 需要 `serde` (含 `derive` 和 `alloc` 特性) 以及 `serde_json` (含 `alloc` 特性)。
- **`trace`**: 无额外外部依赖的 Trace 数据结构。
- **`tracing`**: 依赖 `tracing` crate。

