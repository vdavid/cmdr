# Transfer (copy + move)

Copy and move, local-FS and volume-aware (Local ↔ MTP ↔ SMB), via `transfer_driver/` and `OperationEventSink`. Op
state, intent, cancel/rollback, ETA, the conflict mutex, settle: `../CLAUDE.md`. Frontend:
`apps/desktop/src/lib/file-operations/transfer/CLAUDE.md`.

Local-FS lives in `copy/`, `move_op/`, and `copy_strategy.rs`; the cross-volume engine is `volume/`, a facade reached
only as `transfer::volume::<item>` (contracts: `volume/CLAUDE.md`). All four cores run through
`transfer_driver/CLAUDE.md`. File map: `DETAILS.md` § Files.

## Streaming, cancel, and diagnosis

- **EVERY write stages, local included**: bytes land on a `.cmdr-tmp-<uuid>` SIBLING and take the real name by one
  same-directory rename, which is what makes abandoning a wedged worker safe. Local-FS goes through
  `overwrite::stage_and_land_file` (all four `LocalCopyStrategy` arms, ❌ never straight to the destination);
  cross-volume asks `resolve_staging`. A non-overwrite landing REFUSES an occupied destination.
- **A source that would land on ITSELF is a duplicate, ❌ never a conflict**: settled by `dev+ino` per TOP-LEVEL source
  before either engine's loop, because every answer the conflict machinery has destroys the original or refuses the
  user. DETAILS § "Self-collision".
- **A ledger entry carries the identity it landed with, ❌ never an mtime** (`../ledger.rs`): local = size +
  `(dev,ino)`, volume = size, a partial marked as ITS OWN. Ledgers POP as they reverse. DETAILS § "What the in-flight
  ledgers record".
- **A reversal RECHECKS each entry right before acting, ❌ never a batch** (`../reversal.rs`): changed or unprovable ⇒
  leave it, report it on `write-cancelled`; an own-partial goes on sight; a move-back never overwrites an occupied
  source (case-only self-collision aside). Only the `Drop` net is unconditional, sweeping from `../ledger.rs` — ❌ don't
  route that through `reversal.rs`, which is a module cycle. § "What a reversal does with that identity".
- **A MERGED move is NOT rollbackable, and a cross-FS move journals FINAL paths, never staging ones**
  (`note_not_rollbackable` at every merge and phase-3 conflict; `JournalDestUnder` rebases, created-dir rows included).
  `operation_log/DETAILS.md` § "Why a directory merge isn't reversible".
- **Created-dir rows journal on EVERY terminal path**: a canceled transfer keeps the dirs it made, so a reversal needs
  them or leaves an empty skeleton. ❌ A new `copy/` arm commits through `commit_journaling_created_dirs`, never bare;
  `move_with_staging` is the ONE exception. DETAILS § "Who may commit a `CopyTransaction` bare".
- **Cross-volume copy parks and yields between chunks** (`CheckpointStream`): park in place, ❌ no release/reopen. TWO
  opt-ins, ❌ don't merge: SOURCE read-yield (MTP + SMB) is unbounded, DESTINATION write-yield (SMB) is capped. A
  single-shot write skips the min-progress floor (nothing is open server-side during its drain): the only way a
  sub-floor file yields.
- **Every phase announces itself to `transfer_probe.rs`, on ALL THREE streaming paths**: ❌ no `.await` without a
  phase, ❌ never derive a stall from FE timing. A new streaming path owes BOTH `register_operation` and a
  `CURRENT_TASK_PROBE` scope (registering without binding reports nothing), plus a `MergeCtx.op_probe` if it opens a
  `FileWindow`. The watchdog judges movement by the counters the UI shows, and ❌ the probe never gets any of its own:
  one the drivers must feed is one a driver forgets. DETAILS § "The stall signal".
- **Cancel has TWO tiers, and ❌ nothing a user clicks reaches tier 2.** Tier 1 (`state.backend_cancel`) travels via
  `on_progress` so the BACKEND deletes its own partial; ❌ never race a write against it. Tier 2
  (`state.backend_abort`, the quit deadline's) skips all backend cleanup and leaves the temp to the startup sweep.
  Drain deadlines are caller-chosen: 15 s cancel, 1 s abort. DETAILS § "Two tiers of cancel".
- **Retry is per-FILE, ONLY inside `stream_pipe_file`** (`retry.rs`): ❌ never higher, ❌ never on a `Cancelled`. **The
  stall watchdog is GATED and inert** (`connection_liveness() == Dead` AND `STALL_ABORT_AFTER`, and nothing answers
  `Dead`), so ❌ never collapse the AND.

Semantics, flows, decisions, and the staging/retry/auto-yield/stall contracts: `DETAILS.md`. Read it before any
non-trivial work here.
