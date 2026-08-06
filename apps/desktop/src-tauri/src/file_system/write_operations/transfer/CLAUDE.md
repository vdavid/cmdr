# Transfer (copy + move)

Copy and move, local-FS and volume-aware (Local ↔ MTP ↔ SMB), via `transfer_driver/` and `OperationEventSink`. Op state,
intent, cancel/rollback, ETA, the conflict mutex, settle: `../CLAUDE.md`. Frontend:
`apps/desktop/src/lib/file-operations/transfer/CLAUDE.md`.

Local-FS lives in `copy/` (+ `CopyTransaction` rollback), `move_op.rs`, and `copy_strategy.rs`; volume work in
`volume/`, where `copy.rs` runs the phases and drives ONE of `copy_{concurrent,serial}.rs`. All four cores run through
the shared scaffolding in `transfer_driver/CLAUDE.md`. File map: `DETAILS.md` § Files.

- **`volume/` is a facade: reach it only as `transfer::volume::<item>`**, never `volume::copy::…`. Every module under
  `volume/` is private to it; a new outside caller adds a re-export to `volume/mod.rs`. That is also what keeps
  `volume/move.rs`'s `r#move` escape from leaking.

## Merge and conflicts

- **The merge invariant**: a merge never deletes or overwrites a dest file the source doesn't shadow — every policy,
  backend, and cancel/rollback/retry mid-merge (`volume/merge_tests.rs`).
- **Dir-vs-dir is NEVER a conflict**; only files prompt. **Overwrite means merge for dirs, replace for files**, enforced
  at the `apply_volume_conflict_resolution` call site, ❌ not by `Volume::delete`. ❌ It is NOT reversible.
- **A MOVE's source sweep spares every child the merge skipped** (`delete_volume_path_recursive_preserving`): a skipped
  child never reached the dest, so its source is the ONLY copy.
- **❌ Never fabricate a destination size for the conflict dialog**; report `None`. A fabricated `0` makes every dest
  look smaller, silently turning "Overwrite all smaller" into an unconditional overwrite.
- **Skip the top-level dest pre-check ONLY for a dest dir THIS op created** (`DirectoryCreation::Created`), ❌ never one
  that merely looks empty.

## Staging and durability

- **A cross-volume file write stages on `.cmdr-tmp-<uuid>`**, taking its final name only after its last byte. Ask
  `resolve_staging`; ❌ single-shot-ness earns an exemption, NEVER smallness.
- **Cleanup and rollback for a DIRECTORY source are per-FILE, never the dir root**: a merge holds pre-existing dest
  files, so a recursive root delete is silent data loss.
- **Cross-FS move deletes sources AFTER `flush_created_destinations`, preserving Skipped ones.** Same-volume move is a
  rename-merge with top-level hints only, never a subtree walk.

## Streaming, cancel, and diagnosis

- **Cross-volume copy parks and yields between chunks** (`CheckpointStream`): park in place, ❌ no release/reopen. TWO
  opt-ins, ❌ don't merge them: SOURCE read-yield (MTP + SMB) is unbounded, DESTINATION write-yield (SMB only) is capped.
- **A LOCAL `max_concurrent_ops` must ❌ NOT bound a REMOTE peer** (`transfer_concurrency`, ❌ never a `min()`; a remote
  cap always binds, which keeps MTP serial). The concurrent driver watches cancel/rollback ON ITS AWAIT, draining under
  `CANCEL_DRAIN_DEADLINE`.
- **Every phase announces itself to `transfer_probe.rs`, on BOTH drivers**: ❌ no `.await` on a transfer path without a
  phase, ❌ never derive a stall from FE timing.
- **A failure carries the path it happened ON** (`PathedVolumeError`): ❌ never re-label with the top-level source, ❌
  never `.at()` above the frame that knows the item, and a directory sweep names the first child that refused, ❌ never
  the parent's own `ENOTEMPTY`. `DETAILS.md` § "Naming the item that failed".
- **Retry is per-FILE, ONLY inside `stream_pipe_file`** (`retry.rs`): ❌ never higher, ❌ never on a `Cancelled`. **The
  stall watchdog is GATED and inert** (`connection_liveness() == Dead` AND `STALL_ABORT_AFTER`, and nothing answers
  `Dead` today), so ❌ never collapse the AND.
Semantics, flows, decisions, and the staging/retry/auto-yield/stall contracts: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
