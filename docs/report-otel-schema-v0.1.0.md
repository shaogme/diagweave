# Report OTEL Schema v0.1.0

This document defines the machine-consumable OpenTelemetry envelope emitted by `diagweave` through `OtelEnvelope`.

- Schema version: `v0.1.0`
- Draft: JSON Schema 2020-12
- Canonical schema file: `diagweave/schemas/report-otel-v0.1.0.schema.json`
- Related JSON schema: [`docs/report-json-schema-v0.1.0.md`](docs/report-json-schema-v0.1.0.md)

## Stable payload fields

- `records: Array<OtelEvent>`

## OtelEvent model

- `name: string`
- `body: OtelValue` on the primary exception record; omitted on trace-event records
- `timestamp_unix_nano: integer` when present
- `observed_timestamp_unix_nano: integer` when present
- `severity_text: string` when present
- `severity_number: OtelSeverityNumber` when present (`1|5|9|13|17|21` on the wire)
- `trace_id: TraceId` when present (`32-hex-string`)
- `span_id: SpanId` when present (`16-hex-string`)
- `trace_flags: integer` when present
- `trace_context: { parent_span_id?: ParentSpanId, trace_state?: TraceState }` when present
- `attributes: Array<OtelAttribute>`

Record semantics:

- The primary `exception` record uses a plain string `body` value containing the error message, per OTel Semantic Conventions.
- The full structured error data (message + type) is preserved in `exception.raw_data` attribute for complete context.
- For the primary record, `severity_text` / `severity_number` are projected from `metadata.severity`.
- Trace-event records omit `body` and carry their data in top-level fields and attributes.
- For trace-event records, top-level severity comes from `trace.events[*].level`; when an event level is absent, the exporter falls back to the report `metadata.severity`.
- `to_otel_envelope()` is only available on `DiagnosticIr<'_, HasSeverity>`, so export always carries a report-level severity fallback and event severity fields are always populated.
- `trace_context` is a fixed top-level object on each `OtelEvent` record. It carries `parent_span_id` and `trace_state` when trace metadata is present.

## OtelAttribute model

- `key: string`
- `value: OtelValue`

## OtelValue model

`OtelValue` is serialized with Rust's externally tagged enum shape.

- `String` as `{ "String": string }`
- `Int` as `{ "Int": integer }`
- `U64` as `{ "U64": integer >= 0 }`
- `Double` as `{ "Double": number }`
- `Bool` as `{ "Bool": boolean }`
- `Bytes` as `{ "Bytes": byte[] }`
- `Array` as `{ "Array": OtelValue[] }`
- `KvList` as `{ "KvList": OtelAttribute[] }`

## Attribute Conventions

Current exporters populate these keys:

- `exception.type`
- `exception.message`
- `exception.raw_data` (structured error data with message and type)
- `exception.stacktrace`
- `error.code`
- `error.category`
- `error.retryable`
- `diagnostic_bag.display_causes`
- `diagnostic_bag.origin_source_errors`
- `diagnostic_bag.diagnostic_source_errors`
- `attachment.note`
- `attachment.payload.{name}`
- `attachment.payload.{name}.media_type`

Notes:

- `exception.stacktrace` is emitted as a structured `KvList` value, not a flattened string.
- `diagnostic_bag.origin_source_errors` and `diagnostic_bag.diagnostic_source_errors` use the same arena shape as JSON:
  - `roots: integer[]`
  - `nodes[*].message: string`
  - `nodes[*].type: string` when present
  - `nodes[*].source_roots: integer[]`
  - `truncated: boolean`
  - `cycle_detected: boolean`
- Empty or absent nested fields are omitted rather than encoded as `null`.
- Empty trace, context, and attachment sections are omitted by default when they carry no data.

## Rust type definitions

When `feature = "otel"` is enabled, `diagweave` exports:

- `OtelEnvelope`
- `OtelEvent`
- `OtelTraceContext`
- `OtelAttribute`
- `OtelSeverityNumber`
- `TraceId`
- `SpanId`
- `ParentSpanId`
- `TraceState`
- `OtelValue`
- `REPORT_OTEL_SCHEMA_VERSION`
- `REPORT_OTEL_SCHEMA_DRAFT`
- `report_otel_schema()`

When `feature = "json"` is also enabled, these types additionally derive `serde::Serialize` / `serde::Deserialize`.

See also the JSON report schema in [`docs/report-json-schema-v0.1.0.md`](docs/report-json-schema-v0.1.0.md).
