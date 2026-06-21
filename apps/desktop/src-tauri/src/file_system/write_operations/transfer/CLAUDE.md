# Transfer (copy + move)

Copy and move, local-FS and volume-aware (Local ↔ MTP ↔ SMB). All flows go through the shared driver
(`transfer_driver.rs`) and emit progress via `OperationEventSink`.

Shared `WriteOperationState`, `OperationIntent`, the cancel/rollback contract, ETA estimator, and settle contract:
[`../CLAUDE.md`](../CLAUDE.md). Parallel delete doc: [`../delete/CLAUDE.md`](../delete/CLAUDE.md). Frontend:
[`src/lib/file-operations/transfer/CLAUDE.md`](../../../../../src/lib/file-operations/transfer/CLAUDE.md).

## Module map

- Local-FS: `copy.rs` (`CopyTransaction` rollback), `move_op.rs` (same-fs rename / cross-fs staging), `copy_strategy.rs`
  + `macos_copy.rs` / `linux_copy.rs` / `chunked_copy.rs` (per-file strategy + backends).
- Shared driver: `transfer_driver.rs` (`drive_transfer_serial_sync` + `_async`, per-file progress builders).
- Volume: `volume_copy.rs`, `volume_move.rs`, `volume_preflight.rs`, `volume_rename_merge.rs`, `volume_conflict.rs`,
  `volume_strategy.rs`.

## Must-knows (data-safety invariants)

- **The merge invariant**: a merge never deletes or overwrites a dest file the source doesn't shadow — every policy and
  backend, including cancel/rollback mid-merge (pinned by `volume_merge_tests.rs`).
- **Dir-vs-dir is NEVER a conflict**: `resolve_volume_conflict` short-circuits to merge before any policy lookup or
  `write-conflict` emit. Even Stop/Skip/Rename merge the folder; only files prompt. Cross-type (file↔folder) keeps the
  full conflict machinery.
- **Overwrite means merge for dirs, replace for files**: enforced at the `apply_volume_conflict_resolution` call site
  (stats dest, skips the delete for dirs), NOT by `Volume::delete`'s contract — a backend with recursive delete would
  otherwise silently flip merge → wholesale replace. Pinned by
  `dir_overwrite_must_merge_not_replace_even_with_recursive_delete`.
- **Cross-volume file→file Overwrite is a safe-replace, NOT delete-then-write**: stream into a `.cmdr-tmp-<uuid>`
  sibling, then `finalize_safe_replace` (delete orig, rename temp in). That post-write temp is committed data, NOT a
  cleanable partial; partial-cleanup must not touch it after `copy_single_path` returns `Ok`. Cross-type Overwrite
  (file↔folder) stays delete-first (no temp+rename atomicity for a type change).
- **Cross-volume cleanup/rollback for a DIRECTORY source is per-FILE, never the dir root** (a merge holds pre-existing
  dest files; recursively deleting the root is silent data loss). The `CreatedPaths` ledger flows out of the mid-stream
  `Err` arms, `last_dest_path` is CLEARED for a dir source, and a dir root must never enter `in_flight_partials`
  (cleanup `delete_volume_path_recursive`s those). Pinned by
  `rollback_tests::cancel_mid_merge_stream_concurrent_preserves_preexisting_dest_file`.
- **Cross-FS move source-delete preserves Skipped sources** (don't delete a source the user kept) and runs AFTER
  `flush_created_destinations` (never delete the source before the dest is durable). Cross-volume move finalizes before
  deleting the source.
- **Empty directories land via `copy.rs::create_scanned_dirs_at_destination`** (the per-file loop only creates dirs as
  file parents). A dest already holding anything is left untouched.
- **Same-volume move is a rename-merge with top-level hints only** (`top_level_move_hints`, `bytes_total = 0`), never a
  subtree walk. Cross-volume move runs the full `scan_volume_sources` preflight.
- **Cross-type Rename reserves the name with a 0-byte O_EXCL placeholder** (`find_unique_name` /
  `find_unique_volume_name`) and returns `needs_safe_overwrite: true` so the copy lands ON it (TOCTOU guard).
- **MTP can't signal collisions via `create_directory`** (it allows duplicate-name siblings); the merge walker
  pre-checks `exists()`, gated by `Volume::create_directory_errors_on_existing_dir()`.
- **The conflict-dispatch mutex serializes the human across concurrent/nested merges**; released on every exit, never
  held across the write. See `../CLAUDE.md`.
- **Volume copy/move must skip `write-error` on `Cancelled`** (inner already emitted `write-cancelled`); cancellation
  propagates as typed `VolumeError::Cancelled`, not `IoError`. A Cancelled-shaped `copy_error` is reclassified to `None`
  so the cancel emit still fires.
- **Overwrite is NOT reversible**: rollback un-creates new files but can't restore an Overwrite-replaced original (no
  unbounded backup). Don't reintroduce the unbounded-backup footgun.
- **`stream_pipe_file` retries once on `VolumeError::StaleDestinationHandle`** (re-opens source, re-runs
  `write_from_stream`): the only layer that can retry an MTP stale-handle rejection (backend stream is single-use), so
  don't drop the loop. Why: [`src/mtp/connection/DETAILS.md`](../../../mtp/connection/DETAILS.md) § "Stale parent handle".
- **Cross-volume copy parks/yields between chunks** via `volume_strategy.rs`'s `CheckpointStream` (sync `on_progress`
  can't `.await`). Keep pause/yield in the wrapper, cancel in the backend's `on_progress`. DETAILS § "Pause reaches
  between chunks".

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
