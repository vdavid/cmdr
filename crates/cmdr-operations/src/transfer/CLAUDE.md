# Transfer (copy + move)

Local and cross-volume copy/move (`transfer_driver/`, `OperationEventSink`). Shared operation state: `../../CLAUDE.md`.
Frontend: `apps/desktop/src/lib/file-operations/transfer/CLAUDE.md`.

Local: `copy/`, `move_op/`, `copy_strategy.rs`. Cross-volume facade: `transfer::volume::<item>` (`volume/CLAUDE.md`).
Drivers: `transfer_driver/CLAUDE.md`. File map: `DETAILS.md` § Files.

## Streaming, cancel, and diagnosis

- **EVERY write stages, local included**: bytes land on a `.cmdr-tmp-<uuid>` SIBLING, then one same-directory rename.
  Local-FS uses `overwrite::stage_and_land_file` (❌ never straight to the destination); cross-volume asks
  `resolve_staging`. A non-overwrite landing REFUSES an occupied destination, a move's renames included
  (`move_op::rename_onto_free_name`); ❌ only a name the CALLER claimed earns `land`'s clear-and-rename
  (`staged_write::LandingName`).
- **An explicit transfer leaf applies before conflict/identity checks**, on every engine; never copy then rename.
  DETAILS § "Named destinations".
- **A source that would land on ITSELF is a duplicate, ❌ never a conflict**: settled by `dev+ino` per TOP-LEVEL source,
  before the loop. DETAILS § "Self-collision".
- **A symlink is a LEAF to a same-volume move (cross-volume follows it) and to every copy's DESTINATION**: ask
  `validation::is_real_directory` / `Volume::entry_kind`, ❌ never `Path::is_dir` / `Volume::is_directory` (both follow
  it). DETAILS § "Symlinks are opaque to a move".
- **A ledger entry carries the identity it landed with, ❌ never an mtime** (`../ledger.rs`): local = size +
  `(dev,ino)`, volume = size, a partial marked as ITS OWN. Ledgers POP as they reverse. DETAILS § "What the in-flight
  ledgers record".
- **A reversal RECHECKS each entry right before acting, ❌ never a batch** (`../reversal.rs`): changed or unprovable ⇒
  leave it, report it on `write-cancelled`; an own-partial goes on sight; a move-back never overwrites an occupied
  source. Only the `Drop` net is unconditional, sweeping from `../ledger.rs`; ❌ don't route it through `reversal.rs`
  (module cycle). § "What a reversal does with that identity".
- **A move's source delete removes the LEDGER of what it copied, ❌ never the tree** (`move_op/`, `volume/`
  `source_sweep.rs`; locally after the flush, DETAILS § Durability): what arrived mid-move keeps its original
  (`AppearedDuringMove`), as does one saved over after copying (`SourceStamp`).
- **A MERGED move is NOT rollbackable, and a cross-FS move journals FINAL paths, never staging ones**
  (`note_not_rollbackable`; `JournalDestUnder` rebases, created-dir rows included).
  `crates/cmdr-operation-log/DETAILS.md` § "Why a directory merge isn't reversible".
- **Created-dir rows journal on EVERY terminal path**: a canceled transfer keeps the dirs it made. ❌ A new `copy/` arm
  commits through `commit_journaling_created_dirs`, never bare; `move_with_staging` is the ONE exception. DETAILS § "Who
  may commit a `CopyTransaction` bare".
- **Cross-volume copy parks and yields between chunks** (`CheckpointStream`), ❌ no release/reopen. TWO opt-ins, ❌
  don't merge: SOURCE read-yield (MTP + SMB) is unbounded, DESTINATION write-yield (SMB) is capped, and a single-shot
  write is exempt from its floor.
- **Every phase announces itself to `transfer_probe.rs`, on ALL THREE streaming paths**: ❌ no `.await` without a phase,
  ❌ never derive a stall from FE timing. A new streaming path owes BOTH `register_operation` and a `CURRENT_TASK_PROBE`
  scope, plus a `MergeCtx.probe` if it opens a `FileWindow`. The watchdog reads the UI's counters, ❌ never its own.
  DETAILS § "The stall signal".
- **Cancel has TWO tiers, and ❌ nothing a user clicks reaches tier 2.** Tier 1 travels via `on_progress` so the BACKEND
  deletes its own partial; ❌ never race a write against it. Tier 2 (`backend_abort`) skips cleanup. A cancel AND the
  concurrent driver's first failure fire `state.stop_work` (it DRAINS, ❌ never drops); only the intent tells them
  apart. DETAILS § "Two tiers of cancel".
- **Retry is per-FILE** (`retry.rs`): ❌ never higher, ❌ never on a `Cancelled`; a cut-off landing RESUMES. A
  `Disconnected` volume is WAITED for (`volume_outage.rs`); ❌ only re-runnable calls use `through_outages`. **The stall
  watchdog is GATED** (liveness `Dead`, SMB only, AND `STALL_ABORT_AFTER`): ❌ never collapse the AND.

Read `DETAILS.md` before non-trivial work.
