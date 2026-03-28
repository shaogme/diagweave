# diagweave 性能测试设计（Criterion 0.8.2）

本文定义 `diagweave` 的核心性能指标、基准分组和回归门禁建议，配套实现位于：

- `diagweave/benches/report_bench.rs`

## 1. 测试目标

聚焦运行时关键链路的稳定性与可回归性：

1. `Report` 构建与增量附加（context/note/payload/source）
2. `DiagnosticIr` 构建
3. 渲染开销（Compact / Pretty / JSON）
4. source 链遍历随深度增长的复杂度

## 2. 核心性能指标（KPI）

以 Criterion 默认统计输出为准，核心看：

1. `mean` / `median`（典型时延）
2. `p95`（尾延迟，关注抖动）
3. `slope`（规模增长趋势）
4. `change`（相对上一基线的回归比例）

建议门禁阈值：

1. 主链路（IR 构建、Compact 渲染）：回归不超过 `+8%`
2. 辅链路（Pretty/JSON）：回归不超过 `+12%`
3. 任意 case 的 `p95/median > 1.8` 视为抖动异常

## 3. 基准分组

### 3.1 `report_build`

目标：衡量 Report 构建与增量附加成本。

- `contexts/{0,4,16,64}`：纯 context 追加
- `mixed_attachments/{0,2,8,32}`：context + note + payload 复合追加

### 3.2 `report_transform_and_render`

目标：衡量转换与渲染成本。

输入规模：

- `small`：低负载报告
- `medium`：中负载报告
- `large`：高负载报告

子项：

- `to_diagnostic_ir/*`
- `render_compact/*`
- `render_pretty/*`
- `render_json_compact/*`（需 `json` feature）
- `render_json_pretty/*`（需 `json` feature）

### 3.3 `source_traversal`

目标：衡量 source 链遍历在不同 `max_depth` 下的开销。

- `iter_sources_depth/{1,4,16,64}`

## 4. 执行方式

在仓库根目录运行：

```bash
cargo bench -p diagweave --bench report_bench
```

启用 JSON 场景：

```bash
cargo bench -p diagweave --bench report_bench --features json
```

保存基线并对比：

```bash
cargo bench -p diagweave --bench report_bench -- --save-baseline main
cargo bench -p diagweave --bench report_bench -- --baseline main
```

## 5. CI 接入建议

1. `push` 仅跑 `small/medium` 样本，快速监控趋势
2. `nightly` 跑全量（含 `large` 和 `json`）
3. 回归超阈值时阻断并输出 Criterion change 报告

## 6. 扩展建议

1. 若需内存 KPI：补充 `dhat`/`heaptrack` 任务，关注渲染分配次数
2. 若需编译期 KPI（宏）：新增独立任务测 `cargo check`/`trybuild` 耗时，不与 Criterion 混用
