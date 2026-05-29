# Many Tauri commands still return `Result<T, String>` instead of typed `IpcError`

**Severity:** low
**Lens:** C — Error handling
**Confidence:** medium

## Location

`apps/desktop/src-tauri/src/commands/` — counts of `Result<..., String>` per file:

| File | Count |
|---|---|
| `ui.rs` | 28 |
| `clipboard.rs` | 12 |
| `file_system/listing.rs` | 11 |
| `search.rs` | 9 |
| `indexing.rs` | 9 |
| `file_viewer.rs` | 7 |
| `selection.rs` | 6 |
| `network.rs` | 6 |
| `rename.rs` | 5 |
| `file_system/drag.rs` | 5 |
| `error_reporter.rs` | 4 |
| `settings.rs`, `mtp.rs`, `mcp.rs`, `file_system/e2e_support.rs` | 2 each |

117 sites across 20 files.

## What

The architecture migration documented in `commands/CLAUDE.md` says timeout-protected and `Result`-returning commands should return `IpcError` (`{ message, timedOut }`) or `TimedOut<T>` so the frontend can distinguish "timed out" from "real error" from "empty result". Many commands still return `Result<T, String>`, losing both the timeout signal and any structured error info. The frontend collapses every error into a single string message.

## Why it matters

- The FE can't show different copy for "device disconnected" vs "operation timed out" vs "permission denied" without an unsafe `.includes("...")` string match (which the `cmdr/no-error-string-match` lint bans).
- The friendly-error system in `file_system/volume/friendly_error/` only fires for commands that already round-trip a `VolumeError` / `WriteOperationError`. Listing/clipboard/UI-meta commands skip it entirely and the user sees a raw OS message.
- Pure cache lookups (`get_total_count`, `find_file_index`, etc.) don't really need timeout-aware shape, so the contract is "cluster by domain, not blanket migrate."

## Evidence

A representative non-trivial straggler — `commands/file_system/listing.rs`:
```rust
pub fn get_total_count(listing_id: String, include_hidden: bool) -> Result<usize, String> { ... }
```

The IpcError pattern (`commands/file_system/mod.rs:88` and a few siblings) is wired with explicit `// allowed-error-string-match: IpcError is a flat struct; message is the signal` opt-outs — so the FE-side test of `IpcError.message` *is* already accepted. The friction is migration cost: 117 call sites, the matching FE call sites, and `bindings.ts` regeneration.

## Suggested fix

Not for launch. Post-launch cleanup pass:

1. Triage the 117 by category:
   - **Pure cache lookups** (listing.rs, font_metrics.rs, ui.rs `set_menu_context`): keep `Result<T, String>` — they don't touch the FS and `String` is fine.
   - **Filesystem-touching** (rename.rs, indexing.rs, search.rs `prepare_search_index`, clipboard.rs file IO): migrate to `IpcError` so timeout / friendly-error info survives.
   - **External-call** (network.rs upgrade flow, mcp.rs lifecycle, licensing.rs): migrate to a typed enum returning the same JSON shape, since these errors are user-visible.
2. Run `cargo mutants` on the migrated files to confirm new error variants are actually tested.

## Notes

No commands today route a user-visible OS error through `Result<_, String>` *and* drop it; the typed `VolumeError → FriendlyError` chain is the main mapping pipeline and that's fully shaped. This is a polish item, not a correctness gap.
