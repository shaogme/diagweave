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
- `severity_number: 1|5|9|13|17|21|null`
- `trace_id: 32-hex-string|null`
- `span_id: 16-hex-string|null`
- `trace_flags: integer|null`
- `attributes: Array<OtelAttribute>`

Record semantics:

- The primary `exception` record uses a structured `body` value that mirrors the report error node rather than a plain message string.
- Trace-event records keep `body: null` and carry their data in top-level fields and attributes.

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
- `exception.stacktrace`
- `error.code`
- `error.category`
- `error.retryable`
- `trace.parent_span_id`
- `trace.state`
- `diagnostic_bag.display_causes`
- `diagnostic_bag.source_errors`
- `attachment.note`
- `attachment.payload.{name}`
- `attachment.payload.{name}.media_type`

Notes:

- `exception.stacktrace` is emitted as a structured `KvList` value, not a flattened string.
- `diagnostic_bag.source_errors` items preserve `message`, `type`, and nested `source` fields.
- Empty trace, context, and attachment sections are omitted by default when they carry no data.

## Rust type definitions

When `feature = "otel"` is enabled, `diagweave` exports:

- `OtelEnvelope`
- `OtelEvent`
- `OtelAttribute`
- `OtelValue`
- `REPORT_OTEL_SCHEMA_VERSION`
- `REPORT_OTEL_SCHEMA_DRAFT`
- `report_otel_schema()`

When `feature = "json"` is also enabled, these types additionally derive `serde::Serialize` / `serde::Deserialize`.

See also the JSON report schema in [`docs/report-json-schema-v0.1.0.md`](docs/report-json-schema-v0.1.0.md).
