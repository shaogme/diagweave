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
- `body: OtelValue|null`
- `timestamp_unix_nano: integer|null`
- `observed_timestamp_unix_nano: integer|null`
- `severity_text: string|null`
- `severity_number: OtelSeverityNumber|null` (`1|5|9|13|17|21` on the wire)
- `trace_id: 32-hex-string|null`
- `span_id: 16-hex-string|null`
- `trace_flags: integer|null`
- `attributes: Array<OtelAttribute>`

Record semantics:

- The primary `exception` record uses a plain string `body` value containing the error message, per OTel Semantic Conventions.
- The full structured error data (message + type) is preserved in `exception.raw_data` attribute for complete context.
- For the primary record, `severity_text` / `severity_number` are projected from `metadata.severity`.
- Trace-event records keep `body: null` and carry their data in top-level fields and attributes.
- For trace-event records, top-level severity comes from `trace.events[*].level`; when an event level is absent, the exporter falls back to the report `metadata.severity`.
- `to_otel_envelope()` is only available on `DiagnosticIr<'_, HasSeverity>`, so export always carries a report-level severity fallback and event severity fields are always populated.
- `parent_span_id` is emitted as the `trace.parent_span_id` attribute rather than a top-level event field.

## OtelAttribute model

- `key: string`
- `value: OtelValue`

## OtelValue model

`OtelValue` is serialized with Rust's externally tagged enum shape.

- `Null` as the string literal `"Null"`
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
- `trace.parent_span_id`
- `trace.state`
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
  - `nodes[*].type: string|null`
  - `nodes[*].source_roots: integer[]`
  - `truncated: boolean`
  - `cycle_detected: boolean`
- Empty trace, context, and attachment sections are omitted by default when they carry no data.

## Rust type definitions

When `feature = "otel"` is enabled, `diagweave` exports:

- `OtelEnvelope`
- `OtelEvent`
- `OtelAttribute`
- `OtelSeverityNumber`
- `OtelValue`
- `REPORT_OTEL_SCHEMA_VERSION`
- `REPORT_OTEL_SCHEMA_DRAFT`
- `report_otel_schema()`

When `feature = "json"` is also enabled, these types additionally derive `serde::Serialize` / `serde::Deserialize`.

See also the JSON report schema in [`docs/report-json-schema-v0.1.0.md`](docs/report-json-schema-v0.1.0.md).
