# Volume-side auto-rename is not atomic; the streaming write then clobbers a racing file

**Severity:** medium
**Lens:** A — Data safety
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/volume_conflict.rs:462-493` (`find_unique_volume_name`) and `:378-385`; the chosen path is consumed by `volume_strategy.rs:97-99` → `LocalPosixVolume` write (`local_posix.rs:541`, `std::fs::File::create`).

## What
For a Rename-resolution conflict on the volume copy/move path, `find_unique_volume_name` picks `name (N)` using only a non-atomic `dest_volume.exists()` probe and returns the path. The streaming writer then opens that path with `File::create`, which truncates whatever is there. Between the `exists()` check and the `File::create`, a concurrent writer (a second Cmdr op, a cloud-sync agent, a backup tool) can create a real file at `name (N)`, and the copy silently clobbers it. This is exactly the TOCTOU that the local-FS `helpers::find_unique_name` was fixed for — it now reserves the name with an `O_CREAT|O_EXCL` placeholder (documented in transfer CLAUDE.md § "Cross-type Rename") — but the volume variant never got the same treatment.

## Why it matters
A user copying files from a phone/NAS into a directory that another process is also writing can have that other process's freshly-created `foo (1).txt` overwritten by Cmdr's auto-renamed copy, with no conflict prompt — silent third-party data loss. Narrower than the local path (only triggers on volume-routed copies with an active concurrent writer landing on the exact auto-chosen name), but the window and the clobber mechanism are both real, and the local path was considered serious enough to fix.

## Evidence
```rust
// volume_conflict.rs ~462
async fn find_unique_volume_name(dest_volume: &Arc<dyn Volume>, path: &Path) -> PathBuf {
    // ...
    loop {
        let new_path = parent.join(new_name);
        if !dest_volume.exists(&new_path).await {
            return new_path;        // ← no reservation; TOCTOU window opens here
        }
        counter += 1;
        // ...
    }
}
// later, write_from_stream: std::fs::File::create(&dest)  // truncates if it now exists
```

## Suggested fix
Close the window the same way the local path does. For local-FS-backed destination volumes, reserve the chosen name with an `O_CREAT|O_EXCL` placeholder (or add a `Volume::create_exclusive` / `reserve_name` capability) and `continue` on `AlreadyExists`, then let the streaming write land on the placeholder. For backends without exclusive-create semantics (MTP/SMB), at minimum re-check existence immediately before the write and loop, and document the residual narrow window. The cleanest cross-backend shape is a `Volume`-level "create-exclusive then stream into it" primitive used by the Rename branch.

## Notes
The local-FS fix is documented in transfer CLAUDE.md (the `find_unique_name` placeholder + `needs_safe_overwrite` dance, pinned by `type_mismatch_rename_tests.rs`). The volume sibling lacking the same guard is the gap. Lower severity than the cross-FS-move-skip finding because it requires a concurrent external writer hitting the exact chosen name.
