#!/usr/bin/env bash
set -euo pipefail

echo "[1/9] cargo test --workspace"
cargo test --workspace

echo "[2/9] cargo hack test --each-feature"
cargo hack test --each-feature

echo "[3/9] cargo hack test --no-default-features"
cargo hack test --no-default-features

echo "[4/9] cargo hack check --no-default-features --features json"
cargo hack check --no-default-features -p diagweave --features json

echo "[5/9] cargo hack check --no-default-features --features otel"
cargo hack check --no-default-features -p diagweave --features otel

echo "[6/9] cargo check -p diagweave --no-default-features --features trace"
cargo check -p diagweave --no-default-features --features trace

echo "[7/9] cargo check -p diagweave --no-default-features --features trace,otel"
cargo check -p diagweave --no-default-features --features trace,otel

echo "[8/9] cargo check -p diagweave --no-default-features --features tracing"
cargo check -p diagweave --no-default-features --features tracing

echo "[9/9] cargo hack test --feature-powerset"
cargo hack test --feature-powerset
