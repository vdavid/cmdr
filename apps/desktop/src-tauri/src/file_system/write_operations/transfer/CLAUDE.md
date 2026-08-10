# Transfer (copy + move)

Copy and move, local-FS and volume-aware (Local ↔ MTP ↔ SMB), via `transfer_driver/` and `OperationEventSink`. Op state,
intent, cancel/rollback, ETA, the conflict mutex, settle: `../CLAUDE.md`. Frontend:
`apps/desktop/src/lib/file-operations/transfer/CLAUDE.md`.

Local-FS lives in `copy/` (+ `CopyTransaction` rollback), `move_op.rs`, and `copy_strategy.rs`. The whole cross-volume
engine is `volume/`, a facade reached only as `transfer::volume::<item>`; its merge, conflict, staging, and failure
contracts are `volume/CLAUDE.md`. All four cores run through the shared scaffolding in `transfer_driver/CLAUDE.md`. File
map: `DETAILS.md` § Files.

## Streaming, cancel, and diagnosis

- **EVERY write stages, local included**: bytes land on a `.cmdr-tmp-<uuid>` SIBLING and take the real name by one
  same-directory rename. Local-FS goes through `overwrite::stage_and_land_file` (all four `LocalCopyStrategy` arms, ❌
  never straight to the destination); cross-volume asks `strategy.rs::resolve_staging`. Staging is what makes
  abandoning a wedged worker safe. A non-overwrite landing REFUSES an occupied destination (`RENAME_EXCL` /
  `RENAME_NOREPLACE`), keeping the guarantee the old direct `O_EXCL` create gave.
- **Cross-volume copy parks and yields between chunks** (`CheckpointStream`): park in place, ❌ no release/reopen. TWO
  opt-ins, ❌ don't merge them: SOURCE read-yield (MTP + SMB) is unbounded, DESTINATION write-yield (SMB only) is capped.
- **Every phase announces itself to `transfer_probe.rs`, on BOTH drivers**: ❌ no `.await` on a transfer path without a
  phase, ❌ never derive a stall from FE timing.
- **Retry is per-FILE, ONLY inside `stream_pipe_file`** (`retry.rs`): ❌ never higher, ❌ never on a `Cancelled`. **The
  stall watchdog is GATED and inert** (`connection_liveness() == Dead` AND `STALL_ABORT_AFTER`, and nothing answers
  `Dead` today), so ❌ never collapse the AND.

Semantics, flows, decisions, and the staging/retry/auto-yield/stall contracts: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
