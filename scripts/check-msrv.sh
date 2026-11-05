#!/usr/bin/env bash
# Checks the crate against the declared MSRV (Rust 1.88, edition 2024).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOOLCHAIN="${MSRV_TOOLCHAIN:-1.88.0}"

if ! command -v rustup >/dev/null 2>&1; then
    echo "check-msrv: rustup is required to select Rust $TOOLCHAIN" >&2
    exit 1
fi

if ! rustup run "$TOOLCHAIN" rustc --version >/dev/null 2>&1; then
    echo "check-msrv: Rust $TOOLCHAIN is not installed" >&2
    echo "  rustup toolchain install $TOOLCHAIN --profile minimal" >&2
    exit 1
fi

cd "$ROOT"
echo "== checking with Rust $TOOLCHAIN (MSRV) =="
cargo "+$TOOLCHAIN" check --all-targets --locked
