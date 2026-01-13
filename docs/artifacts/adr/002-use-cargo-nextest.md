# ADR 002: Use cargo-nextest instead of cargo test

## Status

Accepted

## Context

Rust tests can be run via the standard `cargo test` command. However, we wanted better speed and output for our test
suite.

## Decision

Use `cargo-nextest` instead of `cargo test` for running Rust tests.

## Consequences

### Positive

- Faster test execution (parallel by default)
- Better output formatting and progress reporting
- Clearer failure messages
- Better CI integration

### Negative

- Additional dependency (auto-installed by check script if missing)
- Slightly different behavior than standard cargo test

### Notes

- The check script auto-installs cargo-nextest if it's not found
- CI uses nextest as well for consistency
