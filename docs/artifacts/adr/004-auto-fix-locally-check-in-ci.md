# ADR 004: Auto-fix locally, check-only in CI

## Status

Accepted

## Context

Linters and formatters (rustfmt, clippy, prettier, eslint) can either:

1. Auto-fix issues in-place, or
2. Check only and report errors

We needed to decide how to balance developer productivity with CI safety.

## Decision

- **Locally** (no `--ci` flag): Auto-fix formatting and linting issues
- **In CI** (with `--ci` flag): Run in check-only mode and fail if fixes are needed

## Consequences

### Positive

- Developers get instant fixes locally, reducing friction
- CI ensures code is properly formatted before merge
- No "fix the formatting" ping-pong in reviews
- Clippy's `--allow-dirty --allow-staged` lets devs fix issues even with uncommitted changes

### Negative

- Developers must remember to run check script before committing
- Could surprise developers if formatters change their code

### Notes

- The behavior is controlled by the `--ci` flag in the Go check script
- CI runs are deterministic and fail fast if formatting/linting is needed
- Check script output explains what needs to be run locally to fix issues
