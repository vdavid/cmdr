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
- **The stall watchdog judges movement by the byte total the UI is showing** (`state.last_progress_bytes()`); ❌ the
  probe never gets a byte counter of its own. One the drivers must remember to feed is one a driver forgets, and the
  serial path forgetting it called every 1–2-source (and every MTP) transfer stalled. `DETAILS.md` § "The stall signal".
- **Cancel has TWO tiers, and ❌ nothing a user clicks reaches tier 2.** Tier 1 (`state.backend_cancel`) travels via
  `on_progress` so the BACKEND deletes its own partial; ❌ never race a write against it. Tier 2
  (`state.backend_abort`, fired only by `abort_*_write_operation`, i.e. the quit deadline) races the source open and
  the write in `stream_pipe_file`, skips all backend cleanup, and leaves the temp to the startup sweep. The driver's
  drain deadline is caller-chosen too (`copy.rs::drain_deadline`): 15 s cancel, 1 s abort.
- **Retry is per-FILE, ONLY inside `stream_pipe_file`** (`retry.rs`): ❌ never higher, ❌ never on a `Cancelled`. **The
  stall watchdog is GATED and inert** (`connection_liveness() == Dead` AND `STALL_ABORT_AFTER`, and nothing answers
  `Dead` today), so ❌ never collapse the AND.

Semantics, flows, decisions, and the staging/retry/auto-yield/stall contracts: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
