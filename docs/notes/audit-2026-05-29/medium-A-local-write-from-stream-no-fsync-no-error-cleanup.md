# `LocalPosixVolume::write_from_stream` doesn't fsync and leaves partial file on chunk-write error

**Severity:** medium
**Lens:** A — Data safety
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/file_system/volume/backends/local_posix.rs:500-574`

## What
The local-FS `write_from_stream` (the destination side of every cross-volume copy where the target is local — SMB↔Local, MTP↔Local, InMemory↔Local) does two unsafe things compared to the local-FS chunked-copy path in `chunked_copy_with_metadata`:

1. It calls `file.flush()` (`std::io::Write::flush`) at the end, NOT `sync_data()` / `sync_all()`. `flush` only drains the in-process `BufWriter` (and `File` has no internal buffer, so flush is a no-op). The file's data sits in the OS page cache; the user pulling a USB drive seconds after "copy finished" loses the file. The sibling chunked-copy path explicitly addresses this with `sync_data` and a comment about USB-pull scenarios.

2. On any non-cancellation error (a `chunk_result?` propagating the source-stream error, a chunk-write failure on a slow disk hitting ENOSPC mid-stream), the function returns `Err` without removing the partial file at `dest_abs`. Only the cooperative-cancellation path (`on_progress(...) == Break(())`) cleans up the partial. The volume_copy post-loop branch only knows to clean partials for paths it tracked in `last_dest_path` / `in_flight_partials` — those are populated AFTER `write_from_stream` returns, so a mid-stream error never registers them and the partial stays on disk indefinitely.

## Why it matters
For (1): the user copies a file to an SD card or thumb drive, sees "Copy finished," ejects the drive (which on macOS Finder maps to `unmount` which sync_data's the FS automatically — but power-yanking the drive instead bypasses that). The file shows zero bytes or partial on the next system. The local chunked-copy path in `chunked_copy.rs:167-170` explicitly defends this scenario; the volume-write path silently doesn't.

For (2): a long-running cross-volume copy from a flaky SMB share to local disk. Network drops mid-stream on the 47th file. The source-side stream errors out. The 47th file's partial sits at the destination as a half-written file with the user's actual filename. There's no `.cmdr-tmp-` marker, no warning to the user that the file is incomplete — they'll discover it when they try to open it later and it's corrupt. Worst-case: combined with the cross-volume Overwrite finding, the original was deleted before the stream started, so the partial replaces the original under the same name.

## Evidence
`local_posix.rs:528-572`:
```rust
let mut bytes_written = 0u64;
while let Some(chunk_result) = stream.next_chunk().await {
    let chunk = chunk_result?;
    //                       ⚠ early-return on stream error; no cleanup of partial.
    ...
    let (file_ret, write_res) = spawn_blocking(move || {
        use std::io::Write;
        let res = file.write_all(&chunk);
        (file, res)
    }).await.unwrap();
    file = file_ret;
    write_res.map_err(VolumeError::from)?;
    //                                  ⚠ same: early-return on write error.
    ...

    if on_progress(bytes_written, size) == std::ops::ControlFlow::Break(()) {
        // Cancel path DOES clean up.
        drop(file);
        let partial = dest_abs.clone();
        let _ = spawn_blocking(move || std::fs::remove_file(&partial)).await;
        return Err(VolumeError::Cancelled(...));
    }
}

// Flush and close on the blocking pool.
let flush_res = spawn_blocking(move || {
    use std::io::Write;
    file.flush()
    //   ⚠ NOT sync_data() — leaves data in page cache.
}).await.unwrap();
```

Compare `chunked_copy.rs:167-170`:
```rust
dst_file.sync_data().map_err(|e| WriteOperationError::WriteError { ... })?;
```

## Suggested fix
1. Replace `file.flush()` with `file.sync_data()` in `write_from_stream`. The cost is one `fdatasync` per cross-volume copy on local destinations — measured in ms, dwarfed by the network/USB time of the copy itself.
2. Add an `Err` arm cleanup: wrap the chunk loop in a sentinel guard that on early return runs `std::fs::remove_file(&dest_abs)` on the blocking pool. Match the cooperative-cancel cleanup that already exists for `Break(())`. A `scopeguard::defer` or a small RAII drop guard works.

A more invasive but more correct fix: write to `dest.cmdr-tmp-<uuid>` throughout, fsync the temp on success, then atomic-rename to `dest`. That matches the "temp+rename" pattern documented as Cmdr's data-safety principle in AGENTS.md.

## Notes
- Closes a real instance of the AGENTS.md "Assume the hostile case (... crashed mid-operation)" principle. As written, the cross-volume copy path violates it.
- The MTP and SMB backends' `write_from_stream` overrides may have analogous gaps; this audit didn't drill into those backends' chunk loops in detail. Worth a follow-up scan to verify each backend's destination-side write path either uses temp+rename or cleans up the partial on error.
