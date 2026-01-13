# ADR 003: Disable cargo-deny advisories check

## Status

Accepted

## Context

`cargo-deny` can check for:

- Security advisories (known vulnerabilities)
- License compliance
- Banned dependencies
- Source verification

We encountered many advisory warnings from Tauri's transitive dependencies (gtk3-rs, unic-\*, fxhash, proc-macro-error,
etc.) that we cannot control.

## Decision

Disable the advisories check in `deny.toml` and rely on `cargo-audit` for security vulnerability scanning instead.

## Consequences

### Positive

- Check script doesn't fail due to unmaintained transitive dependencies we can't control
- Still get security scanning via cargo-audit
- License, bans, and sources checks still active

### Negative

- Less comprehensive security checking from cargo-deny specifically
- Need to maintain two tools (cargo-deny + cargo-audit)

### Notes

- See comment in src-tauri/deny.toml explaining this decision
- When Tauri updates these dependencies, we can re-enable advisories check
- cargo-audit still catches critical vulnerabilities
