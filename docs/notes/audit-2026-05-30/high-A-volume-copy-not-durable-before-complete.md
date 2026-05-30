# Volume copy/move to a local-FS destination reports "complete" before data is durable

**Severity:** high
**Lens:** A — Data safety
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/file_system/volume/backends/local_posix.rs:576-590` (`write_from_stream` finish: `flush()` only, no `sync_data`); `apps/desktop/src-tauri/src/file_system/write_operations/transfer/volume_copy.rs` and `transfer/volume_move.rs` (emit `write-complete` without calling `flush_created_destinations`).

## What
Every cross-volume copy/move whose destination is a `LocalPosixVolume` (SMB→Local, MTP→Local, and any Local↔Local routed through the volume engine) writes via `LocalPosixVolume::write_from_stream`, which finishes the chunk loop with only `file.flush()` — a userspace flush that is a no-op on a raw `std::fs::File`, NOT `sync_data`/`fdatasync`. The volume copy and volume move handlers then emit `write-complete` directly; unlike the local-FS engine (`copy.rs:399`, `move_op.rs:292/681`), they never call `helpers::flush_created_destinations`. So the bytes can still live only in the OS page cache when the user is told the copy finished.

## Why it matters
This is precisely the scenario the project's durability decision says it defends against (transfer CLAUDE.md § "Durability": *"complete" means "you can eject now," not "buffered in the page cache"*). A user imports photos from an Android phone (MTP) or a NAS (SMB) onto a local USB stick / SD card, sees "Copy finished," and ejects — or the machine sleeps/crashes. Files that were only buffered are lost. On a cross-volume **move** they're lost from both sides (source delete only runs after the copy reports `Ok`, but the copy's bytes were never fsynced). The local-FS-to-local-FS path is durable; the volume path that lands on the same local disk is not — the guarantee is silently inconsistent depending on which engine ran the copy, and the user has no way to tell which one it was.

## Evidence
```rust
// local_posix.rs:576-583  write_from_stream, after the chunk loop:
// Flush and close on the blocking pool.
let flush_res = spawn_blocking(move || {
    use std::io::Write;
    file.flush()          // ← userspace flush only; std::fs::File doesn't buffer, so ~no-op. No sync_data/fdatasync, no parent-dir fsync.
})
.await
.unwrap();
flush_res.map_err(VolumeError::from)?;
```
```
$ grep -rn flush_created_destinations transfer/
copy.rs:399      ← local copy: durable
move_op.rs:292   ← same-FS local move: durable
move_op.rs:681   ← cross-FS local move: durable
# volume_copy.rs / volume_move.rs: NOT PRESENT — volume path never flushes
```

## Suggested fix
Make the volume streaming-write path durable before "complete." Cleanest: add `file.sync_data()` (and a best-effort parent-dir fsync) at the end of `LocalPosixVolume::write_from_stream` after the chunk loop, mirroring `chunked_copy.rs`'s per-file `sync_data` — this also gives the "durable as each file completes" property the chunked path already has, so a crash mid-batch leaves earlier files safe. Alternatively, give the volume copy/move handlers an end-of-op pass equivalent to `flush_created_destinations` that fsyncs each local-FS destination (resolved via `dest_volume.local_path()`) and emits the same `Flushing` phase for UI consistency. Non-local destinations (MTP/SMB/InMemory) need no change — their durability is the device/server's concern.

## Notes
Pair the fix with a test analogous to `local_copy_emits_flushing_phase_before_complete` but driving a volume→local copy and asserting `sync_data` was invoked (or asserting the `Flushing` phase fires). Related to the broader durability decision documented in `write_operations/CLAUDE.md` § "Decision: Copy and move are durable before they report complete" — that decision's wording is a blanket promise, but only the local-FS engine implements it.
