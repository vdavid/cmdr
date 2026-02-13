# ADR 001: Use Go for the check script instead of Bash

## Status

Accepted

## Context

We needed a unified check script for both Rust and Svelte checks that works cross-platform (macOS, Windows, Linux). The
script needs to:

- Run formatters, linters, and tests
- Support auto-fixing locally and check-only mode in CI
- Provide colored output and clear error messages
- Be maintainable and extensible

## Decision

Use Go instead of Bash to implement the check script in `scripts/check/`.

## Consequences

### Positive

- Better cross-platform support (especially Windows)
- Type-safe, easier to extend and maintain
- Better error handling and structured output
- Can build complex logic (parallel checks, colored output, etc.)

### Negative

- Requires Go in the toolchain (already needed, managed via mise)
- Slightly more verbose than Bash for simple tasks

### Notes

- Go is already managed via mise (.mise.toml)
- The script is built on-the-fly via `go run *.go` in scripts/check.sh wrapper
