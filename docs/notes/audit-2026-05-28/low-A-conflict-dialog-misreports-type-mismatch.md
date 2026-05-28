# Conflict dialog misreports source/dest type on file-vs-dir collisions

**Severity:** low **Lens:** A — Data safety **Confidence:** medium

## Location

`apps/desktop/src-tauri/src/file_system/write_operations/helpers.rs:338-443` (`resolve_conflict`) also
`helpers.rs:664-701` (`create_conflict_info`)

## What

The Stop-mode conflict resolution emits a `WriteConflictEvent` with no `is_directory` field on either side. The frontend
dialog renders a generic "file already exists at destination" prompt with `source_size` and `destination_size`. When the
actual collision is a type mismatch (source is a file, dest is a directory with the same name, or vice versa), the
prompt is misleading — both `len()` calls return values, but the destination "size" of a directory is
filesystem-specific (often the dir's entry-table size, not its content size) and meaningless to the user.

`create_conflict_info` (lines 664-701) does carry `is_directory: source_metadata.is_dir()` but it reflects the source
only; the dest's type isn't surfaced. The Stop path doesn't go through `create_conflict_info` at all — it constructs the
event inline.

## Why it matters

The user picks "Overwrite" on what they think is a file-over-file conflict and ends up replacing a directory tree
(`safe_overwrite_file` at line 622-627 handles this with `remove_dir_all`, so it works mechanically, but the user wasn't
warned). Losing a directory tree they didn't realize they were replacing is the kind of "wait what" that ends with bad
reviews and lost trust.

The reverse — source is a directory, dest is a file — routes through `copy_single_item`'s parent-directory walk
(`copy.rs:531-639`) rather than the conflict-resolution emit, so this finding only covers the file→dir clobber and the
symmetric file→dir-where-the-source-is-actually-a-file-being-mistaken case.

## Evidence

```rust
events.emit_conflict(WriteConflictEvent {
    operation_id: operation_id.to_string(),
    source_path: source.display().to_string(),
    destination_path: dest_path.display().to_string(),
    source_size,
    destination_size,
    source_modified,
    destination_modified,
    destination_is_newer,
    size_difference,
});
```

No `is_directory` indicator on either side.

## Suggested fix

Add `source_is_directory` and `destination_is_directory` to `WriteConflictEvent`. Populate them from
`source_meta.as_ref().map(|m| m.is_dir())` and `dest_meta.as_ref().map(|m| m.is_dir())`. The frontend dialog renders a
different layout for type-mismatch collisions ("Replace the _folder_ at destination with this _file_? This will delete N
items inside the folder.").

Two-line change at the emit site, type-shape addition on the FE side. The existing `ConflictInfo` struct's
`is_directory: bool` should probably split into source/dest too for `dry_run_scan` consistency.

## Notes

This isn't a data-loss bug today (the safe-overwrite path correctly removes a directory backup via `remove_dir_all` at
`helpers.rs:622-627`), it's a UX-driven data-loss risk: the user agrees to "Overwrite" without knowing they're agreeing
to drop a whole tree. The principle from `AGENTS.md` § "Protect the user's data — Use safe overwrite patterns" extends
to "tell the user what they're overwriting."
