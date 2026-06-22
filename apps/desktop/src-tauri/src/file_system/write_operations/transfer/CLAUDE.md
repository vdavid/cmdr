# Transfer (copy + move)

Copy and move, local-FS and volume-aware (Local ↔ MTP ↔ SMB). All flows go through the shared driver
(`transfer_driver.rs`) and emit progress via `OperationEventSink`.

Shared `WriteOperationState`, `OperationIntent`, cancel/rollback, ETA, and settle contract:
[`../CLAUDE.md`](../CLAUDE.md). Delete: [`../delete/CLAUDE.md`](../delete/CLAUDE.md). Frontend:
[`src/lib/file-operations/transfer/CLAUDE.md`](../../../../../src/lib/file-operations/transfer/CLAUDE.md).

## Module map

- Local-FS: `copy.rs` (`CopyTransaction` rollback), `move_op.rs` (same-fs rename / cross-fs staging), `copy_strategy.rs`
  + `{macos,linux,chunked}_copy.rs` (per-file strategy + backends).
- Shared driver: `transfer_driver.rs` (`drive_transfer_serial_sync` + `_async`, per-file progress builders).
- Volume: `volume_{copy,move,preflight,rename_merge,conflict,strategy}.rs`.

## Must-knows (data-safety invariants)

- **The merge invariant**: a merge never deletes or overwrites a dest file the source doesn't shadow — every policy and
  backend, including cancel/rollback mid-merge (`volume_merge_tests.rs`).
- **Dir-vs-dir is NEVER a conflict**: `resolve_volume_conflict` short-circuits to merge before any policy lookup or
  `write-conflict` emit. Even Stop/Skip/Rename merge the folder; only files prompt. Cross-type keeps the full machinery.
- **Overwrite means merge for dirs, replace for files**: enforced at the `apply_volume_conflict_resolution` call site
  (skips the delete for dirs), NOT by `Volume::delete`'s contract — else a recursive-delete backend silently flips
  merge → wholesale replace. Pinned by `dir_overwrite_must_merge_not_replace_even_with_recursive_delete`.
- **Cross-volume file→file Overwrite is a safe-replace, NOT delete-then-write**: stream into a `.cmdr-tmp-<uuid>`
  sibling, then `finalize_safe_replace` (delete orig, rename temp in). That post-write temp is committed data, NOT a
  cleanable partial; cleanup must not touch it after `copy_single_path` returns `Ok`. Cross-type stays delete-first.
- **Cross-volume cleanup/rollback for a DIRECTORY source is per-FILE, never the dir root** (a merge holds pre-existing
  dest files; recursive root delete is silent data loss). The `CreatedPaths` ledger flows out of the `Err` arms,
  `last_dest_path` is CLEARED for a dir source, and a dir root must never enter `in_flight_partials`. Pinned
  (`rollback_tests`); DETAILS.
- **Cross-FS move source-delete preserves Skipped sources** (don't delete a kept source) and runs AFTER
  `flush_created_destinations` (never delete the source before the dest is durable).
- **Empty directories land via `copy.rs::create_scanned_dirs_at_destination`** (the per-file loop only creates dirs as
  file parents). A dest already holding anything is left untouched.
- **Same-volume move is a rename-merge with top-level hints only** (`top_level_move_hints`, `bytes_total = 0`), never a
  subtree walk. Cross-volume move runs the full preflight.
- **Cross-type Rename reserves the name with a 0-byte O_EXCL placeholder** (`find_unique_name` /
  `find_unique_volume_name`), returns `needs_safe_overwrite: true` so the copy lands ON it (TOCTOU guard).
- **MTP can't signal collisions via `create_directory`** (allows duplicate-name siblings); the merge walker pre-checks
  `exists()`, gated by `Volume::create_directory_errors_on_existing_dir()`.
- **The conflict-dispatch mutex serializes the human across concurrent/nested merges**; released on every exit, never
  held across the write (`../CLAUDE.md`).
- **Volume copy/move must skip `write-error` on `Cancelled`** (inner already emitted `write-cancelled`); cancellation
  propagates as typed `VolumeError::Cancelled`, not `IoError` (a Cancelled-shaped `copy_error` reclassifies to `None`).
- **Overwrite is NOT reversible**: rollback un-creates new files but can't restore an Overwrite-replaced original (no
  unbounded backup — don't reintroduce that footgun).
- **`stream_pipe_file` retries once on `VolumeError::StaleDestinationHandle`** (re-opens source, re-runs
  `write_from_stream`): the only layer that can retry an MTP stale-handle rejection (backend stream is single-use), so
  don't drop the loop. Why: [`mtp/connection/DETAILS.md`](../../../mtp/connection/DETAILS.md).
- **Cross-volume copy parks/yields between chunks** via `volume_strategy.rs`'s `CheckpointStream` (sync `on_progress`
  can't `.await`; pause/yield in the wrapper, cancel in the backend's `on_progress`). An MTP source
  (`pause_releases_read_stream()`, via `mtp-rs` `download_stream_from_offset` ≥ 0.21.0) RELEASES its session and reopens
  at `bytes_yielded` on TWO triggers, same machinery: (1) **user pause**, and (2) **auto-yield to foreground** — on
  `foreground_pending` the running copy releases + `background_yield_point`s + reopens (gated by
  `supports_foreground_yield()`), so a transfer is a yielding gate user like the scan. Auto-yield keeps the op
  **Running** (device yield, not user pause — don't flip `OperationIntent`); a debounce + min-progress floor (named,
  cancel-aware constants) prevent reopen thrash and starvation. DETAILS §§ "Pause … chunks", "Foreground auto-yield".

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here.
