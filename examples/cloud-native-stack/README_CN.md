# cloud-native-stack 示例

`cloud-native-stack` 是一个面向云原生可观测链路的 `diagweave` 端到端演示。

它同时展示同一份失败信息的三种视图：

1. `diagweave` 的人类可读报告渲染
2. `diagweave` 的 `DiagnosticIr` 到 OpenTelemetry envelope 转换
3. 原生 OpenTelemetry Rust trace 和 log 导出

这个示例的目标不是把结构化信息压成一段 JSON 字符串，而是尽量把诊断结构保留到 OpenTelemetry 记录里。

## 数据流

示例的数据流如下：

```text
Report
  -> DiagnosticIr
  -> OtelEnvelope
  -> OpenTelemetry log records
```

同时，每个场景都会包裹在一个 `tracing` span 中，并通过 `tracing-opentelemetry` 接入 OpenTelemetry：

```text
tracing span
  -> tracing-opentelemetry
  -> OpenTelemetry trace export
```

因此，一次运行会产生：

1. `diagweave` 的 human / pretty / JSON 报告输出
2. 当前场景对应的 OpenTelemetry trace/span 输出
3. 由 `OtelEnvelope` 拆分出来的结构化 OTEL 日志记录

## Log 属性命名

现在示例统一使用 `diagweave.otel.*` 前缀来组织 log attributes：

1. `diagweave.otel.scenario.name`
2. `diagweave.otel.envelope.record.count`
3. `diagweave.otel.envelope.record.index`
4. `diagweave.otel.event.name`
5. `diagweave.otel.event.body`
6. `diagweave.otel.trace_context.parent_span_id`
7. `diagweave.otel.trace_context.trace_state`
8. `diagweave.otel.event.attr.<原始字段名>`

如果 `OtelEvent` 本身已经属于 `diagweave.otel.*` 命名空间，示例会保留原样；否则会统一放到 `diagweave.otel.event.attr.*` 下。

## 运行方式

### 离线模式

如果没有设置 OTEL collector 环境变量，示例会自动回退到 stdout exporter：

```bash
cargo run -p cloud-native-stack
```

### Collector 模式

只要提供任意 OTEL endpoint，示例就会切换到 OTLP Collector 输出。支持下面这些环境变量：

1. `OTEL_EXPORTER_OTLP_ENDPOINT`
2. `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT`
3. `OTEL_EXPORTER_OTLP_LOGS_ENDPOINT`

示例：

```powershell
$env:OTEL_EXPORTER_OTLP_ENDPOINT = "http://localhost:4318"
cargo run -p cloud-native-stack
```

也可以分开配置 traces 和 logs：

```powershell
$env:OTEL_EXPORTER_OTLP_TRACES_ENDPOINT = "http://localhost:4318"
$env:OTEL_EXPORTER_OTLP_LOGS_ENDPOINT = "http://localhost:4318"
cargo run -p cloud-native-stack
```

如果 Collector 初始化失败，程序会自动回退到 stdout，保证离线演示仍然可用。

## 本地 Collector

示例目录下已经补好本地 OpenTelemetry Collector 配置：

- [docker-compose.yaml](./docker-compose.yaml)
- [otel-collector-config.yaml](./otel-collector-config.yaml)

启动 Collector：

```bash
docker compose up -d
```

然后让示例连到 Collector：

```powershell
$env:OTEL_EXPORTER_OTLP_ENDPOINT = "http://localhost:4318"
$env:OTEL_EXPORTER_OTLP_TRACES_ENDPOINT = "http://localhost:4318"
$env:OTEL_EXPORTER_OTLP_LOGS_ENDPOINT = "http://localhost:4318"
cargo run -p cloud-native-stack
```

Collector 配置了 `4317` 和 `4318` 的 OTLP receiver，并使用 `debug` exporter 方便本地观察 trace 和 log。

## 这个示例展示了什么

OpenTelemetry envelope 不再被序列化成一整段字符串塞进 log body，而是被转换成原生的 OTEL log records。

1. 每个 `OtelEvent` 对应一条 OTEL log record
2. `OtelEvent.attributes` 会作为结构化 log attributes 输出
3. `OtelEvent.body` 会保留成结构化属性
4. `trace_id`、`span_id`、`trace_sampled` 会继续挂在当前 OpenTelemetry span 上

这样 Collector 中看到的是原生 OTEL 结构，而不是应用自定义的扁平 JSON。

## 输出内容

一次运行通常会看到：

1. `Compact (Human)`
2. `Pretty (Human)`
3. `JSON (ELK)`
4. `OTel Envelope`
5. stdout 或 Collector 输出的 OTEL logs / spans

## 说明

这个示例重点是观测结构和数据流，不是生产级 exporter 调优。

当前优先级是：

1. 离线始终可用
2. OTEL 数据保持结构化和可读
3. Collector 集成可选但易于开启

如果你还想继续扩展，下一步最自然的是补更完整的 resource attributes，以及更语义化的日志事件映射。
