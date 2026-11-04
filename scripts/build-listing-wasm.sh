#!/bin/bash
# Builds the browser demo's listing engine (`crates/cmdr-listing-wasm`) into a `.wasm` plus its
# ES-module JS glue, by default into `crates/cmdr-listing-wasm/pkg/` (gitignored). What it does,
# step by step: `scripts/build-listing-wasm/main.go`. The crate and its JS API:
# `crates/cmdr-listing-wasm/DETAILS.md`.
#
# Usage: ./scripts/build-listing-wasm.sh [-out DIR] [-smoke]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}/scripts/build-listing-wasm"
exec go run . -root "${ROOT}" "$@"
