# cfg-gate lint for platform-specific Rust crates

## Problem

Rust code that uses macOS-only crates (declared under `[target.'cfg(target_os = "macos")'.dependencies]` in Cargo.toml)
compiles fine on macOS but fails on Linux if the `use` statement isn't wrapped in `#[cfg(target_os = "macos")]`. CI runs
on Linux so it catches this, but only after a push. We want to catch it locally, on any platform, instantly.

## Approach

Add a new check to the Go-based check runner (`scripts/check/`) that:

1. **Parses `Cargo.toml`** to auto-derive the list of macOS-only crate names from the
   `[target.'cfg(target_os = "macos")'.dependencies]` section. Converts crate names to Rust module form (hyphens to
   underscores, for example `cmdr-fsevent-stream` becomes `cmdr_fsevent_stream`).
2. **Scans all `.rs` files** under `apps/desktop/src-tauri/src/` for `use <crate>::` statements that reference any of
   those crates.
3. **Verifies each match is gated.** For each `use` of a macOS-only crate, walks backwards from that line to check that
   either:
   - The line itself or a preceding non-empty line has `#[cfg(target_os = "macos")]`, or
   - The use is inside a block that's already gated (for example, a `mod` block or `#[cfg(test)]` + `target_os` combo).
   For simplicity, the initial version only checks the "preceding `#[cfg(...)]` attribute" pattern, which covers 100% of
   the current codebase's usage. The block-level gating can be added later if needed.
4. **Reports violations** with file path and line number.

## Scope

Only the `[target.'cfg(target_os = "macos")'.dependencies]` section. We could extend this to `cfg(unix)` later, but
`libc` is the only unix-only dep and it's common enough to not be worth the noise.

## Check runner integration

- **File:** `scripts/check/checks/desktop-rust-cfg-gate.go`
- **ID:** `desktop-rust-cfg-gate`
- **Nickname:** `cfg-gate`
- **Display name:** `cfg-gate`
- **App/tech:** `AppDesktop` / `"🦀 Rust"`
- **Slow:** No (pure text scanning, should run in milliseconds)
- **Depends on:** Nothing (independent, can run in parallel with everything)
- **Position in registry:** After `desktop-rust-jscpd`, before `desktop-rust-tests`

## Algorithm

```
1. Read Cargo.toml
2. Find the [target.'cfg(target_os = "macos")'.dependencies] section
3. Extract crate names, convert hyphens → underscores → build set
4. Walk all .rs files in src-tauri/src/
5. For each file, scan lines:
   a. If line matches `use <macos_crate>::` (ignoring leading whitespace and `pub`):
      - Walk backwards over blank lines and `#[...]` attribute lines
      - Look for `cfg(target_os = "macos")` in any of those attributes (including compound forms
        like `cfg(all(test, target_os = "macos"))`)
      - If not found, record a violation: {file, line number, crate name}
6. If any violations, return error listing them all
7. If none, return success with count of gated uses verified
```

## Success message examples

- `23 gated uses of 8 macOS-only crates verified` (all good)
- Error: `apps/desktop/src-tauri/src/indexing/watcher.rs:5: use of macOS-only crate 'cmdr_fsevent_stream' without #[cfg(target_os = "macos")]`

## Testing

Add a test in `scripts/check/checks/` that:
- Constructs a minimal Cargo.toml snippet and a few `.rs` file contents (as strings/temp files)
- Verifies that correctly gated uses pass
- Verifies that ungated uses are caught
- Verifies that crate name extraction handles hyphens, inline tables (`{ version = "..." }`), and git deps

## Task list

- [ ] Implement crate name extraction from Cargo.toml (parse the target section, convert hyphens to underscores)
- [ ] Implement `.rs` file scanner (find `use <crate>::` lines, walk backwards for `cfg` attributes)
- [ ] Wire up as `RunCfgGate` in `desktop-rust-cfg-gate.go`
- [ ] Register in `registry.go` (after jscpd, before tests)
- [ ] Add unit tests for extraction and scanning logic
- [ ] Run `./scripts/check.sh --check cfg-gate` to verify it passes on the current codebase
- [ ] Run `./scripts/check.sh --go` to verify Go checks pass (gofmt, vet, staticcheck, and the rest)
