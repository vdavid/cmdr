# Per-navigation index verifier walks disk synchronously on the async runtime

**Severity:** medium
**Lens:** B — Concurrency
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/indexing/verifier.rs:76-177` (`verify_and_correct` invoked from the `tauri::async_runtime::spawn` block)

## What
`trigger_verification` spawns an async task per navigation (debounced 30 s, max 2 concurrent). Inside, `verify_and_correct`:

1. Calls `pool.with_conn(|conn| ...)` to bulk-read children from SQLite — synchronous, runs on the runtime worker.
2. Calls `std::fs::read_dir(&normalized)` — synchronous I/O on the runtime worker (`verifier.rs:138`).
3. Loops `dir_entry.file_name()` + `std::fs::symlink_metadata(dir_entry.path())` per entry — synchronous I/O per file (`verifier.rs:152-156`).

For a directory with 5-10k entries on a slow disk (Time Machine backup, deep node_modules, a network mount surfaced as a local path), each `symlink_metadata` is a syscall. There's no `spawn_blocking` around any of it.

## Why it matters
The verifier spawn happens via `tauri::async_runtime::spawn` — the task runs on a tokio worker thread. `read_dir` / `symlink_metadata` calls block the worker, not just the task. With `MAX_CONCURRENT_VERIFICATIONS = 2`, up to two workers can be parked at once on disk syscalls — and the architecture-patterns doc names the verifier as fully fire-and-forget, so callers don't `await` and can't cancel.

The 30 s per-path debounce limits how often a given path triggers a re-verify, but a user who pumps through 20 different directories (rapid keyboard-driven navigation, command palette jumps) can keep the two-slot pool occupied with cold-cache verifies for the whole session. None of these have a timeout. A single wedged path (the user's `~/.Trash` on an unresponsive eviction, a stale FUSE mount, a frozen iCloud directory) parks one worker until the syscall returns.

The fix in `space_poller.rs` discussion applies the same way here. The mitigation in `indexing/CLAUDE.md` ("uses `ReadPool` for lock-free bulk SQLite reads ... No `INDEXING` lock is held during verification") addresses the lock-contention angle but not the runtime-worker-blocking angle.

## Evidence
- `apps/desktop/src-tauri/src/indexing/verifier.rs:76-87`: `tauri::async_runtime::spawn(async move { let affected_paths = verify_and_correct(...).await; ... })`. The inner future never crosses a `spawn_blocking` boundary.
- `verifier.rs:138`: `std::fs::read_dir(&normalized)`.
- `verifier.rs:152-156`: per-entry `std::fs::symlink_metadata` inside a `for dir_entry in disk_entries.flatten()` loop.
- `indexing/CLAUDE.md` § verifier: "Phase 1 uses `ReadPool` (from `enrichment.rs`) for lock-free bulk SQLite reads into a `HashMap`, Phase 2 does all disk I/O without any lock." Phase 2's disk I/O is exactly what's running on the worker.

## Suggested fix
Wrap the disk-I/O phase (the `read_dir` + the per-entry stat loop) in a single `tokio::task::spawn_blocking`. The diff comparison after both phases is pure CPU + writer-message construction, which is fine on the async path. The blocking pool already handles indexing scans and other heavy work; one extra task per nav (rate-limited at 2 concurrent + 30 s debounce) is well within budget.

```rust
let disk_entries = tokio::task::spawn_blocking(move || {
    let rd = std::fs::read_dir(&normalized).ok()?;
    let mut disk_map = HashMap::new();
    for dir_entry in rd.flatten() {
        let metadata = std::fs::symlink_metadata(dir_entry.path()).ok()?;
        // ... fill disk_map
    }
    Some(disk_map)
}).await.ok().flatten();
```

## Notes
The architecture explicitly calls out per-navigation verification as "fully fire-and-forget"; that's still the right shape — just route the disk work through `spawn_blocking` so a wedged path can't park an async worker.
