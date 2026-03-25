#!/usr/bin/env bash
set -euo pipefail

echo "[1/7] cargo test --workspace"
cargo test --workspace

echo "[2/7] cargo hack test --each-feature"
cargo hack test --each-feature

echo "[3/7] cargo hack test --no-default-features"
cargo hack test --no-default-features

echo "[4/7] cargo hack check --no-default-features --features json"
cargo hack check --no-default-features -p diagweave --features json

echo "[5/7] cargo check -p diagweave --no-default-features --features trace"
cargo check -p diagweave --no-default-features -p diagweave --features trace

echo "[6/7] cargo check -p diagweave --no-default-features --features tracing"
cargo check -p diagweave --no-default-features -p diagweave --features tracing

echo "[7/7] cargo hack test --feature-powerset"
cargo hack test --feature-powerset
