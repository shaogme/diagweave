# 分布式微服务治理（The Cloud-Native Stack）示例方案

本文档仅描述**设计与实施方案**，不包含任何实际代码改动。请在你确认方案后再开始实施。

---

## 目标与验证点

该示例用于验证 diagweave 在“跨服务边界 + 运行时诊断 + 观测导出”场景中的可用性。

核心验证点：

1. **`union!` 边界能力**：在网关层将所有下游服务错误统一 `union` 进一个 `ApiError`。
2. **自动 Trace 关联**：通过 `register_global_injector` 自动把当前 `RequestID` 与 `SpanID` 注入到每个 `Report` 中。
3. **OTel 导出**：验证在 `otel` feature 开启时，`ir.to_otel_envelope()` 生成的数据是否符合云端监控所需字段结构。

目标场景（模拟三层微服务）：

- Gateway（网关层）
- Order Service（订单服务）
- Payment Service（支付下游）

错误传播链：

- Payment Service 抛出 `NetworkError::Io`
- Order Service `wrap` 为 `OrderError::PaymentFailed`
- Gateway 捕获并用 `.json()` 输出结构化日志（为 ELK 设计）

---

## 方案概览

### 1. 新增示例 crate

在 `examples/` 下新增一个独立 crate（例如 `cloud-native-stack`）：

- `examples/cloud-native-stack/Cargo.toml`
- `examples/cloud-native-stack/src/main.rs`

将其加入 workspace：

- 修改根目录 `Cargo.toml` 的 `workspace.members` 增加 `examples/cloud-native-stack`

### 2. Error Set 设计

模拟三个模块各自错误集合，并在 Gateway 统一收敛。

- `PaymentError`
  - `NetworkError::Io(std::io::Error)`
- `OrderError`
  - `PaymentFailed(PaymentError)` or `wrap` with `OrderError::PaymentFailed`
- `GatewayError` / `ApiError`
  - 使用 `union!` 包含 `OrderError`、`PaymentError`、以及扩展错误（如 `AuthError`）

示意：

```
PaymentError  -> OrderError -> ApiError
```

要求：

- `union!` 在 Gateway 层完成所有下游错误的统一收敛
- 支持 `From<PaymentError>` / `From<OrderError>` 自动派生

### 3. Trace 与 Global Context 注入

在 `main` 或初始化函数中：

- 设置 `register_global_injector`：自动注入 `request_id` 和 `span_id`
- 所有 `Report` 自动携带上下文字段（无需显式 `with_context`）

验证：

- 在 JSON 输出中检查 `context` 部分包含 `request_id` 与 `span_id`

### 4. OTel Envelope 验证

- 在网关层：
  - `let ir = report.to_diagnostic_ir()`
  - `let otel = ir.to_otel_envelope()`

检查点：

- `attributes` 包含 trace/span 信息
- `events` 对应错误链/诊断事件

### 5. JSON 输出（结构化日志）

在网关层捕获最终 `Report<ApiError>` 后：

- 使用 `render(Json::new(...))` 生成 JSON
- 输出到 stdout（模拟 ELK ingestion）

检查点：

- `schema_version` 是否为 `v0.1.0`
- `diagnostic_bag` 是否包含 `display_causes` / `source_errors`

---

## 代码结构规划（草案）

### `examples/cloud-native-stack/src/main.rs`

模块划分：

- `mod payment`（Payment Service）
- `mod order`（Order Service）
- `mod gateway`（Gateway）

流程：

1. `payment::charge()` -> 返回 `Result<(), Report<PaymentError>>`
2. `order::create()` -> 捕获并 `wrap` 为 `OrderError::PaymentFailed`
3. `gateway::handle_request()` -> 捕获并 `wrap_with(ApiError::Order)`
4. `main()` -> 初始化 tracer/global injector，调用 gateway，并输出 JSON & OTel

---

## 验证方式

运行：

```
cargo run -p cloud-native-stack
```

期望输出：

- 打印最终 JSON（带 request_id/span_id）
- 打印 OTel envelope 的关键字段统计
- 打印结构化日志可见错误链：`NetworkError::Io -> OrderError::PaymentFailed -> ApiError::...`

---

## 依赖与 feature 规划

该示例需要：

- `diagweave`（workspace 版本）
- `json` feature
- `trace` / `tracing` / `otel` feature
- `serde_json`（如果要构造 payload）

示意：

```
[dependencies]
diagweave = { path = "../../diagweave", features = ["json", "trace", "otel", "tracing"] }
serde_json = "1"
```

---

## 风险与注意事项

- `register_global_injector` 仅在 `std` feature 下可用
- `to_otel_envelope()` 仅在 `otel` feature 下可用，`trace` 只负责补充 trace 事件
- 如果示例需要 `tracing` 输出，需要初始化 `tracing_subscriber`
- `union!` 与 `wrap_with` 的使用顺序需谨慎，确保错误链可追溯

---

## 下一步（等你确认）

1. 创建 `examples/cloud-native-stack` crate 与 `Cargo.toml`
2. 实现 `payment/order/gateway` 三层错误传播
3. 添加 JSON + OTel 导出逻辑
4. 运行并校验输出格式

---

如果你认可该方案，我将开始执行具体实现。
