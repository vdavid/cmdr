# Transfer (copy + move)

Copy and move, local-FS and volume-aware (Local ↔ MTP ↔ SMB), all through the shared driver (`transfer_driver/`),
emitting via `OperationEventSink`. State, intent, cancel/rollback, ETA, the conflict mutex, and the settle contract:
`../CLAUDE.md`. Frontend: `apps/desktop/src/lib/file-operations/transfer/CLAUDE.md`.

## Module map

- Local-FS: `copy/` (+ `CopyTransaction` rollback), `move_op.rs`, `copy_strategy.rs` + `{macos,linux,chunked}_copy.rs`.
- Volume: `volume_{copy,move,preflight,rename_merge,conflict,strategy}.rs`, `checkpoint_stream.rs`, `staged_write.rs`,
  `transfer_probe.rs` (in-flight table + stall watchdog).

## Merge and conflicts

- **The merge invariant**: a merge never deletes or overwrites a dest file the source doesn't shadow — every policy,
  every backend, cancel/rollback mid-merge included (`volume_merge_tests.rs`).
- **Dir-vs-dir is NEVER a conflict**: `resolve_volume_conflict` short-circuits to merge before any policy lookup or
  emit. Even Stop/Skip/Rename merge the folder; only files prompt.
- **Overwrite means merge for dirs, replace for files**, enforced at the `apply_volume_conflict_resolution` call site,
  NOT by `Volume::delete`'s contract — else a recursive-delete backend flips merge → wholesale replace.
- **Overwrite is NOT reversible**: rollback can't restore a replaced original. ❌ No unbounded backup.
- **Cross-type Rename reserves the name with a 0-byte `O_EXCL` placeholder** (TOCTOU guard), returning
  `needs_safe_overwrite: true`.

## Staging and durability

- **A cross-volume file write stages on `.cmdr-tmp-<uuid>` and takes its final name only after its last byte**: a
  force-quit must never leave a truncated file at a real name. Exemptions: `AlreadyStaged` and `SingleShot` — ❌
  single-shot-ness buys that, NEVER smallness; ask via `resolve_staging`. A temp sits in `in_flight_temps` only while
  partial, and carries `state.liveness_token()` so it stays hidden.
- **Cross-volume file→file Overwrite is a safe-replace** (sibling temp + `finalize_safe_replace`), not
  delete-then-write; that temp is committed data, not a cleanable partial. Cross-type stays delete-first.
- **Cleanup/rollback for a DIRECTORY source is per-FILE, never the dir root**: a merge holds pre-existing dest files, so
  a recursive root delete is silent data loss. `last_dest_path` is cleared for a dir source; a dir root never enters
  `in_flight_partials`.
- **Cross-FS move source-delete preserves Skipped sources and runs AFTER `flush_created_destinations`.**
- **Same-volume move is a rename-merge with top-level hints only** (`bytes_total = 0`), never a subtree walk.

## Streaming, cancel, and diagnosis

- **Cross-volume copy parks/yields between chunks** (`CheckpointStream`). Pause and yield both mean **don't start the
  next window** (park in place, NO release/reopen); auto-yield keeps the op **Running**. TWO opt-ins, ❌ don't merge:
  SOURCE read-yield (MTP + SMB) parks UNBOUNDED; DESTINATION write-yield (SMB) is HARD-CAPPED, holding a write handle
  the server reaps. ❌ MTP never opts in there.
- **The concurrent driver observes cancel/rollback ON ITS AWAIT, not only in the spawn loop**: parked on
  `in_flight.next()` it never reaches `is_cancelled`. It races that await against `state.backend_cancel`, drains under
  `CANCEL_DRAIN_DEADLINE`, then ABANDONS the rest. ❌ Never an unbounded wait.
- **Every phase must announce itself** (`transfer_probe.rs`): a parked transfer is invisible to a stack sample. ❌ No
  `.await` on a transfer path without a phase around it.
- **The probe is ALSO the UI's stall signal** (a `TransferActivity` on every progress event). ❌ Don't derive a stall
  from FE event timing — a wedged transfer emits NOTHING. Register on BOTH paths, or a directory copy gets no signal.
- **`stream_pipe_file` retries once on `VolumeError::StaleDestinationHandle`** — the only layer that can retry MTP's
  stale-handle rejection. Don't drop the loop.

Semantics, flows, decisions, the auto-yield and stall-signal contracts: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
