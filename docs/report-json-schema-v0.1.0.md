# Report JSON Schema v0.1.0

This document defines the machine-consumable JSON contract emitted by `diagweave` when using the `Json` renderer.

- Schema version: `v0.1.0`
- Draft: JSON Schema 2020-12
- Canonical schema file: `diagweave/schemas/report-v0.1.0.schema.json`

## Stable payload fields

- `schema_version: string` (const: `v0.1.0`)
- `error: { message: string, type: string }`
- `metadata: { error_code: string|integer|null, severity: "debug"|"info"|"warn"|"error"|"fatal"|null, category: string|null, retryable: boolean|null }`
- `diagnostic_bag: { stack_trace: StackTrace|null, display_causes: DisplayCauseChain|null, source_errors: SourceErrorChain|null }`
- `trace: { context: TraceContext, events: TraceEvent[] }`
- `context: Array<{ key: string, value: AttachmentValue }>`
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

- `diagnostic_bag.source_errors.items[*].message: string`
- `diagnostic_bag.source_errors.truncated: boolean`
- `diagnostic_bag.source_errors.cycle_detected: boolean`

## Trace model

- `trace.context.trace_id: string|null`
- `trace.context.span_id: string|null`
- `trace.context.parent_span_id: string|null`
- `trace.context.sampled: boolean|null`
- `trace.context.trace_state: string|null`
- `trace.context.flags: integer|null`
- `trace.events[*].name: string`
- `trace.events[*].level: "trace"|"debug"|"info"|"warn"|"error"|null`
- `trace.events[*].timestamp_unix_nano: integer|null`
- `trace.events[*].attributes: Array<{ key: string, value: AttachmentValue }>`

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

## Rust type definitions

When `feature = "json"` is enabled, `diagweave` exports:

- `ReportJsonDocument`
- `ReportJsonError`
- `ReportJsonMetadata`
- `ReportJsonStackTrace`
- `ReportJsonStackFrame`
- `ReportJsonStackTraceFormat`
- `ReportJsonDisplayCauseChain`
- `ReportJsonSourceError`
- `ReportJsonSourceErrorChain`
- `ReportJsonContext`
- `ReportJsonAttachment`
- `REPORT_JSON_SCHEMA_VERSION`
- `REPORT_JSON_SCHEMA_DRAFT`
- `report_json_schema()`

These can be used for strict cross-service validation and compatibility checks.
