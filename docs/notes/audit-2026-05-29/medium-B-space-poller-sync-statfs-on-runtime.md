# Space poller can block a runtime worker on a hung mount despite its timeout

**Severity:** medium
**Lens:** B — Concurrency
**Confidence:** medium

## Location
`apps/desktop/src-tauri/src/space_poller.rs:166-185`

## What
The poll-loop tick wraps the volume's space fetch in `tokio::time::timeout(FETCH_TIMEOUT, fetch)`. Inside `fetch`, if the `Volume`-level path returns `None` (or fails), the code falls back to the synchronous `fetch_space_for_path(&path_clone)`:

```rust
let fetch = async move {
    if let Some(vol) = vol_clone
        && let Ok(info) = vol.get_space_info().await
    { return Some(...); }
    fetch_space_for_path(&path_clone)  // sync statfs / NSURL, NOT spawn_blocking
};
let space = match tokio::time::timeout(FETCH_TIMEOUT, fetch).await { ... };
```

`fetch_space_for_path` runs `volumes::get_volume_space` on macOS (`statfs` + NSURL) and `volumes_linux::get_volume_space` on Linux (`statvfs`). Both are sync FFI calls executed inline on the async task — there is no `spawn_blocking` wrap.

## Why it matters
`tokio::time::timeout` only times out the *await*. It cannot interrupt a blocked syscall on a tokio worker thread. If `statfs` / NSURL wedges on a stuck NFS mount, a stale SMB mount, or a paused iCloud Drive root, the `timeout` future never gets to fire because the worker thread is stuck executing the FFI call and can't poll the timer. The `tokio::time::timeout` returns when the worker eventually returns from the syscall (could be minutes).

The poller's `tokio::time::sleep(Duration::from_secs(1))` runs the same loop forever, so the same path is re-attempted every `interval_secs`. On a wedged mount this keeps a worker pinned indefinitely while the architecture's "rock solid: never block the main thread" principle expects the IPC/runtime pool to stay responsive.

`Volume::get_space_info` on `LocalPosixVolume` does the same internally (synchronous `statvfs` FFI without `spawn_blocking`) — most of the calls today go through this path, so the fallback is the secondary concern but the primary risk is the same.

The risk is bounded by `MAX_TOKIO_WORKERS`. If two or three different stuck mounts are being watched (a Time Machine drive that disconnected, an SMB share whose server went away, an NFS mount blocked on a network partition), they can together park enough workers to noticeably degrade IPC throughput.

## Evidence
- `apps/desktop/src-tauri/src/space_poller.rs:171-181` — the inline fetch closure.
- `apps/desktop/src-tauri/src/space_poller.rs:182-184` — the timeout wraps the await but cannot interrupt the syscall.
- `apps/desktop/src-tauri/src/space_poller.rs:197-220` — `fetch_space_for_path` calls sync FFI directly.
- `file_system/volume/CLAUDE.md` § "Gotchas": "On macOS, never use `statvfs` alone for disk space" documents the macOS-specific quirk but doesn't address the blocking nature.
- `commands/CLAUDE.md:42-49` calls out the same anti-pattern: "`spawn_blocking` alone doesn't protect against hung NFS/SMB mounts where even a simple `path.exists()` can block indefinitely."

## Suggested fix
Move the sync FFI behind `tokio::task::spawn_blocking` *inside* the `fetch` closure, then keep the outer `tokio::time::timeout` wrap. The blocking-pool thread can stay parked in the syscall and still be killed by tokio when the timeout fires — the worker thread no longer is. `commands/volumes.rs::get_volume_space` already uses `blocking_with_timeout_flag` (see line 51) — the same wrapper would apply cleanly here. Also worth checking that `LocalPosixVolume::get_space_info` itself is `spawn_blocking`-wrapped (it should be — same pattern as `LocalPosixVolume`'s I/O methods).

## Notes
Not a hot finding for typical home users (a stuck mount is rare), but the file manager's whole pitch is "hostile filesystems are normal." This is the canonical case the architecture's stuck-mount story should defend against.
