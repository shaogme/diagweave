# Pretty/JSON 渲染器重复代码统一方案

## 1. 现状分析

`pretty.rs` 和 `json/report.rs` 之间存在以下重复代码：

### 1.1 完全相同的逻辑（可提取为共享函数）

| 重复项 | pretty.rs 行号 | json/report.rs 行号 | 相似度 |
|--------|---------------|---------------------|--------|
| `should_filter_frame` | 291-297 | 593-599 | 完全相同 |
| `is_std_or_runtime_frame` | 299-308 | 601-610 | 完全相同 |
| `is_internal_frame` | 310-314 | 612-616 | 完全相同 |
| `CauseCollectOptions` 构建模式 | 多处 | 多处 | 完全相同 |

### 1.2 相似逻辑、不同输出格式（可提取为共享迭代器/访问器）

| 重复项 | pretty.rs | json/report.rs | 差异 |
|--------|-----------|----------------|------|
| 堆栈帧迭代 + max_lines 截断 | 257-276 | 571-581 | Pretty 额外输出 "N more frames filtered" |
| Display causes 遍历 | 504-552 | 246-292 | 输出格式不同，遍历逻辑相同 |
| Source error chain 遍历 | 554-655 | 294-390 | Pretty 用 DFS 迭代，JSON 用 export 扁平结构 |
| Trace context 字段输出 | 182-205 | 393-426 | 6 个相同字段，格式不同 |
| Trace event 渲染 | 158-231 | 447-549 | 相同字段，相同 `show_trace_event_details` 守卫 |
| Governance 字段输出 | 105-156 | 209-244 | 4 个相同字段，格式不同 |

## 2. 设计目标

1. **消除完全相同的代码** — 帧过滤逻辑只保留一份
2. **共享迭代/访问模式** — 将"遍历什么"与"如何输出"解耦
3. **不破坏现有 API** — 所有公开接口保持不变
4. **不引入运行时开销** — 零成本抽象，使用 `inline` 和泛型
5. **保持代码可读性** — 提取后每个渲染器文件不超过 400 行

## 3. 方案设计

### 3.1 新建 `render/stack_filter.rs` 模块

将三个帧过滤函数提取到独立模块，供 Pretty 和 JSON 共享：

```rust
// render/stack_filter.rs
use crate::report::StackFrame;
use super::StackTraceFilter;

/// 判断是否应该过滤掉某个堆栈帧
#[inline]
pub fn should_filter_frame(frame: &StackFrame, filter: &StackTraceFilter) -> bool {
    match filter {
        StackTraceFilter::All => false,
        StackTraceFilter::AppOnly => is_std_or_runtime_frame(frame),
        StackTraceFilter::AppFocused => {
            is_std_or_runtime_frame(frame) || is_internal_frame(frame)
        }
    }
}

/// 判断是否为标准库或运行时帧
#[inline]
pub fn is_std_or_runtime_frame(frame: &StackFrame) -> bool {
    frame.module_path.as_ref().map_or(false, |m| {
        m.starts_with("std::")
            || m.starts_with("core::")
            || m.starts_with("alloc::")
            || m.starts_with("backtrace::")
            || m.contains("rust_begin_unwind")
            || m.contains("rust_panic")
    })
}

/// 判断是否为诊断库内部帧
#[inline]
pub fn is_internal_frame(frame: &StackFrame) -> bool {
    frame.module_path.as_ref().map_or(false, |m| {
        m.starts_with("diagweave::")
            || m.contains("diagnostic")
            || m.contains("report")
    })
}

/// 迭代过滤后的堆栈帧，返回 (帧, 原始索引)
pub fn filtered_frames<'a>(
    frames: &'a [StackFrame],
    filter: &StackTraceFilter,
) -> impl Iterator<Item = (usize, &'a StackFrame)> {
    frames.iter().enumerate().filter(move |(_, f)| {
        !should_filter_frame(f, filter)
    })
}

/// 计算过滤后应显示的帧数（受 max_lines 限制）
pub fn count_displayed_frames(
    frames: &[StackFrame],
    filter: &StackTraceFilter,
    max_lines: usize,
) -> (usize, usize) {
    let mut displayed = 0usize;
    let mut total_filtered = 0usize;
    for frame in frames {
        if should_filter_frame(frame, filter) {
            total_filtered += 1;
        } else if displayed < max_lines {
            displayed += 1;
        } else {
            total_filtered += 1;
        }
    }
    (displayed, total_filtered)
}
```

### 3.2 修改 `render.rs` 模块结构

```rust
#[path = "render/stack_filter.rs"]
mod stack_filter;
```

公开 `stack_filter` 模块供子模块使用：

```rust
pub(crate) use stack_filter::{
    should_filter_frame, is_std_or_runtime_frame, is_internal_frame,
    filtered_frames, count_displayed_frames,
};
```

### 3.3 重构 Pretty 和 JSON 中的帧迭代

**Pretty 侧**（`pretty.rs`）：删除三个本地函数，改为：

```rust
use super::stack_filter::{filtered_frames, should_filter_frame};

// 帧迭代部分改为：
for (idx, frame) in filtered_frames(&stack_trace.frames, &options.stack_trace_filter)
    .take(options.stack_trace_max_lines)
{
    // 输出帧信息...
}
```

**JSON 侧**（`json/report.rs`）：同样删除三个本地函数，改为：

```rust
use super::stack_filter::filtered_frames;

for (_, frame) in filtered_frames(&stack_trace.frames, &options.stack_trace_filter)
    .take(options.stack_trace_max_lines)
{
    write_stack_frame_object(f, pretty, depth + 2, frame)?;
}
```

### 3.4 共享 `CauseCollectOptions` 构建

当前两个渲染器中重复出现：

```rust
let traversal_options = CauseCollectOptions {
    max_depth: options.max_source_depth,
    detect_cycle: options.detect_source_cycle,
};
```

在 `render.rs` 中添加辅助方法：

```rust
impl ReportRenderOptions {
    #[inline]
    pub(crate) fn cause_collect_options(&self) -> crate::report::CauseCollectOptions {
        crate::report::CauseCollectOptions {
            max_depth: self.max_source_depth,
            detect_cycle: self.detect_source_cycle,
        }
    }
}
```

### 3.5 不提取的部分（理由）

以下部分**不建议提取**，因为差异大于共性：

| 部分 | 不提取理由 |
|------|-----------|
| Display causes 输出 | Pretty 输出编号列表，JSON 输出 `{items, truncated, cycle_detected}` 对象结构，输出模型完全不同 |
| Source error chain 遍历 | Pretty 用迭代 DFS 逐行输出，JSON 用 `export_with_options` 生成扁平结构再序列化，算法本质不同 |
| Trace event 输出 | 字段相同但 JSON 需要构造嵌套对象，Pretty 是逐行文本，共享价值低 |
| Governance 字段 | 仅 4 个字段，格式完全不同（JSON key-value vs Pretty 标签行） |

这些部分的共性在于"读取相同的 Report 字段"，但"如何格式化"差异远大于"读取什么"的共性。强制提取会产生过度抽象的回调地狱。

## 4. 实施步骤

### 阶段一：帧过滤提取（核心收益）

1. 创建 `render/stack_filter.rs`，移入三个过滤函数 + 两个辅助迭代器
2. 在 `render.rs` 中添加模块声明和 `pub(crate)` 导出
3. 修改 `pretty.rs`：删除本地三个函数，改为 `use super::stack_filter::*`
4. 修改 `json/report.rs`：删除本地三个函数，改为 `use super::stack_filter::*`
5. 运行 `cargo test --workspace --all-targets --all-features` 验证

### 阶段二：CauseCollectOptions 辅助方法

1. 在 `ReportRenderOptions` 上添加 `cause_collect_options()` 方法
2. 替换两个渲染器中的重复构建代码

### 阶段三（可选）：帧迭代统一

1. 使用 `filtered_frames().take(max_lines)` 替换两边的 for 循环
2. Pretty 侧保留 "N more frames filtered" 的额外逻辑

## 5. 预期收益

| 指标 | 当前 | 阶段一后 | 阶段二后 |
|------|------|---------|---------|
| `pretty.rs` 行数 | ~662 | ~645 | ~640 |
| `json/report.rs` 行数 | ~646 | ~615 | ~605 |
| 重复函数数 | 3 个完全相同 + 多处模式重复 | 0 个完全相同 | 0 个完全相同 |
| 新增文件 | 0 | 1 (`stack_filter.rs` ~50 行) | 0 |
| 测试覆盖率 | 不变 | 不变 | 不变 |

核心收益来自阶段一：消除三个完全相同的函数（~25 行重复代码），并通过 `filtered_frames` 迭代器消除帧循环逻辑的重复。

## 6. 风险评估

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| `stack_filter.rs` 中函数名冲突 | 编译错误 | 使用 `pub(crate)` 限制可见性 |
| 迭代器引入性能开销 | 理论上零成本 | `#[inline]` + 基准测试验证 |
| 重构引入回归 | 测试失败 | 每个阶段后运行全量测试 |
| 过度抽象降低可读性 | 维护困难 | 阶段三标记为可选，仅在收益明确时执行 |
