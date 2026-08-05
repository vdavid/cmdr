# Transfer (copy + move)

Copy and move, local-FS and volume-aware (Local ↔ MTP ↔ SMB), via `transfer_driver/` and `OperationEventSink`. Op
state, intent, cancel/rollback, ETA, the conflict mutex, settle: `../CLAUDE.md`. Frontend:
`apps/desktop/src/lib/file-operations/transfer/CLAUDE.md`.

## Module map

- Local-FS: `copy/` (+ `CopyTransaction` rollback), `move_op.rs`, `copy_strategy.rs` + `{macos,linux,chunked}_copy.rs`.
- Volume: `volume_{copy,move,preflight,rename_merge,conflict,strategy}.rs`, where `volume_copy.rs` runs the phases and
  drives ONE of `volume_copy_{concurrent,serial}.rs`; plus `checkpoint_stream.rs`, `staged_write.rs`, `retry.rs`,
  `transfer_probe.rs`.

## Merge and conflicts

- **The merge invariant**: a merge never deletes or overwrites a dest file the source doesn't shadow — every policy,
  backend, and cancel/rollback/retry mid-merge (`volume_merge_tests.rs`).
- **Dir-vs-dir is NEVER a conflict**; only files prompt. **Overwrite means merge for dirs, replace for files**, enforced
  at the `apply_volume_conflict_resolution` call site, ❌ not by `Volume::delete`'s contract. ❌ Overwrite is NOT
  reversible: no unbounded backup.
- **A MOVE's source sweep spares every child the merge skipped**
  (`delete_volume_path_recursive_preserving`, fed by `CreatedPaths::skipped_source_paths`). A skipped child never
  reached the dest, so its source is the ONLY copy; ❌ never sweep a merged source folder unconditionally. The
  conditional policies reduce to Skip per file, so "Overwrite all smaller / older" hits this on ordinary use.
- **❌ Never fabricate a destination size for the conflict dialog.** `resolve_volume_conflict` takes the caller's hint
  or the stat it already does for the mtime, and reports `None` ("unknown") otherwise. A fabricated `0` both lies in the
  dialog and makes every dest look smaller, silently turning "Overwrite all smaller" into an unconditional overwrite.
- **Skip the top-level dest pre-check ONLY for a dest dir THIS op created** (`DirectoryCreation::Created`), ❌ never one
  that merely looks empty. A merge answers it from `dest_name_index.rs`, a deliberate snapshot; whatever that can't
  disprove falls through to the real probe. `DETAILS.md` § "Answering the pre-check from one listing".

## Staging and durability

- **A cross-volume file write stages on `.cmdr-tmp-<uuid>`**, taking its final name only after its last byte. Ask
  `resolve_staging`; ❌ single-shot-ness earns an exemption, NEVER smallness.
- **Cleanup and rollback for a DIRECTORY source are per-FILE, never the dir root**: a merge holds pre-existing dest
  files, so a recursive root delete is silent data loss.
- **Cross-FS move deletes sources AFTER `flush_created_destinations`, preserving Skipped ones.** Same-volume move is a
  rename-merge with top-level hints only (`bytes_total = 0`), never a subtree walk.

## Streaming, cancel, and diagnosis

- **Cross-volume copy parks and yields between chunks** (`CheckpointStream`): park in place, ❌ no release/reopen. TWO
  opt-ins, ❌ don't merge them: SOURCE read-yield (MTP + SMB) is unbounded, DESTINATION write-yield (SMB only) is
  hard-capped.
- **Concurrency and cancel are bounded on purpose**: a LOCAL `max_concurrent_ops` must ❌ NOT bound a REMOTE peer
  (`transfer_concurrency`, ❌ never a `min()`; a remote cap always binds, which keeps MTP serial), and the concurrent
  driver watches cancel/rollback ON ITS AWAIT, draining under `CANCEL_DRAIN_DEADLINE` before abandoning the rest.
- **Every phase announces itself to `transfer_probe.rs`, on BOTH drivers**: ❌ no `.await` on a transfer path without a
  phase, and ❌ never derive a stall from FE timing (a wedge emits nothing).
- **A failure carries the path it happened ON** (`PathedVolumeError`): the walker descends a whole subtree per
  `copy_single_path` and per `delete_volume_path_recursive`, so ❌ never re-label an error with the top-level source,
  and ❌ never `.at()` a frame above the one that knows the item. A directory sweep reports the first child that
  refused, ❌ never the parent's own `ENOTEMPTY` (the symptom, named after the folder the user picked). The concurrent
  driver keeps `failed_path` (dest partial to clean) separate from `reported_path` (the source item to name) — ❌ don't
  collapse them. `DETAILS.md` § "Naming the item that failed".
- **Retry is per-FILE, ONLY inside `stream_pipe_file`** (`retry.rs`): ❌ never higher (conflicts, the ledger, the
  journal, and the milestone sit above it and happen once), ❌ never on a `Cancelled`. **The stall watchdog is GATED and
  inert**: `connection_liveness() == Dead` AND `STALL_ABORT_AFTER`, and nothing answers `Dead` today, so ❌ never
  collapse the AND.
- **Progress is HIGH-WATER per file** (concurrent `fetch_max`, ❌ not `swap`; serial `leaf_high_water`), so a retry
  neither double-counts nor reverses the bar.

Semantics, flows, decisions, the staging/retry/auto-yield/stall contracts: `DETAILS.md`. Read it before any non-trivial
work here: editing, planning, reorganizing, or advising.
