# Copy completes without fsync; power loss can lose just-copied file

**Severity:** low **Lens:** A — Data safety **Confidence:** medium

## Location

`apps/desktop/src-tauri/src/file_system/write_operations/transfer/chunked_copy.rs:159-160` (explicit removal of
`sync_all`) `apps/desktop/src-tauri/src/file_system/write_operations/helpers.rs:307-316` (`spawn_async_sync`)

## What

After a chunked copy finishes writing the destination file, the code:

1. Does NOT call `sync_all` / `fdatasync` on the destination file handle (comment at line 159 explicitly notes this was
   removed).
2. Closes the file handle (which only flushes user-space buffers).
3. Eventually triggers a global `libc::sync()` in a detached thread via `spawn_async_sync` (called from
   `copy_files_with_progress_inner` after the loop completes).

Between steps 2 and 3 — and during step 3, which is async-fire-and-forget — the frontend has already received
`write-complete`. The user sees "Copy finished" and may yank the USB drive, sleep the laptop, or unmount the network
share. The destination file's data is still only in OS page cache and may not have hit physical storage. A power loss or
hard yank within that window can leave the destination file truncated, empty, or partially written despite the UI
claiming success.

## Why it matters

- **External media (USB drives, SD cards):** The exact use case `Move` and `Copy` are most often used for. Mac users
  routinely eject by pulling the cable after copying — that's the OS's job to handle, but only if the data is actually
  flushed. The current pattern hands "complete" to the user before the data is durable.
- **Network mounts:** SMB writes are usually synchronous from the protocol's perspective, but the OS-side mount may
  still buffer. AFP / NFS even more so. The chunked-copy path is specifically the one taken for network FS (per
  `copy_strategy.rs`).
- **Crash protection in general:** AGENTS.md § "Protect the user's data — Design for the crash mid-operation." Saying
  "the operation completed" before the bytes are on disk violates that.

This is industry-standard latency-vs-durability tradeoff and many file managers do the same — but Cmdr's stated
principle is to do the durable thing. Fine for the live-edit case where the user controls when to remove the drive; not
fine for the "finished, you can disconnect now" case.

## Evidence

```rust
// chunked_copy.rs
// Note: sync_all() removed - network writes are synchronous and the async sync
// at operation completion handles durability. Blocking sync defeats cancellation.

Ok(total_bytes)
```

```rust
// helpers.rs
pub(super) fn spawn_async_sync() {
    std::thread::spawn(|| {
        #[cfg(unix)]
        unsafe {
            libc::sync();
        }
    });
}
```

`libc::sync()` is "best effort" per POSIX: it schedules the flush but doesn't wait for it.

## Suggested fix

Two layers, in order of cost:

1. **Per-file `fdatasync` after the last chunk** in `chunked_copy::copy_data_chunked`, **only on the final pass** (don't
   sync between chunks; that defeats the streaming throughput). Add roughly 10-50ms of latency to the per-file path.
   Negligible on the common case (most copies are batches of small files where the OS coalesces the writes); meaningful
   on the few-large-files case. Make it opt-out behind `advanced.fastCopy` if the latency hit shows up.
2. **Synchronous `sync` on copy completion** before emitting `write-complete`. Replace the detached `spawn_async_sync`
   thread with a blocking `libc::sync()` (or `fdatasync` per recorded destination) on the spawn_blocking thread. This is
   what guarantees "complete" means durable.

For the macOS APFS clone path (`copyfile(3) + COPYFILE_CLONE`), durability isn't a concern — clonefile is metadata-only
and atomic; the file's data is shared with the source until COW kicks in. Skip the sync there.

## Notes

The "blocking sync defeats cancellation" rationale in the removed comment is true mid-copy but doesn't apply after the
last chunk has been written: at that point there's nothing left to cancel. Sync-on-last-chunk doesn't conflict with the
cancel-between-chunks contract.

`spawn_async_sync` calling `libc::sync()` rather than `fdatasync` on the specific files is overkill — it flushes every
dirty page on the system, which can pause the whole machine for seconds on a busy laptop. Targeting `fdatasync` per
touched file is both more durable AND less invasive.
