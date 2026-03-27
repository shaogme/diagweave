$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# For native commands (cargo, rustup, etc.), fail fast on non-zero exit.
$PSNativeCommandUseErrorActionPreference = $true

function Run-Step {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Title,
        [Parameter(Mandatory = $true)]
        [scriptblock]$Command
    )

    Write-Host $Title
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "Step failed with exit code ${LASTEXITCODE}: $Title"
    }
}

Run-Step "[1/9] cargo test --workspace" {
    cargo test --workspace
}

Run-Step "[2/9] cargo hack test --each-feature" {
    cargo hack test --each-feature
}

Run-Step "[3/9] cargo hack test --no-default-features" {
    cargo hack test --no-default-features
}

Run-Step "[4/9] cargo hack check --no-default-features --features json" {
    cargo hack check --no-default-features -p diagweave --features json
}

Run-Step "[5/9] cargo hack check --no-default-features --features otel" {
    cargo hack check --no-default-features -p diagweave --features otel
}

Run-Step "[6/9] cargo check -p diagweave --no-default-features --features trace" {
    cargo check -p diagweave --no-default-features --features trace
}

Run-Step "[7/9] cargo check -p diagweave --no-default-features --features trace,otel" {
    cargo check -p diagweave --no-default-features --features trace,otel
}

Run-Step "[8/9] cargo check -p diagweave --no-default-features --features tracing" {
    cargo check -p diagweave --no-default-features --features tracing
}

Run-Step "[9/9] cargo hack test --feature-powerset" {
    cargo hack test --feature-powerset
}
