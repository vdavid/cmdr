# Cross-volume copy Cancel/Rollback while a directory MERGE is still mid-copy recursively deletes the dest directory root, destroying pre-existing dest-only files

**Severity:** high **Lens:** A — Data safety **Confidence:** high (red→green test) **Status:** FIXED

## Relationship to HIGH-A

This is the sibling cell the round-4 SUMMARY flagged under "Areas worth a second pass": the HIGH-A fix (`0efb0e12`) made
the **completed** merged-directory Rollback safe by threading a per-file `CreatedPaths` ledger, but only the success arm
consumed that ledger. A directory source interrupted **mid-stream** (cancel/rollback/error while still copying its
children) took the `Err` arm, which **discarded** the ledger and fell back to recording the top-level dest directory
ROOT as the partial — then recursively deleted it. Same data-loss shape as HIGH-A, reachable under both Cancel and
Rollback, on both the serial and concurrent copy paths.

## What

In `copy_volumes_with_progress` (`transfer/volume_copy.rs`), a directory source's transfer sets `last_dest_cell` /
`in_flight_partials` to the dest directory ROOT before calling `copy_single_path`. On success the per-file ledger is
recorded and the root slot cleared. But on **error/cancel mid-stream**:

- **Serial path:** the `Err(e)` arm left `last_dest_cell = <dest dir root>` and dropped the `created` ledger. Post-loop,
  `last_dest_path` = the dir root, and both the Stopped arm (partial cleanup) and the RollingBack arm
  (`copied_paths.push(last_dest_path)`) ran `delete_volume_path_recursive` on it.
- **Concurrent path:** the task's `Err` returned `(dest_root, e, cleanup_temp=true)`, discarding `created_files` /
  `created_dirs`; the result handler pushed the dir root for recursive cleanup.

On a merge ("Overwrite means merge for dirs"), that root holds pre-existing dest-only files the operation never wrote.
Recursively deleting it is silent loss of untouched user data — under both the keep-partials Cancel and the
advertised-as-safe Rollback.

## Repro (now regression tests)

Pre-populate a dest dir `/album` with a unique `sentinel.txt`, start a cross-volume directory copy of `/album` (with new
files) merging into it under Overwrite, trip Cancel (or Rollback) the moment the first byte streams (mid-merge), then
assert the sentinel survives. Pre-fix the sentinel was destroyed; the assertion went red:

```
thread '...cancel_mid_merge_stream_preserves_preexisting_dest_file' panicked:
  cancel mid-merge-stream wrongly deleted a pre-existing dest-only file
thread '...rollback_mid_merge_stream_preserves_preexisting_dest_file' panicked:
  rollback mid-merge-stream wrongly deleted a pre-existing dest-only file
thread '...cancel_mid_merge_stream_concurrent_preserves_preexisting_dest_file' panicked:
  concurrent cancel mid-merge-stream wrongly deleted a pre-existing dest-only file
```

After the fix all three pass.

## Fix

Thread the per-file `CreatedPaths` ledger out of the `Err`/cancel arms, mirroring the success arm:

- **Serial `Err` arm:** for a directory source, clear `last_dest_cell` to `None` (so the post-loop never recursively
  deletes the dir root) and record `created.files` into `copied_paths` + `created.dirs` into `created_dirs`. A FILE
  source keeps `last_dest_cell` = its single dest/temp (a genuine half-written partial, safe to remove).
- **Concurrent path:** replaced the `(PathBuf, VolumeError, bool)` Err tuple with a `CopyTaskFailure` struct carrying
  `source_is_dir` + `created_files` + `created_dirs`. The result handler records the per-file ledger for a directory
  source instead of pushing the dir root.

Rollback/cleanup then operates per-file; created dirs are pruned empty-only (deepest-first), so a merged dir holding a
sentinel survives. Same mechanism and invariant as the HIGH-A fix, now also covering the interrupted path. The fix is
~the size of the HIGH-A fix (one struct + two arm edits), so it was applied here rather than left.

## Pinned by

`volume_copy_tests.rs::{cancel_mid_merge_stream_preserves_preexisting_dest_file, rollback_mid_merge_stream_preserves_preexisting_dest_file, cancel_mid_merge_stream_concurrent_preserves_preexisting_dest_file, rollback_after_rename_keeps_preexisting_dest_file}`.
Decision recorded in `transfer/CLAUDE.md` (extends the HIGH-A Decision).

## Notes on the MOVE path

The cross-volume **move** path (`volume_move.rs`) has no rollback and does NOT clean up the dest on cancel/error (it's
copy+delete-source per file, keep-partials), so it has no analogous merged-dir-destruction bug. Its move-invariant
("source survives iff not moved") is already pinned by the `volume_move_tests.rs` Skip/Overwrite/Rename/bulk-skip cases.
