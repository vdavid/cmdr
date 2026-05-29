# macOS `copyfile` and Linux `copy_file_range` paths skip per-file fsync; only chunked-copy guarantees durability before write-complete

**Severity:** low
**Lens:** A — Data safety
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/macos_copy.rs:265-361` (no fsync after `copyfile`)
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/linux_copy.rs:32-180` (no fsync after `copy_file_range`)
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/chunked_copy.rs:167-170` (the sync_data that other paths lack)
`apps/desktop/src-tauri/src/file_system/write_operations/helpers.rs:353-362` (`spawn_async_sync` global fallback)

## What
The chunked-copy path explicitly calls `dst_file.sync_data()` after the chunk loop so an unplugged USB drive doesn't lose page-cache-resident data the moment "Copy finished" toasts. Neither `copy_single_file_native` (macOS `copyfile(3)`) nor `copy_single_file_linux` (`copy_file_range(2)`) does the same — they hand the descriptor back to Rust which drops it, and the umbrella `spawn_async_sync` (which calls `libc::sync()` on a detached thread) is the only durability guarantee. That global `sync()` fires AFTER `write-complete` is emitted, races with anything else, and returns immediately (it doesn't wait for the syscall to finish on every fs).

So the per-file durability story is:
- Chunked copy (network FS on macOS, network FS on Linux, non-APFS on macOS): file is sync_data'd before write-complete. Good.
- APFS clonefile on macOS: clonefile is logically copy-on-write; the FS metadata change is durable when the syscall returns. Probably OK.
- macOS `copyfile(3)` data-copy mode on the SAME-APFS path with `needs_safe_overwrite`: goes through `safe_overwrite_file` which calls `copy_single_file_native` for the temp + a `fs::rename` chain. Rename to dest is atomic but the data file may still be in page cache.
- Linux `copy_file_range` for local same-FS: no fsync at all. The user unplugs the drive after "Copy finished" — could lose the file.

## Why it matters
USB / SD card / external SSD scenarios. The user copies a file, gets a "Copy finished" toast, ejects via Finder eject (which `unmount`s and flushes — safe) OR power-yanks the drive / Finder eject fails / drive is hot-removed (NOT safe). On chunked-copy paths the data is durable when write-complete fires. On native paths it's only as durable as the OS page cache, with the `libc::sync()` racing behind on a detached thread.

The user-facing pattern in modern macOS is that Finder's eject is the primary disconnect mechanism and it's safe — most users never pull a drive unceremoniously. So this is a low-severity finding. But:
- It contradicts the AGENTS.md "Assume the hostile case" principle.
- The chunked-copy path's sync_data comment ("the user can pull a USB drive after 'Copy finished' and lose a file that lived only in the OS page cache") shows the team recognized the problem; the fix just didn't propagate to the other strategies.

## Evidence
`chunked_copy.rs:158-172`:
```rust
// Flush the file's data pages durably before signalling success.
//
// `sync_all` defeating cancellation only applies *during* the chunk loop;
// at this point we've left the loop, there's nothing left to cancel, and
// the next thing the caller does is emit `write-complete`. Without this,
// the user can pull a USB drive after "Copy finished" and lose a file
// that lived only in the OS page cache.
dst_file.sync_data().map_err(|e| WriteOperationError::WriteError {
    path: dest.display().to_string(),
    message: format!("Couldn't flush destination to disk: {}", e),
})?;
```

`macos_copy.rs:334-360` (the entire post-copyfile epilogue — no sync_data, no fsync on the dest descriptor; we don't even hold one):
```rust
let result = unsafe { copyfile(src_cstring.as_ptr(), dst_cstring.as_ptr(), state, flags) };
log::debug!("copyfile: completed with result={}", result);
unsafe { copyfile_state_free(state); }
// ... cancellation check, error mapping ...
if result == 0 { Ok(()) } else { ... }
```

`helpers.rs:353-362`:
```rust
/// Spawns a background thread to call sync() for durability.
/// This ensures writes are flushed to disk without blocking the completion event.
pub(super) fn spawn_async_sync() {
    std::thread::spawn(|| {
        #[cfg(unix)]
        unsafe { libc::sync(); }
    });
}
```

## Suggested fix
After `copyfile` and `copy_file_range` succeed, `open` the destination, `fsync`/`sync_data` it, close it. Cheap (an fdatasync on a freshly-written file is ms-class) and aligned with the chunked-copy path's behavior. For the clonefile path on APFS the COW semantics mean fsync mostly only durables the metadata; still cheap.

Alternative: replace `spawn_async_sync` (global background `sync()`) with a per-volume `syncfs(fd)` on the destination volume, gated to actually `await`-style block before the `write-complete` emit. Costs latency but matches user intent ("I want to safely eject").

Filing as low because real-world impact requires aggressive disk-yanking and the user has the eject affordance for the safe path.

## Notes
- The doc comment on `spawn_async_sync` ("ensures writes are flushed to disk without blocking the completion event") is honest about the trade-off; the trade-off is just biased away from durability.
- Adjacent finding: `medium-A-local-write-from-stream-no-fsync-no-error-cleanup.md` (cross-volume copy destination side, same class of bug, more impactful because it's the SMB↔Local and MTP↔Local hot paths).
