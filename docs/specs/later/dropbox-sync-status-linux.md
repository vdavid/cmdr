# Dropbox sync status on Linux

## Context

Linux gap #16 from `docs/specs/linux-remaining-gaps.md`. The macOS implementation detects Dropbox/iCloud sync
status via `stat()` + NSURL APIs. On Linux, the non-macOS fallback returns an empty map — no sync icons shown.

Dropbox on Linux exposes sync status via a Unix domain socket at `~/.dropbox/command_socket`. This is the same
protocol the Nautilus Dropbox extension uses. Fallback: `dropbox filestatus <path>` CLI.

Linux Dropbox removed Smart Sync in 2019, so `OnlineOnly` and `Downloading` states won't occur — only `Synced`,
`Uploading` (mapped from "syncing"), and `Unknown`.

## Plan

### 1. Extract shared `SyncStatus` enum to `sync_status_types.rs`

Create `file_system/sync_status_types.rs` — always compiled, contains:
- `SyncStatus` enum (moved from `sync_status.rs`)
- Serialization test (moved from `sync_status.rs`)

Update `sync_status.rs` (macOS) to `pub use super::sync_status_types::SyncStatus` instead of defining the enum.
Remove the serialization test from it (now lives in the shared file).

### 2. Update `file_system/mod.rs` module gating

Follow the `network/mod.rs` `#[path]` pattern:
```rust
pub mod sync_status_types;

#[cfg(target_os = "macos")]
pub mod sync_status;              // resolves to sync_status.rs

#[cfg(target_os = "linux")]
#[path = "sync_status_linux.rs"]
pub mod sync_status;
```

### 3. Create `sync_status_linux.rs`

Core new file. Fallback chain: socket -> CLI -> empty map.

**Socket protocol:**
- Connect to `~/.dropbox/command_socket` (Unix domain socket)
- Send: `icon_overlay_file_status\npath\t<filepath>\ndone\n`
- Receive: `status\t<status_string>\ndone\n`
- Reuse one connection for all paths in a batch (clone stream for separate reader/writer to avoid `BufReader` buffering issues)

**Status mapping:**
| Dropbox string | SyncStatus | Reason |
|---|---|---|
| "up to date" | `Synced` | Matches cloud |
| "syncing" | `Uploading` | No direction info; Linux has no Smart Sync so it's always upload |
| "unsyncable" | `Unknown` | Can't sync (permissions, path length) |
| "unwatched" | `Unknown` | Outside Dropbox folder |

**CLI fallback:** `dropbox filestatus <path>` — output format `<path>: <status>`. Parse with `rfind(": ")` to handle colons in paths. One subprocess per path.

**Key functions:**
- `get_sync_statuses(paths) -> HashMap<String, SyncStatus>` — public API, matches macOS signature
- `get_sync_statuses_with_socket_path(paths, socket_path)` — testable version with injected path
- `query_statuses_via_socket(socket_path, paths)` — batch socket query
- `read_socket_response(reader)` — parse one response from socket
- `query_statuses_via_cli(paths)` — CLI fallback
- `map_dropbox_status(status_str) -> SyncStatus` — string-to-enum mapping
- `parse_cli_status_line(line) -> Option<(String, SyncStatus)>` — parse one CLI output line

**No new dependencies.** Uses `std::os::unix::net::UnixStream`, `dirs` (already in Cargo.toml), `log` (already used).

### 4. Update `commands/sync_status.rs`

Widen `#[cfg]` gates from `target_os = "macos"` to `any(target_os = "macos", target_os = "linux")`.
The `lib.rs` command registration is already platform-independent — no change needed there.

### 5. Tests (all in `sync_status_linux.rs`)

**Pure parsing tests (no I/O):**
- `map_dropbox_status` — all known strings, case-insensitive, whitespace trimming, unknown strings
- `parse_cli_status_line` — valid lines, syncing status, colon-in-path edge case, invalid lines

**Socket integration tests (mock server):**
- Create real Unix socket in `tempfile::TempDir`, spawn thread as mock Dropbox daemon
- `socket_query_returns_synced` — single file query
- `socket_batch_query_multiple_files` — two files, sequential on one connection
- `nonexistent_socket_returns_empty` — graceful degradation
- `empty_paths_returns_empty_map` — early return

CLI fallback is tested implicitly: parsing logic is covered by `parse_cli_status_line` tests, and the graceful
degradation test covers the case where neither socket nor CLI is available.

### 6. Update `commands/CLAUDE.md`

Update the sync_status entry to reflect that Linux now delegates to `file_system::sync_status` too.

## Files to modify

| File | Action |
|---|---|
| `src-tauri/src/file_system/sync_status_types.rs` | Create — shared `SyncStatus` enum |
| `src-tauri/src/file_system/sync_status.rs` | Edit — remove enum definition, `pub use` from shared |
| `src-tauri/src/file_system/sync_status_linux.rs` | Create — socket protocol, CLI fallback, tests |
| `src-tauri/src/file_system/mod.rs` | Edit — add `sync_status_types`, `#[path]` gating for Linux |
| `src-tauri/src/commands/sync_status.rs` | Edit — widen `#[cfg]` to include Linux |
| `src-tauri/src/commands/CLAUDE.md` | Edit — update sync_status description |

## Verification

1. `./scripts/check.sh --check rust-tests` — all Rust tests pass (including new socket mock tests)
2. `./scripts/check.sh --check clippy` — no warnings
3. `./scripts/check.sh --check rustfmt` — formatting
4. `./scripts/check.sh --check cfg-gate` — macOS-only imports properly gated
