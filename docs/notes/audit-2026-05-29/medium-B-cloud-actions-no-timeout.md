# `cloud_make_available_offline` and `cloud_remove_download` have no timeout

**Severity:** medium
**Lens:** B — Concurrency
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/commands/ui.rs:519-537`

## What
Both Tauri commands wrap their work in `tokio::task::spawn_blocking(...)` but skip the
`tokio::time::timeout` / `blocking_with_timeout` wrappers that every other filesystem-touching command in this crate uses (see `commands/CLAUDE.md` § "Timeout-protected I/O").

```rust
#[tauri::command]
pub async fn cloud_make_available_offline(path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        crate::file_system::cloud_actions::request_download(std::path::Path::new(&path))
    })
    .await
    .map_err(|e| e.to_string())?
}
```

`cloud_remove_download` has the identical shape.

## Why it matters
`request_download` and `evict_item` wrap `FileManager.startDownloadingUbiquitousItem(at:)` /
`evictUbiquitousItem(at:)` (per `file_system/CLAUDE.md` § "Cloud actions and 'Open with' (macOS)"). Those are synchronous XPC round-trips into iCloud's file provider daemon, which can wedge on bad network, a paused account, a long iCloud queue, or a stuck FileProvider extension. With no timeout:

- The frontend `await`-ing the IPC never gets a response — the dialog spinner stays up indefinitely, no "this is taking too long" affordance, no way to surface the wedge.
- A blocking thread stays parked in the FileProvider XPC call. Other `spawn_blocking` work (rename, trash, etc.) shares the same pool, so cumulative stalls reduce the pool's effective capacity.
- The architecture mandate from `commands/CLAUDE.md` is explicit: "`blocking_with_timeout` for all filesystem-touching commands, not just read-only ones … `spawn_blocking` alone doesn't protect against hung NFS/SMB mounts where even a simple `path.exists()` can block indefinitely. The timeout wrapper … returns a fallback value (or error for `Result`-returning commands) instead of freezing the IPC thread or exhausting the blocking pool."

These two are the only fs-touching commands in `ui.rs` that violate the rule.

## Evidence
- `commands/CLAUDE.md:42-49` documents the timeout policy and lists `move_to_trash` (15 s), `create_directory` / `rename_file` (5 s), reads (2 s), scans (30 s) as the canonical buckets.
- Every other command in `commands/rename.rs`, `commands/file_system/listing.rs`, `commands/file_system/git.rs`, `commands/volumes.rs`, `commands/sync_status.rs`, `commands/icons.rs`, `commands/eject.rs`, `commands/file_viewer.rs`, `commands/file_system/write_ops.rs`, `commands/file_system/volume_copy.rs` follows it.
- `cloud_actions.rs` calls `FileManager` ubiquity APIs that block on XPC to `cloudd` / `fileproviderd`; those daemons have been known to hang for minutes when iCloud is in a bad state.

## Suggested fix
Wrap with `blocking_result_with_timeout` (or the raw `tokio::time::timeout` pattern). A 30 s budget seems reasonable: the operation itself is fire-and-forget on the iCloud side (the download / eviction continues even if our await times out), so a long timeout doesn't risk corrupting state. Map the `Elapsed` case to a typed error variant (or `IpcError::timeout()` if/when the signature is loosened from `Result<(), String>` to `Result<(), IpcError>`) so the FE can show "Couldn't reach iCloud right now — give it another try" rather than spinning forever.

## Notes
The actions are best-effort by design (the user can retry from Finder), so the right behavior on timeout is to release the IPC, not retry server-side.
