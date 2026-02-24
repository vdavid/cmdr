# cfg-gate lint for platform-specific Rust crates

## Problem

Rust code that uses macOS-only crates (declared under `[target.'cfg(target_os = "macos")'.dependencies]` in Cargo.toml)
compiles fine on macOS but fails on Linux if the `use` statement isn't wrapped in `#[cfg(target_os = "macos")]`. CI runs
on Linux so it catches this, but only after a push. We want to catch it locally, on any platform, instantly.

## Approach

Add a new check to the Go-based check runner (`scripts/check/`) that:

1. **Parses `Cargo.toml`** using a TOML library (`github.com/BurntSushi/toml`) to extract macOS-only crate names from
   the `[target.'cfg(target_os = "macos")'.dependencies]` section. Converts crate names to Rust module form (hyphens to
   underscores, for example `cmdr-fsevent-stream` becomes `cmdr_fsevent_stream`). A proper TOML parser handles
   multi-line inline tables (like `objc2-foundation` with its multi-line features array) without fragile regex hacks.
2. **Builds a set of "module-gated" files** by scanning `lib.rs` and `mod.rs` files for
   `#[cfg(target_os = "macos")] mod <name>;` patterns, resolving each to the corresponding `.rs` file (or
   `<name>/mod.rs`). Files inside a cfg-gated module are inherently safe — everything in them is already gated by the
   parent's `mod` declaration. These files are skipped during the `use` scan.
3. **Scans remaining `.rs` files** under `apps/desktop/src-tauri/src/` for `use <crate>::` statements that reference any
   of those crates (including indented `use` inside function bodies and `pub use` re-exports).
4. **Verifies each match is gated.** For each `use` of a macOS-only crate, walks backwards from that line to check that
   a preceding `#[...]` attribute contains `target_os = "macos"` (including compound forms like
   `cfg(all(test, target_os = "macos"))`).
5. **Reports violations** with file path and line number.

## Why module-level gating matters

The codebase uses two gating patterns:

- **Per-line gating:** `#[cfg(target_os = "macos")]` before each `use` statement (for example, `watcher.rs`, `icons.rs`).
- **Module-level gating:** `#[cfg(target_os = "macos")] mod foo;` in `lib.rs`/`mod.rs`, where everything inside `foo.rs`
  is inherently gated (for example, `drag_image_swap`, `accent_color`, `volumes`, `network`, `mtp`, `permissions`,
  `macos_icons`, `file_system/macos_metadata`, `file_system/volume/mtp`).

Module-level gating is actually the more common pattern. Without handling it, the checker would produce dozens of false
positives.

## Scope

Only the `[target.'cfg(target_os = "macos")'.dependencies]` section. We could extend this to `cfg(unix)` later, but
`libc` is the only unix-only dep and it's common enough to not be worth the noise.

Note: some cross-platform crates (`chrono`, `bytes`) live in the macOS-only section because they're only needed for
macOS features. The checker correctly flags ungated uses of these too — if someone adds `use chrono::` in non-macOS
code, it genuinely won't compile on Linux.

## Check runner integration

- **File:** `scripts/check/checks/desktop-rust-cfg-gate.go`
- **ID:** `desktop-rust-cfg-gate`
- **Nickname:** `cfg-gate`
- **Display name:** `cfg-gate`
- **App/tech:** `AppDesktop` / `"🦀 Rust"`
- **Slow:** No (pure text scanning, should run in milliseconds)
- **Depends on:** Nothing (independent, can run in parallel with everything)
- **Position in registry:** After `desktop-rust-jscpd`, before `desktop-rust-tests`
- **New dependency:** `github.com/BurntSushi/toml` (MIT license — run `cargo deny`-equivalent check: it's fine)

## Algorithm

```
1. Read and parse Cargo.toml with BurntSushi/toml
2. Extract crate names from the [target.'cfg(target_os = "macos")'.dependencies] table
3. Convert hyphens to underscores, build set of macOS-only crate module names
4. Build set of module-gated files:
   a. Walk all lib.rs and mod.rs files in src-tauri/src/
   b. For each, find lines matching: #[cfg(target_os = "macos")] followed by mod <name>;
      (possibly with blank lines or other attributes in between)
   c. Resolve <name> to <dir>/<name>.rs or <dir>/<name>/mod.rs
   d. If the resolved file is a directory module (mod.rs), recursively add all .rs files under it
   e. Collect all resolved paths into a "skip set"
5. Walk all .rs files in src-tauri/src/, skipping files in the skip set
6. For each non-skipped file, scan lines:
   a. If line matches `use <macos_crate>::` (ignoring leading whitespace, optional `pub `):
      - Walk backwards over blank lines and #[...] attribute lines
      - Look for `target_os = "macos"` in any of those attributes
      - If not found, record a violation: {file, line number, crate name}
7. If any violations, return error listing them all
8. If none, return success with count of gated uses verified + count of module-gated files skipped
```

## Success message examples

- `23 gated uses of 8 macOS-only crates verified (12 files skipped via module-level gating)` (all good)
- Error: `apps/desktop/src-tauri/src/indexing/watcher.rs:5: use of macOS-only crate 'cmdr_fsevent_stream' without
  #[cfg(target_os = "macos")]`

## Testing

Add a test in `scripts/check/checks/` that:
- Constructs a minimal Cargo.toml and a few `.rs` files in a temp directory
- Verifies that per-line gated uses pass
- Verifies that ungated uses are caught
- Verifies that uses inside module-gated files are skipped (not flagged)
- Verifies that crate name extraction handles hyphens, inline tables (`{ version = "..." }`), git deps, and multi-line
  feature arrays
- Verifies the module-gated file resolver handles both `<name>.rs` and `<name>/mod.rs` layouts

## Task list

- [x] Add `github.com/BurntSushi/toml` dependency to `scripts/check/go.mod`
- [x] Implement crate name extraction from Cargo.toml (TOML parser, convert hyphens to underscores)
- [x] Implement module-gated file detection (scan `lib.rs`/`mod.rs` for cfg-gated `mod` declarations, resolve to files)
- [x] Implement `.rs` file scanner (find `use <crate>::` lines, walk backwards for `cfg` attributes, skip gated files)
- [x] Wire up as `RunCfgGate` in `desktop-rust-cfg-gate.go`
- [x] Register in `registry.go` (after jscpd, before tests)
- [x] Add unit tests for crate extraction, module-gating detection, and use-line scanning
- [x] Run `./scripts/check.sh --check cfg-gate` to verify it passes on the current codebase
- [x] Run `./scripts/check.sh --go` to verify Go checks pass (gofmt, vet, staticcheck, and the rest)
- [x] Add `desktop-rust-cfg-gate` to the `--check` list in `AGENTS.md`
