# cloud-native-stack Example

`cloud-native-stack` is a compact end-to-end demonstration of `diagweave` in a cloud-native observability workflow.

It shows three complementary views of the same failure:

1. Human-readable `diagweave` report rendering
2. `diagweave` `DiagnosticIr` to OpenTelemetry envelope conversion
3. Native OpenTelemetry Rust trace and log export

The current goal is not to print a serialized JSON blob. The goal is to preserve the diagnostic structure all the way into OpenTelemetry records.

## Data Flow

The example follows this path:

```text
Report
  -> DiagnosticIr
  -> OtelEnvelope
  -> OpenTelemetry log records
```

At the same time, each scenario is wrapped in a `tracing` span and bridged into OpenTelemetry:

```text
tracing span
  -> tracing-opentelemetry
  -> OpenTelemetry trace export
```

This means a single run produces:

1. `diagweave` human / pretty / JSON report output
2. OpenTelemetry trace/span output for the current scenario
3. Structured OTEL log records derived from `OtelEnvelope`

## Log Attribute Naming

The example now uses a consistent `diagweave.otel.*` prefix strategy for log attributes:

1. `diagweave.otel.scenario.name`
2. `diagweave.otel.envelope.record.count`
3. `diagweave.otel.envelope.record.index`
4. `diagweave.otel.event.name`
5. `diagweave.otel.event.body`
6. `diagweave.otel.trace_context.parent_span_id`
7. `diagweave.otel.trace_context.trace_state`
8. `diagweave.otel.event.attr.<original-key>`

If an `OtelEvent` attribute already belongs to the `diagweave.otel.*` namespace, the example keeps it as-is. Otherwise, it is placed under `diagweave.otel.event.attr.*`.

## Running the Example

### Offline mode

If no OTEL collector environment variables are set, the example falls back to stdout exporters:

```bash
cargo run -p cloud-native-stack
```

### Collector mode

If any OTEL endpoint is provided, the example switches to OTLP Collector export. The following variables are supported:

1. `OTEL_EXPORTER_OTLP_ENDPOINT`
2. `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT`
3. `OTEL_EXPORTER_OTLP_LOGS_ENDPOINT`

Example:

```powershell
$env:OTEL_EXPORTER_OTLP_ENDPOINT = "http://localhost:4318"
cargo run -p cloud-native-stack
```

You can also set the trace and log endpoints separately:

```powershell
$env:OTEL_EXPORTER_OTLP_TRACES_ENDPOINT = "http://localhost:4318"
$env:OTEL_EXPORTER_OTLP_LOGS_ENDPOINT = "http://localhost:4318"
cargo run -p cloud-native-stack
```

If Collector initialization fails, the program automatically falls back to stdout so the demo remains usable offline.

## Local Collector Setup

The example includes a local OpenTelemetry Collector configuration:

- [docker-compose.yaml](./docker-compose.yaml)
- [otel-collector-config.yaml](./otel-collector-config.yaml)

Start the Collector with:

```bash
docker compose up -d
```

Then point the example at the Collector:

```powershell
$env:OTEL_EXPORTER_OTLP_ENDPOINT = "http://localhost:4318"
$env:OTEL_EXPORTER_OTLP_TRACES_ENDPOINT = "http://localhost:4318"
$env:OTEL_EXPORTER_OTLP_LOGS_ENDPOINT = "http://localhost:4318"
cargo run -p cloud-native-stack
```

The Collector is configured with an OTLP receiver on ports `4317` and `4318`, plus a `debug` exporter so you can inspect traces and logs locally.

## What the Example Demonstrates

The OpenTelemetry envelope is converted into native log records instead of being serialized into a single string body.

1. Each `OtelEvent` becomes one OTEL log record
2. `OtelEvent.attributes` are emitted as structured log attributes
3. `OtelEvent.body` is preserved as a structured attribute
4. `trace_id`, `span_id`, and `trace_flags` stay attached to the current OpenTelemetry span context

In practice, the Collector sees native OTEL records instead of an application-specific JSON string.

## Output Overview

A single run prints:

1. `Compact (Human)`
2. `Pretty (Human)`
3. `JSON (ELK)`
4. `OTel Envelope`
5. stdout or Collector output for OTEL logs and spans

## Notes

This example focuses on observability structure and data flow, not on production-grade exporter tuning.

The current priorities are:

1. Offline execution must always work
2. OTEL data should remain structured and readable
3. Collector integration should be optional but easy to enable

If you want to extend it further, the next natural step is to add richer resource attributes and more semantic event mapping for logs.
