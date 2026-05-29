# `commands/rename.rs` carries the rename subsystem's business logic instead of forwarding

**Severity:** medium
**Lens:** D — IPC boundary
**Confidence:** high

## Location

- `apps/desktop/src-tauri/src/commands/rename.rs:200-425` (everything below the `#[tauri::command]` functions)
  - `check_rename_permission_sync` (200-218)
  - `check_dir_writable` (222-242)
  - `check_macos_flags` (245-279)
  - `check_rename_validity_impl` (281-340)
  - `check_sibling_conflict` (344-402)
  - `check_sibling_conflict_via_volume` (406-425)
  - `notify_rename_in_listing` (157-199)
  - DTOs `RenameValidityResult`, `ConflictFileInfo` (49-75)

## What

`commands/CLAUDE.md` says:

> One commands file per domain, with no business logic in commands. … Mixing business logic here makes it untestable
> (Tauri commands need a running app to invoke). Keeping commands as thin pass-throughs means the real logic lives in
> subsystem modules that can be unit-tested independently.

…and:

> No business logic here. If you find yourself adding branching or data transformation, move it to the relevant
> subsystem module.

`rename.rs` violates this. Half the file (lines 156-425, ~270 LOC) is the actual rename business logic: writability
checks via `access(W_OK)`, macOS immutable/SIP flag checks via `lstat` + `UF_IMMUTABLE`/`SF_IMMUTABLE` bitmasking,
filename validation orchestration, sibling-conflict detection with inode comparison for case-only renames on
case-insensitive APFS, and listing-cache invalidation. The `#[tauri::command]` shells at the top of the file just
delegate to these in-file helpers.

The other comparable subsystem, `write_operations`, follows the docs' rule — the commands in `commands/file_system/
write_ops.rs` forward to `crate::file_system::write_operations::*`. `rename.rs` is the outlier.

## Why it matters

1. **Loses unit-test independence.** The tests at the bottom of `rename.rs` (438-637) work, but they exercise the
   logic _through_ the Tauri command surface (`check_rename_permission`, `rename_file`). That's coupling test surface
   to the IPC boundary, which is the exact failure mode the doc rule was written to prevent. Moving the helpers into a
   `crate::file_system::rename` module would let those tests target the helpers directly without `tokio::time::
   timeout` round-trips and without `String` path argument round-trips.
2. **Hides cross-subsystem cohesion.** `notify_rename_in_listing` knows about `volume_manager`, `MutationEvent`, and
   the per-volume mutation event API. That belongs next to the volume + listing-cache code, not in the IPC layer.
   Future changes to `MutationEvent` won't grep up `commands/rename.rs` as a place to update.
3. **Makes the rule harder to defend.** Once the largest single-file exception lives in `commands/`, every future
   "just a bit of glue" addition becomes easier to justify. The principle erodes by precedent.
4. **DTO shape leakage.** `RenameValidityResult` and `ConflictFileInfo` are defined here. They cross the IPC boundary
   (typed via specta), so their natural home is also the subsystem module, alongside the function that produces
   them — not next to the Tauri command shell.

## Evidence

`rename.rs:200-218` (business logic in the commands file):

```rust
fn check_rename_permission_sync(path: &Path) -> Result<(), String> {
    if std::fs::symlink_metadata(path).is_err() { … }
    let parent = path.parent().ok_or_else(|| "Can't rename the root directory".to_string())?;
    check_dir_writable(parent)?;
    #[cfg(target_os = "macos")]
    check_macos_flags(path)?;
    Ok(())
}
```

`rename.rs:245-279` (libc bitmask logic in the commands file):

```rust
#[cfg(target_os = "macos")]
fn check_macos_flags(path: &Path) -> Result<(), String> {
    …
    const UF_IMMUTABLE: u32 = 0x00000002;
    const SF_IMMUTABLE: u32 = 0x00020000;
    if (stat.st_flags & UF_IMMUTABLE) != 0 { … }
    if (stat.st_flags & SF_IMMUTABLE) != 0 { … }
}
```

Compare `rename.rs::move_to_trash` (line 17), which IS a thin pass-through:

```rust
pub async fn move_to_trash(path: String) -> Result<(), IpcError> {
    let expanded = expand_tilde(&path);
    let path_buf = PathBuf::from(&expanded);
    tokio::time::timeout(Duration::from_secs(15),
        tokio::task::spawn_blocking(move || move_to_trash_sync(&path_buf)))
        .await … .map_err(IpcError::from_err)
}
```

…because `move_to_trash_sync` lives in `crate::file_system::write_operations::trash`. Apply the same pattern to the
rename helpers.

## Suggested fix

1. Create `crate::file_system::rename` (or `crate::rename`, whichever fits the existing module map best — there
   isn't one yet because the logic has been hiding here).
2. Move `check_rename_permission_sync`, `check_dir_writable`, `check_macos_flags`, `check_rename_validity_impl`,
   `check_sibling_conflict`, `check_sibling_conflict_via_volume`, `notify_rename_in_listing`, plus the DTOs
   `RenameValidityResult` and `ConflictFileInfo`, into that module.
3. Move the existing test block (lines 442-636) into the new module.
4. Leave the `#[tauri::command]` shells in `commands/rename.rs`, each forwarding to the new module — same shape as
   `move_to_trash` already does.

The diff should make `rename.rs` look structurally like `commands/eject.rs` does today (pure dispatch).

## Notes

This is medium because the current shape works correctly and has tests. It hurts long-term maintainability and the
documented architecture, not the runtime behaviour. Fixing it alongside the next rename-related feature work would
land it cheaply.
