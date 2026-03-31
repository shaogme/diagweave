# Report JSON Schema v0.1.0

This document defines the machine-consumable JSON contract emitted by `diagweave` when using the `Json` renderer.

- Schema version: `v0.1.0`
- Draft: JSON Schema 2020-12
- Canonical schema file: `diagweave/schemas/report-v0.1.0.schema.json`
- Related OTEL schema: [`docs/report-otel-schema-v0.1.0.md`](docs/report-otel-schema-v0.1.0.md)

## Stable payload fields

- `schema_version: string` (const: `v0.1.0`)
- `error: { message: string, type: string }`
- `metadata: { error_code: string|integer|null, severity: "trace"|"debug"|"info"|"warn"|"error"|"fatal"|null, category: string|null, retryable: boolean|null }`
- `diagnostic_bag: { stack_trace: StackTrace|null, display_causes: DisplayCauseChain|null, origin_source_errors: SourceErrorChain|null, diagnostic_source_errors: SourceErrorChain|null }`
- `trace: { context: TraceContext, events: TraceEvent[] }`
- `context: object` (business context map; object keys are non-empty strings)
- `system: { service: object, deployment: object, runtime: object, request: object }` (strongly typed system context object)
- `attachments: Array<Note|Payload>`

## StackTrace model

- `diagnostic_bag.stack_trace.format: "native"|"raw"`
- `diagnostic_bag.stack_trace.frames[*].symbol: string|null`
- `diagnostic_bag.stack_trace.frames[*].module_path: string|null`
- `diagnostic_bag.stack_trace.frames[*].file: string|null`
- `diagnostic_bag.stack_trace.frames[*].line: integer|null`
- `diagnostic_bag.stack_trace.frames[*].column: integer|null`
- `diagnostic_bag.stack_trace.raw: string|null`

## DisplayCauseChain model

- `diagnostic_bag.display_causes.items[*]: string`
- `diagnostic_bag.display_causes.truncated: boolean`
- `diagnostic_bag.display_causes.cycle_detected: boolean`

## SourceErrorChain model

- `diagnostic_bag.origin_source_errors.roots[*]: integer` (node ids of top-level roots)
- `diagnostic_bag.origin_source_errors.nodes[*].message: string`
- `diagnostic_bag.origin_source_errors.nodes[*].type: string|null`
- `diagnostic_bag.origin_source_errors.nodes[*].source_roots[*]: integer` (node ids of children)
- `diagnostic_bag.origin_source_errors.truncated: boolean`
- `diagnostic_bag.origin_source_errors.cycle_detected: boolean`

- `diagnostic_bag.diagnostic_source_errors.roots[*]: integer` (node ids of top-level roots)
- `diagnostic_bag.diagnostic_source_errors.nodes[*].message: string`
- `diagnostic_bag.diagnostic_source_errors.nodes[*].type: string|null`
- `diagnostic_bag.diagnostic_source_errors.nodes[*].source_roots[*]: integer` (node ids of children)
- `diagnostic_bag.diagnostic_source_errors.truncated: boolean`
- `diagnostic_bag.diagnostic_source_errors.cycle_detected: boolean`

## Trace model

- `trace.context.trace_id: string|null` (`string` must match `^[0-9A-Fa-f]{32}$`)
- `trace.context.span_id: string|null` (`string` must match `^[0-9A-Fa-f]{16}$`)
- `trace.context.parent_span_id: string|null` (`string` must match `^[0-9A-Fa-f]{16}$`)
- `trace.context.sampled: boolean|null`
- `trace.context.trace_state: string|null`
- `trace.context.flags: integer|null` (range: `0..=255`)
- `trace.events[*].name: string`
- `trace.events[*].level: "trace"|"debug"|"info"|"warn"|"error"|null`
- `trace.events[*].timestamp_unix_nano: integer|null`
- `trace.events[*].attributes: Array<{ key: string, value: AttachmentValue }>`

### Example `system` payload

```json
{
  "system": {
    "service": {
      "name": { "kind": "string", "value": "cloud-native-stack" }
    },
    "deployment": {
      "environment": { "kind": "string", "value": "staging" }
    },
    "runtime": {
      "host.arch": { "kind": "string", "value": "x86_64" }
    },
    "request": {
      "request_id": { "kind": "string", "value": "req-20260327-0001" }
    }
  }
}
```

## Context model

- `context` is an object map from business context key to `AttachmentValue`.
- business context keys are non-empty strings

## System model

- `system` is a structured governance object with fixed sections:
  - `system.service`
  - `system.deployment`
  - `system.runtime`
  - `system.request`
- each system section is an object map from non-empty string key to `AttachmentValue`
- `system` does not allow additional top-level section names
- emitters should place governance/runtime/platform metadata into one of these four sections instead of flattening them into a single free-form map

## AttachmentValue

`AttachmentValue` is a tagged recursive union with these variants:

- `null`
- `string`
- `integer`
- `unsigned`
- `float`
- `bool`
- `array`
- `object`
- `bytes`
- `redacted`

## Rust JSON-facing APIs

When `feature = "json"` is enabled, the public JSON-related APIs include:

- `diagweave::render::Json` (renderer)
- `diagweave::render::REPORT_JSON_SCHEMA_VERSION`
- `diagweave::render::REPORT_JSON_SCHEMA_DRAFT`
- `diagweave::render::report_json_schema()`

For typed context modeling in report APIs:

- `diagweave::report::JsonContext`
- `diagweave::report::JsonContextEntry`

Use `report_json_schema()` for strict cross-service validation and compatibility checks.

See also the OpenTelemetry envelope schema in [`docs/report-otel-schema-v0.1.0.md`](docs/report-otel-schema-v0.1.0.md).
