# Cross-filesystem move deletes the source of a Skipped file → permanent data loss

**Severity:** critical
**Lens:** A — Data safety
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/move_op.rs:577-665` (Phase 3 skip handling) and `apps/desktop/src-tauri/src/file_system/write_operations/transfer/move_op.rs:706-743` (`delete_sources_after_move`).

## What
On a cross-filesystem move, `move_with_staging` copies every source into a `.cmdr-staging-<uuid>` dir (Phase 2), renames staging→final resolving conflicts against the real destination (Phase 3), then deletes the original sources (Phase 4). When a Phase-3 conflict resolves to **Skip**, the staged copy is removed and the loop `continue`s, so the file never lands at the destination. But Phase 4 (`delete_sources_after_move`) iterates the full `sources` list unconditionally and unlinks each original. The skipped file existed only at the source, and Phase 4 deletes it. The same-filesystem path (`move_with_rename`) is safe because Skip just `continue`s without touching the source; this bug is specific to the staging path's "delete all sources" final phase.

## Why it matters
A user moves `report.pdf` from an external SSD to the internal disk (different filesystems) where a `report.pdf` already exists. The conflict dialog appears; the user clicks **Skip** (or has "Skip all" latched, or picked the Skip radio upfront in TransferDialog). Result: the pre-existing destination file is untouched, the staged copy is discarded, and the user's original `report.pdf` on the SSD is **deleted**. The only surviving copy is the unrelated older destination file. The same loss applies to Skipped children inside a cross-FS directory merge: `merge_move_directory` leaves the skipped child's *staged* copy out, but Phase 4's `fs::remove_dir_all(source)` deletes the whole original source directory, including the skipped children's originals.

Skip is fully reachable: it's a conflict-policy radio (`TransferDialog.svelte:577`), a per-conflict button (`TransferProgressDialog.svelte:1111`), and a bulk "Skip all" mode.

## Evidence
```rust
// Phase 3, on Skip (move_op.rs:627-636):
None => {
    // Skip - remove from staging
    if staged_path.is_dir() {
        let _ = fs::remove_dir_all(&staged_path);
    } else {
        let _ = fs::remove_file(&staged_path);
    }
    files_skipped += 1;
    continue;                       // file never lands at final_path
}
// ...
// Phase 4 (move_op.rs:661-662):
delete_sources_after_move(events, operation_id, state, sources, files_done)?;

// delete_sources_after_move (move_op.rs:713-733):
for source in sources {            // ← ALL sources, incl. the skipped one
    // ...
    if fs::symlink_metadata(source).is_ok() {
        if source.is_dir() {
            fs::remove_dir_all(source).with_path(source)?;
        } else {
            fs::remove_file(source).with_path(source)?;   // ← deletes the never-moved file
        }
    }
}
```
Phase 2 stages everything regardless of destination conflicts (conflicts there are resolved only against the fresh, empty staging dir), so every source — including the ones that will be Skipped in Phase 3 — is present in `sources` when Phase 4 runs.

## Suggested fix
Phase 4 must only delete sources (and source children) that actually landed at the destination. Track skip decisions in Phase 3 — collect a `HashSet<PathBuf>` of source paths (and, for the directory-merge case, per-child paths) that were skipped, or invert and collect the set that successfully reached `final_path` — and have `delete_sources_after_move` consult it before unlinking each source. The directory-merge case needs per-child bookkeeping, mirroring how the same-FS `move_with_rename` / `merge_move_directory` already leave skipped children in place. Add a regression test driving `move_with_staging` with `ConflictResolution::Skip` against a destination holding a same-named file, asserting the source still exists afterward, plus a directory-merge variant with one skipped child.

## Notes
This contradicts AGENTS.md principle #4 ("Protect the user's data") and the transfer CLAUDE.md's stated cross-FS move contract ("delete sources **only after successful copy+rename**" — the comment at `move_op.rs:661` says exactly this, but the code deletes skipped-and-therefore-not-moved sources too). It is NOT the documented "Skip-All on volume copy/move drops the entire subtree" gotcha — that's about volume-routed (MTP/SMB) copies dropping a subtree (a UX annoyance, no source deletion). This is the local cross-filesystem move path and it permanently deletes the user's only copy. Highest-priority fix before launch.
