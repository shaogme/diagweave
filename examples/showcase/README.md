# showcase

Runnable best-practice example for `diagweave`.

This crate is marked with `publish = false`.

What this example enables:
- `diagweave` with explicit features: `std`, `json`, `trace`, `tracing`
- `tracing` + `tracing-subscriber`
- typestate `report.prepare_tracing().emit()` path
- custom `TracingExporterTrait` path

Run:

```bash
cargo run -p showcase
```
