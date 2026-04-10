# 核心开发参考 (面向 AI)

## 1. `set!` 宏

### 概览
用于定义一系列结构化的错误枚举（Error Set），自动实现集合间的组合逻辑、`From` 转换、蛇形命名构造器、报告语义，以及枚举辅助方法（`to_report()`/`source()`/`diag()`）。

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
| `AuthError::to_report(self)` | `Report<AuthError>` | 将错误实例转换为报告 |
| `AuthError::source(&self)` | `Option<&dyn Error>` | 读取底层 source 错误 |
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
- **构造器**：为内联与外部变体生成蛇形命名构造器与 `*_report`。
- **选项**：支持在 union enum 上使用 `#[diagweave(constructor_prefix = \"...\", report_path = \"...\")]`。
- **辅助方法**：自动生成 `to_report()`、`source()` 与 `diag()`。

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
| `pub fn to_report(self)` | `Report<Self>` | 转换为基础报告对象 |
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
核心诊断容器，封装原始错误 `E` 并持有可选的“冷数据”（元数据、附件、展示原因链、追踪信息）以及按报告粒度生效的 `ReportOptions`。采用延迟分配策略，仅在添加辅助信息时才分配堆内存。
`category`、`trace_state`、trace 事件名和 stack trace 原始文本等高频字符串在捕获后会以共享 `StaticRefStr` 持有。
`map_err()` 是当前推荐的错误类型转换入口；其是否继续累积原生 `source` 链由 `ReportOptions` 控制。

### 声明定义

`Report` 结构体是一个高级诊断容器，对辅助数据采用延迟分配策略。所有四个字段都是**私有的**，无法从模块外部直接访问。

```text
pub struct Report<E, State: SeverityState = MissingSeverity> {
    inner: E, // 私有 - 被包装的错误值
    metadata: ReportMetadata<State>, // 私有 - 包含严重性的元数据
    report: ReportOptions, // 私有 - 按报告粒度的配置（内部采用延迟分配）
    cold: Option<Box<ColdData>>, // 私有 - 延迟分配的存储
}
```

**关键点：**
- `inner`：被包装的错误值（私有）
- `metadata`：包含严重性类型状态和可选的 error_code/category/retryable（私有）
- `report`：按报告粒度的配置，用于控制源链累积和原因收集行为（私有）
- `cold`：延迟分配的存储，用于诊断包（附件、展示原因、源错误）（私有）
- `ReportOptions` 内部采用延迟分配（`Option<Box<ReportOptionsInner>>`），仅在显式设置选项时才分配堆内存
- 字段访问通过方法提供，如 `inner()`、`severity()`、`options()` 等

### 核心构造与转换
| 方法声明 | 说明 |
| :--- | :--- |
| `Report::new(err: E)` | 创建报告 |
| `report.inner()` | 获取内部错误引用 |
| `report.into_inner()` | 消费报告并返回原始错误 |
| `report.attachments()` | 返回关联的所有附件列表 (`&[Attachment]`) |
| `report.metadata()` | 返回原始元数据引用 (`&ReportMetadata`) |
| `report.error_code()` | 读取元数据错误码 (`Option<&ErrorCode>`) |
| `report.severity()` | 从类型状态读取严重级别 (`Option<Severity>`) |
| `report.category()` | 读取元数据分类 (`Option<&str>`) |
| `report.retryable()` | 读取元数据重试标记 (`Option<bool>`) |
| `report.stack_trace()` | 获取关联的堆栈信息 (`Option<&StackTrace>`) |
| `report.trace()` | 获取关联的追踪信息 (`Option<&ReportTrace>`) |
| `report.visit_causes(visit)` | 使用默认选项流式遍历展示原因 |
| `report.visit_causes_ext(options, visit)` | 使用自定义选项流式遍历展示原因 |
| `report.visit_origin_sources(visit)` | 使用默认选项流式遍历原生传播链 |
| `report.visit_origin_src_ext(options, visit)` | 使用自定义选项流式遍历原生传播链 |
| `report.visit_diag_sources(visit)` | 使用默认选项流式遍历诊断补充链 |
| `report.visit_diag_srcs_ext(options, visit)` | 使用自定义选项流式遍历诊断补充链 |
| `report.iter_origin_sources()` | 使用默认选项迭代原生传播链 |
| `report.iter_origin_src_ext(options)` | 使用自定义选项迭代原生传播链 |
| `report.iter_diag_sources()` | 使用默认选项迭代诊断补充链 |
| `report.iter_diag_srcs_ext(options)` | 使用自定义选项迭代诊断补充链 |
| `report.options()` | 读取当前 `ReportOptions` 配置 |
| `report.set_options(options: ReportOptions)` | 替换当前报告的选项配置 |
| `report.set_accumulate_source_chain(accumulate: bool)` | 快速设置 `map_err()` 的原生 `source` 链累积行为 |
| `report.map_err(map: FnOnce(E) -> Outer)` | 映射内部错误类型并保留诊断信息；当启用 source 链累积时，会把旧的内层错误接到新错误的 `source` 链上 |

`ReportMetadata<State>` 现在包含严重性类型状态和可选元数据字段。严重性作为直接字段存储（不包装在 Option 或 Box 中），而 error_code/category/retryable 存储在内部的 `Option<Box<MetadataInner>>` 中以实现延迟分配。读取请使用 `error_code()`、`category()`、`retryable()`、`severity()` 等接口。写入式组合请使用 `with_error_code(...)`、`set_severity(...)` 等 builder 方法。

### `ReportOptions` 和 `GlobalConfig`

`ReportOptions` 用于控制单个 `Report` 的错误源链累积行为和原因收集行为。内部采用延迟分配策略（`Option<Box<ReportOptionsInner>>`），仅在显式设置选项时才分配堆内存。配置值通过以下优先级解析：

**配置优先级**：ReportOptions > GlobalConfig > Profile defaults

#### 配置文件相关的默认值

| 选项 | Debug 构建 | Release 构建 |
|------|------------|--------------|
| `accumulate_source_chain` | `true` | `false` |
| `detect_cycle` | `true` | `false` |
| `max_depth` | `16` | `16` |

#### 配置选项说明

- `accumulate_source_chain`：设置后，`map_err()` 会保留并延伸原生 `source` 链；未设置时从 `GlobalConfig` 或 profile 默认值继承。
- `max_depth`：原因收集时的最大深度限制。较高的值提供更完整的错误上下文，但对于非常深的错误链可能影响性能。未设置时从 `GlobalConfig` 或默认值 16 继承。
- `detect_cycle`：设置后，错误链遍历将跟踪已访问的错误并在检测到循环时标记。未设置时从 `GlobalConfig` 或 profile 默认值继承。

#### 构建方法

| 方法 | 说明 |
|------|------|
| `ReportOptions::new()` | 创建延迟分配的选项（在设置任何选项前不会分配堆内存） |
| `.with_accumulate_source_chain(bool)` | 设置源链累积行为（如需要会分配内部存储） |
| `.with_max_depth(usize)` | 设置原因收集深度限制 |
| `.with_cycle_detection(bool)` | 启用/禁用循环检测 |

#### 解析方法

| 方法 | 说明 |
|------|------|
| `.resolve_accumulate_source_chain()` | 解析实际使用的累积设置（优先级：ReportOptions > GlobalConfig > Profile default） |
| `.resolve_max_depth()` | 解析实际使用的深度限制 |
| `.resolve_detect_cycle()` | 解析实际使用的循环检测设置 |
| `.resolve_*_with_source()` | 解析值并返回来源追踪（返回 `ResolvedValue<T>`） |

#### 访问方法

| 方法 | 说明 |
|------|------|
| `.is_set()` | 返回 `true` 表示至少有一个选项被显式配置 |
| `.accumulate_source_chain()` | 返回 `Option<bool>` - 显式设置的值，若继承则返回 `None` |
| `.max_depth()` | 返回 `Option<usize>` - 显式设置的值，若继承则返回 `None` |
| `.detect_cycle()` | 返回 `Option<bool>` - 显式设置的值，若继承则返回 `None` |

#### `GlobalConfig` 全局配置

`GlobalConfig` 提供应用级别的配置默认值。当 `ReportOptions` 的字段未设置时，会从 `GlobalConfig` 继承值。所有字段都是私有的，通过方法访问。

| 方法 | 说明 |
|------|------|
| `GlobalConfig::new()` | 创建具有 profile 相关默认值的全局配置 |
| `.with_accumulate_source_chain(bool)` | 设置默认累积行为 |
| `.with_max_depth(usize)` | 设置默认深度限制 |
| `.with_cycle_detection(bool)` | 设置默认循环检测 |
| `.accumulate_source_chain()` | 返回配置的累积默认值 |
| `.max_depth()` | 返回配置的深度默认值 |
| `.detect_cycle()` | 返回配置的循环检测默认值 |
| `set_global_config(config)` | 设置全局配置（仅可调用一次） |

#### 使用示例

```rust
use diagweave::prelude::*;
use diagweave::report::{GlobalConfig, ReportOptions, set_global_config};

// 设置全局默认值（应用启动时调用一次）
let config = GlobalConfig::new()
    .with_accumulate_source_chain(true)
    .with_max_depth(32)
    .with_cycle_detection(true);
set_global_config(config).expect("全局配置已设置");

// 使用配置文件相关的默认值
let error = std::io::Error::new(std::io::ErrorKind::Other, "测试错误");
let report = Report::new(error);

// 为性能关键路径配置
let error2 = std::io::Error::new(std::io::ErrorKind::Other, "测试错误");
let report2 = Report::new(error2).set_options(
    ReportOptions::new()
        .with_max_depth(8)
        .with_cycle_detection(false)
);

// 启用完整诊断用于调试
let error3 = std::io::Error::new(std::io::ErrorKind::Other, "测试错误");
let report3 = Report::new(error3).set_options(
    ReportOptions::new()
        .with_accumulate_source_chain(true)
        .with_max_depth(32)
        .with_cycle_detection(true)
);
```

### `ErrorCode` 设计与转换规则
- 内部模型：
  - `ErrorCode::Integer(i64)`：紧凑数值错误码
  - `ErrorCode::String(StaticRefStr)`：符号型错误码或超范围数值错误码
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

`AttachmentValue::String` 也使用 `StaticRefStr` 作为内部存储，重复包装同一份 report 时可以减少字符串拷贝。附件 key、payload 名称/media type、全局上下文 key，以及 trace/category 元数据等持久化字符串也遵循同样规则。

### 原因链语义

- `with_display_cause` / `with_display_causes` 接收 `impl Display + Send + Sync + 'static`，并追加到展示原因字符串链（用于渲染与 IR）。
- `with_diag_src_err` 用于显式追加错误对象到**诊断补充链**，参数要求 `impl Error + Send + Sync + 'static`。
- 原生传播链由 `map_err()` 与 `Error::source()` 维护；是否把旧内层错误继续串接到新错误的 `source` 链，由 `ReportOptions` 决定。

### 全局注入 (Global Injection)
用于跨层级自动注入上下文（如 RequestID、SessionID）。
- **注册器**: `register_global_injector(f: fn() -> Option<GlobalContext>)`
- **全局配置**: `set_global_config(config: GlobalConfig)` - 设置应用级别的默认选项
- **注入时机**: 每次创建一个新的 `Report` 实例时自动执行。

| GlobalContext 字段 | 说明 |
| :--- | :--- |
| `context` | `ContextMap` 全局注入的业务上下文 |
| `system` | `ContextMap` 全局注入的系统/运行时上下文 |
| `error` | `Option<GlobalErrorMeta>` 元数据覆盖（`error_code` / `category` / `retryable` / `severity`） |
| `trace`（`trace` feature） | `Option<GlobalTraceContext>` 全局注入的 trace 上下文 |

**注意**: `GlobalConfig` 和 `set_global_config` 是独立的全局配置系统，用于设置 `ReportOptions` 的默认值；而 `register_global_injector` 用于注入上下文信息。两者可以配合使用。

`TraceId` / `SpanId` / `ParentSpanId` 为十六进制校验后的标识符。构造方式：
- `TraceId::new("32位hex")` / `SpanId::new("16位hex")` / `ParentSpanId::new("16位hex")`
- `unsafe { TraceId::new_unchecked(...) }` 跳过校验

### 链式配置方法

**API 命名约定**：
- `set_*` 方法总是替换已有值
- `with_*` 方法仅在未设置时才设置值（条件/保留语义）

| 方法 | 参数类型 | 说明 |
| :--- | :--- | :--- |
| `with_ctx` | `(impl Into<StaticRefStr>, impl Into<ContextValue>)` | 添加业务上下文键值对 |
| `set_ctx` | `(ContextMap)` | 替换业务上下文映射 |
| `with_system` | `(impl Into<StaticRefStr>, impl Into<ContextValue>)` | 添加系统上下文键值对 |
| `set_system` | `(ContextMap)` | 替换系统上下文映射 |
| `set_options` | `ReportOptions` | 替换当前报告的选项配置 |
| `set_accumulate_source_chain` | `bool` | 快速设置 `map_err()` 是否累积原生 `source` 链 |
| `attach_note` / `attach_printable` | `impl Display + Send + Sync + 'static` | 添加备注或解决建议 |
| `attach_payload` / `attach_payload` | `(impl Into<StaticRefStr>, Value, Option<impl Into<StaticRefStr>>)` | 附加命名负载 (支持媒体类型) |
| `set_severity` | `Severity` | 设置严重程度 (Debug, Info, Warn, Error, Fatal)，覆盖已有值 |
| `with_severity` | `Severity` | 设置严重程度，仅当未设置时生效 (保留底层诊断信息) |
| `set_error_code` | `impl Into<ErrorCode>` | 设置稳定的错误代码 (如 "E001")，覆盖已有值 |
| `with_error_code` | `impl Into<ErrorCode>` | 设置错误代码，仅当未设置时生效 (保留底层诊断信息) |
| `set_category` | `impl Into<StaticRefStr>` | 设置错误分类 (用于监控指标)，覆盖已有值 |
| `with_category` | `impl Into<StaticRefStr>` | 设置错误分类，仅当未设置时生效 (保留底层诊断信息) |
| `set_retryable` | `bool` | 标记该错误是否建议重试，覆盖已有值 |
| `with_retryable` | `bool` | 标记是否建议重试，仅当未设置时生效 (保留底层诊断信息) |
| `with_display_cause` | `impl Display + Send + Sync + 'static` | 添加单个展示原因字符串 |
| `with_display_causes` | `impl IntoIterator<Item = impl Display + Send + Sync + 'static>` | 批量添加展示原因字符串 |
| `with_diag_src_err` | `impl Error + Send + Sync + 'static` | 添加单个显式错误源对象 |
| `set_stack_trace` | `StackTrace` | 手动关联已存在的堆栈信息，覆盖已有值 |
| `with_stack_trace` | `StackTrace` | 手动关联已存在的堆栈信息，仅当未设置时生效 |
| `set_trace` | `ReportTrace` | 设置追踪信息，覆盖已有值 |
| `with_trace` | `ReportTrace` | 设置追踪信息，仅当未设置时生效 |
| `set_trace_ids` | `(TraceId, SpanId)` | 设置追踪和 Span ID，覆盖已有值 |
| `with_trace_ids` | `(TraceId, SpanId)` | 设置追踪和 Span ID，仅当未设置时生效 |
| `set_parent_span_id` | `ParentSpanId` | 设置父 Span ID，覆盖已有值 |
| `with_parent_span_id` | `ParentSpanId` | 设置父 Span ID，仅当未设置时生效 |
| `set_trace_sampled` | `bool` | 设置是否采样，覆盖已有值 |
| `with_trace_sampled` | `bool` | 设置是否采样，仅当未设置时生效 |
| `set_trace_state` | `impl Into<StaticRefStr>` | 设置 trace state 用于关联元数据，覆盖已有值 |
| `with_trace_state` | `impl Into<StaticRefStr>` | 设置 trace state，仅当未设置时生效 |
| `set_trace_flags` | `impl Into<TraceFlags>` | 设置追踪标志，覆盖已有值 |
| `with_trace_flags` | `impl Into<TraceFlags>` | 设置追踪标志，仅当未设置时生效 |
| `with_trace_event` | `TraceEvent` | 添加追踪事件到报告 |
| `push_trace_event` | `impl Into<StaticRefStr>` | 追加一个默认字段的 trace 事件 |
| `push_trace_event_with` | `(impl Into<StaticRefStr>, Option<TraceEventLevel>, Option<u64>, impl IntoIterator<Item = TraceEventAttribute>)` | 追加一个完整指定的 trace 事件 |
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

## 5. `Result` 扩展特质 (`Diagnostic` / `ResultReportExt` / `InspectReportExt`)

### 概览
通过为 `Result<T, E>` 和 `Result<T, Report<E>>` 实现扩展特质，提供在错误路径上无缝注入诊断信息的管道。

### 核心特质
#### 1. `Diagnostic` (作用于 `Result<T, E>`)
- `to_report()`: 提升 `Err(E)` 为 `Err(Report<E>)`。
- `to_report_note(msg)`: 提升并注入备注。
- `diag(...)`：Result<T, E> 上的快捷入口，泛型版本允许转换错误类型和状态类型；签名：
  `diag<E2, State2>(self, f: impl FnOnce(Report<E>) -> Report<E2, State2>) -> Result<T, Report<E2, State2>>`。
  闭包接收 `Report<E>` 并返回 `Report<E2, State2>`。当仅添加元数据时无需显式类型标注；
  当转换错误类型（如通过 `map_err`）时需要标注返回类型。若需要控制原生 source 链是否继续累积，可先通过 `set_accumulate_source_chain()` 配置报告选项。

#### 2. `ResultReportExt` (作用于 `Result<T, Report<E>>`)
不再重复每个 `Report` 方法，而是提供单一组合子：
- `and_then_report(|r| r.with_ctx(...).with_severity(...))` — 仅在错误路径上应用任意 `Report` 方法链

闭包接收 owned `Report` 并返回 owned `Report`。在 `Ok` 路径上闭包永远不会被调用，提供天然的延迟语义。

#### 3. `InspectReportExt` (作用于 `Result<T, Report<E>>`)
用于在错误路径做只读查询，避免手动 `match Err(report)`：
- `report_ref()`、`report_metadata()`、`report_attachments()`
- `report_error_code()`、`report_severity()`、`report_category()`、`report_retryable()`


### 用法示例
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

## 6. 展示原因收集

### 概览
负责管理诊断发生的诱因链。`diagweave` 的优势在于它不仅支持 `std::error::Error` 链，还支持跨线程/跨进程的事件消息。

### 展示原因数据
| 类型名 | 说明 |
| :--- | :--- |
| `DisplayCauseChain` | 运行时展示原因链摘要，包含 `items: Vec<Arc<dyn Display + Send + Sync + 'static>>`、`truncated` 与 `cycle_detected`。 |

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

Note 附件读取：
- `Attachment::as_note() -> Option<String>`：返回物化后的 note 文本。
- `Attachment::as_note_display() -> Option<&(dyn Display + Send + Sync + 'static)>`：返回零分配的显示引用。

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
| `show_trace_event_details` | `true` | 是否在 Pretty/JSON 中显示 trace 事件的 level、timestamp、attributes |
| `stack_trace_max_lines` | `24` | 原始堆栈渲染的最大行数截断 |
| `stack_trace_include_raw` | `true` | 渲染堆栈时是否包含原始堆栈输出 |
| `stack_trace_include_frames` | `true` | 渲染堆栈时是否包含解析后的帧信息 |
| `stack_trace_filter` | `All` | 堆栈帧过滤策略：`All`（全部）、`AppOnly`（过滤标准库帧）、`AppFocused`（额外过滤诊断内部帧） |

预设配置：
| 预设 | 说明 |
| :--- | :--- |
| `ReportRenderOptions::developer()` | 开发模式：完整 trace 事件详情，不过滤堆栈，最多 50 行 |
| `ReportRenderOptions::production()` | 生产排障模式：trace 事件详情，仅应用层帧，最多 15 行 |
| `ReportRenderOptions::minimal()` | 最小模式：隐藏 trace 事件详情，聚焦关键帧，最多 5 行，隐藏空段和类型名 |
| `stack_trace_filter` | `All` | 堆栈帧过滤策略：`All`（全部）、`AppOnly`（过滤标准库帧）、`AppFocused`（额外过滤诊断内部帧） |

预设配置：
| 预设 | 说明 |
| :--- | :--- |
| `ReportRenderOptions::developer()` | 开发模式：完整 trace 事件详情，不过滤堆栈，最多 50 行 |
| `ReportRenderOptions::production()` | 生产排障模式：trace 事件详情，仅应用层帧，最多 15 行 |
| `ReportRenderOptions::minimal()` | 最小模式：隐藏 trace 事件详情，聚焦关键帧，最多 5 行，隐藏空段和类型名 |


### 诊断中间表示 (`DiagnosticIr`)
渲染器不直接处理 `Report`，而是先通过 `to_diagnostic_ir()` 转换为稳定的 IR 结构。该 IR 保留错误节点、元数据、trace 引用、附件、展示原因、原生传播链、诊断补充链，以及附件相关部分的聚合计数。
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

`DiagnosticIrMetadata` 现在将内部字段完全私有化，并通过 `error_code()`、`severity()`、`category()`、`retryable()`、`stack_trace()` 等接口对外暴露只读访问。

逐项访问上下文/note/payload 由 `Report::visit_attachments(...)` 提供。

这样使用：
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

`DiagnosticIr` 会保留 `display_causes` 以及两条 source 链作为结构化数据。在 JSON 契约中，`origin_source_errors.type` 与 `diagnostic_source_errors.type` 都是 `string | null`；其中 `origin` 更常见 `null`，因为自然 `Error::source()` 会有信息损耗。
IR 与适配器层采用借用优先策略：错误/type/trace 等字符串投影尽量使用 `RefStr<'a>`，因此 `to_tracing_fields()` 和 `to_otel_envelope()` 在热点路径上会减少不必要的 `String` 物化。OTEL 导出被有意限制在 `DiagnosticIr<'a, HasSeverity>` 上。

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
- **`trace` 特性**：提供数据模型、`PreparedTracingEmission` 以及供自定义导出器使用的 `TracingExporterTrait`。
- **`tracing` 特性**：提供针对 `tracing` crate 的默认实现，以及 `prepare_tracing` / `emit_tracing` 辅助方法。

### 核心 API
| 方法 | 说明 |
| :--- | :--- |
| `prepare_tracing(&self)` | 仅在 `Report<_, HasSeverity>` / `DiagnosticIr<_, HasSeverity>` 上可用；会解析最终 report/event level 并返回可直接发射的 typestate 对象 |
| `emit_tracing(&self)` | `prepare_tracing().emit()` 的便捷封装 |
| `with_trace_ids(tid, sid)` | 手动绑定追踪上下文 (Trace ID / Span ID)，参数为 `TraceId` / `SpanId` |

### 导出行为
- **属性映射**：`Context` 会被映射为 `tracing` 事件的命名字段。
- **结构化字段**：`report_display_causes`、`report_origin_source_errors / report_diagnostic_source_errors`、`report_stack_trace`、`report_context` 和 `report_attachments` 会作为结构化调试字段导出。
- **空部分**：空的 `trace`、`context`、`attachments` 部分默认会省略。
- **Trace ID 绑定**：若 Report 包含 `TraceContext`，导出时会自动关联，或通过注入器自动关联当前 Span 环境信息。

### 用法示例
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

// 绑定 trace/span ids
#[cfg(feature = "trace")]
let report = report.with_trace_ids(
    TraceId::new("4bf92f3577b34da6a3ce929d0e0e4736").unwrap(),
    SpanId::new("00f067aa0ba902b7").unwrap(),
);

// 使用默认选项导出到当前 tracing span
#[cfg(feature = "tracing")]
report.prepare_tracing().emit();

// 使用自定义导出器
#[cfg(feature = "trace")]
report
    .prepare_tracing()
    .emit_with(&MyCustomExporter);
```

---

## 9. 云原生适配 (OpenTelemetry)

### 概览
`diagweave` 提供与 OpenTelemetry (OTel) 规范深度集成的适配器，支持将丰富的诊断数据转换为符合 OTLP 日志数据模型的记录批次。这里需要显式启用 `otel` feature。

### 转换 API
| 方法声明 | 返回类型 | 说明 |
| :--- | :--- | :--- |
| `ir.to_otel_envelope()` | `OtelEnvelope<'a>` | 仅在 `DiagnosticIr<'a, HasSeverity>` 上可用；转换为 OTLP 风格的日志/事件记录批次 |
| `ir.to_tracing_fields()` | `Vec<TracingField<'a>>`| 转换为 KV 形式的 Tracing/Logging 字段 |

### OTel 映射逻辑
1. **记录字段**: 主报告会变成一个日志记录，严重程度、时间戳相关元数据、trace 关联字段和结构化 `body` 错误节点会放在顶层。
2. **属性**: 错误核心字段、重试/分类标记、原因链摘要以及附件/上下文数据会以结构化 OTEL 属性输出。
3. **Trace 事件**: `Report` 内部的 `TraceEvent` 会转换成额外的 OTLP 风格日志/事件记录，带各自的时间戳、严重程度和 trace 关联字段。
4. **结构保留**: `exception.stacktrace` 和 `diagnostic_bag.origin_source_errors / diagnostic_bag.diagnostic_source_errors` 不再被字符串扁平化。

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
let _report = Report::new(MyError).attach_payload(
    "request_meta",
    json!({ "version": "v1", "retry": 3 }),
    Some("application/json")
);
```

### 2. 多层包装与错误链透传 (Wrap)
在架构各层之间传递时保留完整的 `source` 错误链。`map_err()` 默认会在 debug 构建中累积原生 source 链，在 release 构建中默认关闭；可以通过 `set_accumulate_source_chain(true/false)` 显式控制。
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

### 3. 自定义渲染器实现
通过实现 `ReportRenderer` 特质来自定义输出格式（如输出到 HTML 或 Web UI）。
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
| `trace` | 否 | Trace 数据模型 (`ReportTrace` 等)、预校验后的发射 typestate (`PreparedTracingEmission`) 与可插拔导出器 Trait (`TracingExporterTrait`) |
| `otel` | 否 | OTLP envelope 模型 (`OtelEnvelope`、`OtelEvent`、`OtelValue`) 与仅在 `DiagnosticIr<'_, HasSeverity>` 上提供的 `to_otel_envelope()` |
| `tracing` | 否 | 默认 `tracing` 生态集成 (`TracingExporter`、`prepare_tracing`、`emit_tracing`)。会自动开启 `trace`。 |

### 依赖矩阵
- **`no_std`**: 通过关闭默认特性支持。需要 `alloc`。
- **`json`**: 需要 `serde` (含 `derive` 和 `alloc` 特性) 以及 `serde_json` (含 `alloc` 特性)。
- **`trace`**: 无额外外部依赖的 Trace 数据结构。
- **`otel`**: 本身不引入额外依赖，但需要显式开启后才能导出 OTLP envelope。
- **`tracing`**: 依赖 `tracing` crate。





