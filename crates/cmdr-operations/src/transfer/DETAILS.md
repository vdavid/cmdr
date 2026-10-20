# Transfer (copy + move) details

Pull-tier docs for `crates/cmdr-operations/src/transfer/`: architecture, flows, and decision rationale. Must-know
invariants and gotchas live in `CLAUDE.md`.

All transfer flows go through the shared driver in `transfer_driver/` and emit progress via `OperationEventSink`.

See `../../CLAUDE.md` for the shared `WriteOperationState`, `OperationIntent` state machine, cancel/rollback contract,
ETA estimator, and settle contract. `../delete/CLAUDE.md` is the parallel doc for delete + trash.

Frontend counterpart: `apps/desktop/src/lib/file-operations/transfer/CLAUDE.md` for the dialog flow, progress UI,
conflict-policy radios, and the cancel/settle close contract.

## Files

Where a symbol lives and who calls it: `codegraph_search` / `codegraph_explore`. The area's shape: `CLAUDE.md`. What the
mechanisms DO is in the sections below (copy strategy in § "Copy + move semantics" and § "Key decisions", per-file
flushing in § Durability, the two driver entry points and leaf-granular progress in § "Key decisions", the same-volume
rename-merge in § "Key decisions"). The cross-volume engine documents itself in `volume/DETAILS.md` (the archive route
included, § "One-pass sequential extract"). Only the layout facts that none of those carry live here:

- **Both local engines recheck the rollback intent AFTER their loop drains**, for the
  rollback-clicked-in-the-last-millisecond race: `copy/mod.rs` in its three-arm `PostLoopIntent` dispatch (Completed /
  Cancelled / Failed), `move_op/same_fs.rs` in a matching arm before its flush. A loop that returns `Ok` never looks at
  the intent again, so without the recheck the click is discarded and the operation reports complete — and a same-FS
  move renames a whole subtree per call, so on a small move the entire loop fits inside that window. The move's reversal
  is `MoveTransaction::rollback` (per-item recheck, non-clobbering restore) and the engine returns `Cancelled`, so the
  wrapper journals the op canceled rather than done. Pinned by
  `move_op_tests.rs::a_rollback_clicked_as_the_last_item_lands_puts_the_move_back`. Pre-flight scan, dry-run,
  disk-space, and bulk-skip filtering stay OUTSIDE the driver, in `copy/mod.rs`.
- **`move_op/` splits the local move by engine**: `mod.rs` picks between them on `is_same_filesystem` and owns what both
  need (`MoveTransaction`, `move_resolved_into_place`, `merge_move_directory` — the last is written for two callers at
  once), `same_fs.rs` holds the `rename(2)` engine, `cross_fs.rs` the five-phase staging engine, and `source_sweep.rs`
  its Phase 4 (the ledger-driven source delete). Both engine tests all attach to `mod.rs`, so each can reach both
  engines: `move_op/move_op_tests.rs` (engine behavior), `move_op/move_source_sweep_tests.rs` (what Phase 4 may remove),
  `move_op/safety_matrix_tests.rs` (one data-safety invariant swept over both engines × every resolution), and
  `move_op/move_journal_tests.rs` (the journal + reversal full loop), over the rig in `move_op/test_support.rs`.
- **`volume/` holds the whole cross-volume engine and is a facade** (`volume/mod.rs`): copy phases (`copy.rs` + the two
  drivers, serial in `copy_serial.rs` and concurrent across `copy_concurrent{,_source,_task}.rs`), move (`move.rs`,
  declared `mod r#move;` because `move` is a keyword, and only the strategy dispatcher; the cross-volume
  copy-then-delete engine it routes to is `move_cross.rs`, plus `move_same.rs` and `rename_merge.rs` for the same-volume
  rename path), the merge/staging engine (`strategy.rs`, `stream_pipe.rs`, `sequential_extract.rs`), and the supporting
  `conflict.rs`, `cleanup.rs`, `preflight.rs`, `transfer_error.rs`. Every one of those is PRIVATE to `volume`; `mod.rs`
  re-exports the nine items outside code calls, so a caller writes `transfer::volume::<item>`. Adding a caller means
  adding a re-export, not widening a submodule — that boundary is what lets the engine's internals move without a
  repo-wide rename, and it is why the `r#move` escape never appears outside this directory.
- **`transfer_driver_*_tests.rs` sit at the `transfer/` level, wired in as submodules via `#[path = "../…"]`.** That is
  incidental, not load-bearing: `file-length` allowlist keys are plain paths and get re-pathed with the file (the
  `volume/` move did exactly that for eight entries), so it is no reason to keep or to move them.
- **`transfer_probe_tests.rs` is a `#[path]` sibling, not an inline `mod tests`**, for the same reason every other big
  module here splits: the probe plus its watchdog cases is 1.3k lines in one file. `retry.rs`'s policy tests stay inline
  (the module is small and the tests read as its specification).
- **`transfer_probe_vocabulary.rs` is the probe's value half**, a `#[path]` child re-exported by `transfer_probe.rs`, so
  every caller still writes `transfer_probe::<item>`. It holds what a row and a driver can SAY (`TaskPhase`,
  `DriverPhase`, `TaskRole`, `TaskRow`) with each one's `label` and its round trip out of the `AtomicU8` it is stored
  in; the live table, the registry, and the watchdog stay in `transfer_probe.rs`. The seam is that nothing in the
  vocabulary takes a lock, holds an `Arc`, or reads the registry. ❌ Don't put a probe field or a counter here.
- **`volume_outage.rs` is the wait for a volume that went away** (§ "Waiting out a volume that went away"), with
  `volume_outage_tests.rs` beside it and `outage_volume_test_support.rs`'s `OutageVolume` as the double: a volume whose
  session a test drops and restores, like the SMB share in `#398`. The operation-side slot it fills lives one level up,
  in `../outage_slot.rs`, so the activity classifier can read it without reaching into this layer.
  `interrupted_landing.rs` holds the proofs a landing or a single-shot write the drop cut off is settled by, and
  `staged_write_landing.rs::CutOffLanding` the landing's settle itself (§ "A landing the drop cut off").
  `leftover_cleanup.rs` is the background pass that finishes a stopped transfer's cleanup (§ "File writes are staged"),
  one level above `volume/` because its engine store (`LeftoverCleanups`, in `EngineStores`) is the crate root's.
- **`transfer_probe_ends.rs` is what the probe asks the two volumes**, another `#[path]` child: `TransferEnds` (the
  source and destination, held only for their connections' liveness and the source's received-byte count) and
  `InboundWindow` (the receive rate across the last few ticks, pure and tested on its own).
  `transfer_probe_opening_tests.rs` pins what a transfer says before its first byte and while a response is still
  arriving.
- **`transfer_probe_registry.rs` is the engine's table of live transfers**, a third `#[path]` child: `Probes` (one per
  engine, in its `EngineStores`, reached through the op's scope), `register_operation`, and the guard that leaves the
  table. The watchdog holds the table weakly and ends at the first tick after the guard removes its probe.
- **`metadata_matrix_tests.rs` pins what a LOCAL copy or move keeps of each metadata class** (birth, flags, ACL, xattrs,
  mode, symlink dates, folders), one cell per path and class, over `cmdr_fs::testing::metadata_fixture`. A cell still
  pinning a bug or a loss carries a `// FLIPS` comment saying what would turn it; it reaches the move engines through
  `move_op/test_support.rs`. The folder pass it pins is `copy/folder_metadata.rs` (§ "Folders keep their metadata"). Its
  siblings outside this file: the extract cell in `volume/copy_extract_out_tests.rs`, the APFS → HFS+ cell in
  `real_image.rs`, and the SMB creation-date cells in `operations_host/backend_suites/network_dates_test_support.rs`.

## Copy + move semantics

**A local copy's ledger has one reversal and one net.** `copy/rollback.rs::rollback_with_progress` is the Rollback
button's, emitting `write-progress` with `phase: RollingBack`, rechecking each entry, and reading the intent between
deletions so the user can stop the reversal itself. `CopyTransaction`'s `Drop` is the panic net, and the only
unconditional sweep. (`reversal.rs::reverse_copy_transaction` is the same walk without the events, kept `#[cfg(test)]`
so the recheck rules can be exercised on a bare ledger.)

**A FAILURE keeps every file that landed**, on both `copy/mod.rs` error arms (`PostLoopIntent::Failed`, and a
`create_scanned_dirs_at_destination` that fails): commit the ledger, emit `write-error`, keep going no further. The only
thing cleaned is the partial, which `overwrite::stage_and_land_file` already removed on its way out — a local write
never leaves one at a real filename. ❌ Never auto-reverse here: an Overwrite's original is gone the moment the new
bytes take its name (`stage_and_land_file` step 4 deletes the aside), so deleting the replacement leaves the user with
NEITHER copy, and a mid-batch read error would cost them every file the copy had already replaced. This matches the
volume engine's error arm (`volume/copy.rs`, "keep completed files, clean up partial files"). The user still has the
full reversal from history, where they choose it with the whole picture in front of them. Pinned by
`copy/copy_failure_tests.rs`.

### What the in-flight ledgers record

Three ledgers say what an operation currently has at the destination, and each one feeds a reversal that deletes or
renames back: `CopyTransaction` (local copy, `state.rs`), `MoveTransaction` (local move, `move_op/mod.rs`), and the
volume path's `copied_paths` (`volume/copy.rs`, fed from `CreatedPaths.files`). They share one vocabulary, `ledger.rs`'s
`WrittenFile`: a destination path plus the identity it landed with.

**Why an identity at all.** A reversal acts destructively at a path the operation may have written hours ago. A bare
path can only answer "is something here", and "something is here" is what deletes the file a sync client, another app,
or the user replaced in the meantime.

**The identity differs by backend, and the cases are separate variants** so no call site can record half of one:

- **Local: size plus `(dev, ino)`.** The node id is what makes the check exact. Size alone passes a file swapped for a
  DIFFERENT file of the same size, which is exactly what an editor's write-temp-then-rename produces (the most common
  real drift there is); a rename-into-place preserves no node id, so the check correctly refuses. Nothing changes a node
  id without someone touching the file, so it adds no false alarms of its own.
- **Volume (SMB, MTP, archive): size only.** No backend but the local filesystem offers a stable node id, so these carry
  the same-size exposure the operation log's own reversal has always carried.
- **A partial is its own case**, not "a file whose identity we don't know". See below.
- **Unverifiable** is the honest answer when the stat that would have snapshotted an entry failed, and the only route to
  it.

**❌ No mtime, on any path.** (This is the ledgers' identity check only; a copy itself does keep the source's date:
`volume/DETAILS.md` § "Copies keep the source's metadata".) The obvious rule — mtime locally, since local copies
preserve it deliberately, size only on volumes — keys on how Cmdr WRITES, while the failure keys on what the destination
filesystem STORES. Snapshots are whole seconds; FAT32 stores mtime at 2-second granularity and network mounts round too,
so copy to a USB stick, cancel, roll back, and every preserved mtime reads back truncated: every file "drifted", the
whole copy left on the stick. Symlinks are worse, and unconditionally: the snapshot is the link's, but `copy_symlink`
creates a fresh link with no mtime preservation, so every copied link would be left behind.

**Where each snapshot comes from.**

- **Local copy** (`copy/single_item.rs`): one `symlink_metadata` of the destination after the write lands. It describes
  the file that actually arrived, which is what a recheck compares against, and `symlink_metadata` is why a copied
  symlink describes ITSELF (a `metadata` snapshot of a link whose target is missing reads as absent, and the link gets
  left behind). **A recheck must stat the same way.**
- **Same-FS move** (`move_op/same_fs.rs`): the source's own metadata, taken before the rename for the journal row. A
  rename carries the node id across untouched, so the pre-rename snapshot IS the landed item's identity.
- **The directory-merge move branch** (`move_op/mod.rs::merge_move_directory`): one `symlink_metadata` per child, added
  for this. It's the case that matters most — the journal marks a merge `not_rollbackable`, so the in-memory ledger is
  the ONLY thing a cancelled folder-into-folder move can reverse from. Recording nothing there would mean a later "leave
  anything I can't verify" rule reverses nothing at all, which is worse than the drift it guards against.
- **Volume** (`volume/merge_ctx.rs::CreatedPaths`): the byte count the leaf copier reports, which the journal row for
  the same leaf already carries. `copied_paths` and `CreatedPaths.files` are the same entries, so the size rides through
  instead of being dropped at the hand-off.

**A cross-FS move's copy phase records STAGED paths**, since `copy_single_item` writes under `.cmdr-staging-<op>/`.
That's the right thing to reverse while phase 2 is running (the `Drop` net removes exactly those files), and it can't go
stale afterwards: phase 3 renames the tree into place and commits the transaction, so nothing ever reverses off a
staging path that no longer exists. The rename preserves each node id anyway.

**A local directory records its node and no size.** A directory's reported size changes as children come and go, so it
proves nothing about identity and would report a folder as changed for nothing more than a file dropped into it.

**The ledgers are stacks.** Each entry is popped as it's reversed, before the destructive call, so at every instant the
ledger claims exactly what this operation still has on disk — which is what lets a reversal the user stops halfway
report honestly on what stayed. The intent is read BEFORE the pop, or a stopped reversal would drop an entry it never
reversed. `CopyTransaction`'s created-DIRS list is not a stack: `commit_journaling_created_dirs` journals it after a
reversal runs, so it has to survive one.

### What a reversal does with that identity

`reversal.rs` holds the decision, and every reversal a person can observe routes through it:
`copy/rollback.rs::rollback_with_progress` (the Rollback button), `move_op/mod.rs::MoveTransaction::rollback`, and
`volume/cleanup.rs::volume_rollback_with_progress`.

**Recheck immediately before acting, one item at a time.** ❌ Never verify a batch and then act on it: a verification
that aged while other items were processed no longer authorizes anything. `crates/cmdr-operation-log/DETAILS.md`
explains why the history engine's loop has the same shape.

**One vocabulary with the history path.** `operation_log::rollback::verify_snapshot` decides what "the size changed"
means for both, and the verdicts are `SkipReason`s (`Drift`, `UnverifiablePrecondition`, `RestoreTargetOccupied`,
`DirNotEmpty`, `AlreadyGone`, `Failed`, `Locked`). ❌ Don't fork either: a user shouldn't meet two answers to one
question. Locally the node id is checked first (exact), then the size through `verify_snapshot`; on a volume the size is
all there is.

**`AlreadyGone` counts as reversed**, not as a skip: the end state the reversal wanted already holds, and a person
doesn't need telling about it.

**What the copy made, the reversal can remove.** A copy keeps a file's Locked flag and `deny delete` ACL, and either one
makes `unlink` refuse, even for the owner. So every removal a reversal makes (and the `Drop` net, the staging cleanup,
and the sweep's removal of a temp) clears the item's user flags and ACL and retries ONCE, only after a first removal was
refused and only after the check that path already makes: `cmdr_fs::metadata::remove_{file,dir}_clearing_blockers`. A
copied FOLDER can refuse its child's removal too (Locked, `deny delete_child`, `0500`), so a removal still refused after
that opens the folder for one more retry and puts its metadata back right after (`open_parent_folder`); a folder the
reversal then removes goes the same way. A plain item costs exactly the removal it did. One this process can't clear
(`schg`, root-only) is `SkipReason::Locked`. ❌ Never clear inside `LocalPosixVolume::delete`: that's also F8's path,
and F8 must never unlock the user's own file. Why and how: `crates/cmdr-fs/DETAILS.md` § "`metadata`: what a copy keeps,
and when it goes on".

**The move path carries a SECOND guard**, the one the history engine pins alongside the recheck: the restore is
non-destructive. `fs::rename` replaces its target silently and there is no backup to put back, so an original source
somebody has since filled skips with `RestoreTargetOccupied`. **The stat that reports that is advisory; the refusal
lives in the rename** (`move_op/mod.rs::restore_rename`), which goes through `overwrite::rename_no_replace` (what it
guarantees and where it degrades: `file_system/volume/backends/DETAILS.md` § "Local renames are atomic-exclusive"). A
stat can only say what was there when it looked, so a check-then-`fs::rename` pair would still destroy a file created in
the window between them, silently and with nothing to restore from. The carve-out is the case-only self-collision — a
case-insensitive filesystem folds `dog.jpg` and `DOG.jpg` onto one entry, so a case-only move finds its own destination
sitting at the source. Same node id ⇒ the same entry ⇒ the rename back is a case correction. One local filesystem means
`(dev, ino)` settles it exactly, with no path folding needed.

**A merge's reversal recreates the folders the merge removed.** `merge_move_directory` renames a level's children across
and then removes the emptied source directory, so a rollback that only replayed renames would hit `ENOENT` on every
child whose parent was gone, leave the whole first folder merged into the destination, and still report it undone.
`MoveTransaction` is therefore a ledger of STEPS (`MoveStep::Renamed` / `MoveStep::RemovedSourceDir`), not of renames,
and the ORDER is what makes it work: children are recorded before their level's removal and the recursion unwinds
deepest-first, so popping the stack newest-first recreates the outermost directory, then the inner one, then lands each
child. Nothing has to reason about nesting. A recreation is deliberately NOT tallied: a folder that can't come back
makes every child under it report `Skipped` on its own, which is the honest partial-rollback report, and counting the
folder as well would either double-count the failure or inflate "put back N items" with something the user never counted
as an item. Pinned by
`move_op/move_reversal_tests.rs::a_rollback_after_a_merge_recreates_the_source_folders_and_brings_the_children_home`.
(The JOURNAL-driven reversal still refuses a merged move outright, for a different reason:
`crates/cmdr-operation-log/DETAILS.md` § "Why a directory merge isn't reversible". This is the in-flight
Cancel-with-rollback, which has the per-child ledger the journal doesn't.)

**The `Drop` net stays unconditional**, and it keeps its own sweep in `ledger.rs` (`CopyTransaction::remove_everything`)
rather than routing through `reversal.rs`. It runs because a thread panicked mid-copy, where a destination is as likely
half-written as complete and nobody is left to read a report. ❌ Aligning it with the guarded reversals strands partials
after a crash. Keeping it there is also what keeps the direction one-way: `ledger.rs` is the vocabulary, `reversal.rs`
is the policy over it, and `module-cycles` notices the moment that reverses.

**The bar always reaches its end.** Both progress-reporting reversals advance for every item they walk past — removed,
left alone, or refused — and interpolate both axes over the ledger's own length, landing on zero whatever they managed.
A bar stranded partway reads as a crash, and a user who thinks the app crashed never reads the summary that explains
what stayed. The created-directory prune establishes emptiness itself, so a directory a left-behind file keeps alive
reports `DirNotEmpty` rather than a bare failure.

**What comes back** is a `ReversalTally`, folded into the `CancelRollback` the `write-cancelled` event carries: a
three-state outcome plus the reversed count and the per-reason `SkipBreakdown` groups (complete counts, one example name
each) the history dialog's report already uses.

**Considered and rejected: routing the in-flight reversal through the journal.** The in-flight ledgers and the history
engine share a vocabulary (`reversal.rs`), never a mechanism. ❌ Don't reach for the operation log from the cancel path:
it would make a data-safety net depend on the log database having opened and on no row having been dropped under
backpressure, where today it works from memory regardless. For a directory merge the in-memory ledger is also strictly
MORE capable, since the journal marks a merge `not_rollbackable` while `MoveTransaction` holds every step. Pre-finalize
rollback eligibility in the journal only serves that unification, so it stays out too.

**Considered and deferred: flipping an in-flight transfer between copying and rolling back at will.** It would need
`OperationIntent` to lose its one-way `Running → RollingBack/Stopped` shape, and forward and reverse to become two
directions of ONE cursor over one ordered work list, where today forward walks the source list in the transfer driver
and reverse walks the created-path ledger in a terminal epilogue. David declined it for now; the poppable-stack ledger
is the only groundwork laid for it, so ❌ don't lay more speculatively.

### Naming what a cancel left behind

`CancelRollback` reports four kinds of leftover, and they are separate fields on purpose.

`skips` is per-reason `SkipBreakdown` groups: ledger entries a reversal walked to and DECIDED to leave, named by the
file the user is looking at. `staged_leftovers` is one `StagedLeftovers { fate, count, example_name, volume_name }`
group per `StagedLeftoverFate` for the `.cmdr-tmp-*` writes the stop's cleanup couldn't take off the destination at once
(§ "File writes are staged"): refused, waiting for the server, or a complete landing waiting for its name. Three reasons
they don't merge:

- **Nothing chose to keep a staged leftover.** Every `SkipReason` is a decision (drift, unprovable, occupied, non-empty)
  except `Failed`, and even `Failed` is a ledger item that may come back on a retry from history. A staged leftover is
  Cmdr's own working file for a write that never finished, which the destination refused or couldn't be reached to
  remove, and the background pass and the ledger's reconnect sweep take it later (§ "File writes are staged").
- **It has no user-facing name.** `SkipBreakdown::example_name` is documented as "the name the file carries right now,
  which is the one the user is looking at". A `photo.jpg.cmdr-tmp-<uuid>` is a name Cmdr invented, which is why the
  toast words it differently and says it is safe to delete. A landing's group names the file's own final name instead,
  and the toast words it as the user's file.
- **The counts have a job.** `reversed + skipped` is what the reversal's progress bar drains over (`reversal::drained`),
  and a staged temp is not a ledger entry any reversal walks. Folding it in would make the bar report progress across
  items nothing processes.

**`outcome` stays about the LEDGER**, so `RolledBack` with leftovers is a real and honest combination: the reversal did
walk every entry. ❌ Don't force it to `PartiallyRolledBack` — the frontend reads `partiallyRolledBack` with empty
`skips` as "the user stopped it partway", and that inference is what tells the two apart. Instead, the READOUT
(`src/lib/file-operations/transfer/cancel-rollback-toast.ts`) never lets a leftover render as a clean success: it drops
the completeness wording (`someDeleted`, never `doneDeleting`) and the `success` level whenever `staged_leftovers` is
non-empty, and it prints its lines OUTSIDE the reason list, which sits under "Cmdr skips anything it isn't sure about".

The volume emit sites all attach it (the copy's Rollback branch, the copy's plain-Stop branch, the cross-volume move's
cancel, and an archive edit's or compress's cancel, `../archive_edit/engine.rs::EditLeftovers`, for a landing its stop
kept), so a leftover speaks even where there is no reversal at all to report.

`originals_still_in_place` is the third, an `OriginalsStillInPlace { count }` set at exactly one site:
`move_op/cross_fs.rs`'s phase-4 cancel. It counts the user's own originals, not Cmdr's leavings, and it is the one place
`NotRolledBack` carries real news. Why the state needs a field rather than an `outcome` variant, and what the count
includes: the "A stop in the source sweep says where the files are" block below.

`recovered` is the fourth: exact old and new paths for an original displaced by a cross-type Overwrite that could not
take its old name back. Failures carry the same `RecoveredOriginal` vocabulary in
`WriteOperationError::OriginalsKeptAside`; cancellations carry it here because no error dialog opens. Local copy Stop,
volume copy Stop/Rollback, and cross-volume move cancellation all attach it. It is independent of `outcome`: the ledger
can reverse cleanly while another writer occupies the original name, and a plain Stop runs no reversal at all. The
frontend therefore breaks either silent/clean reading and names both paths at `info` level.

### The partials carve-out

`volume/cleanup.rs::append_own_partials` folds the writes still in flight when the user clicked into the rollback
ledger, marked `WrittenIdentity::OwnPartial`.

**They are NOT entries whose identity is unknown.** A partial has no size and no complete file to recognize, by
construction, so a uniform "can't verify it, leave it alone" rule would strand a truncated file at the destination — the
exact failure the mid-file-cancel work exists to prevent, pinned by `test/e2e-playwright/operation-log-rollback.spec.ts`
("cancelling inside one large file leaves no partial behind"). Nothing but this operation can plausibly own a
destination path that never held a complete file, so a reversal removes one on sight. The type keeps the two apart so
the distinction can't be lost in a later pass.

The local path needs no equivalent: `chunked_copy.rs` removes its own partial and never records it, and every local
write stages (§ "Local copies stage"), so an abandoned one wears a `.cmdr-tmp-*` name instead of the destination's.

**Move strategy.** Same filesystem detected via device ID comparison (`MetadataExt::dev`). Cross-filesystem move uses a
`.cmdr-staging-<uuid>` dir at the destination root, then atomic `rename` into place, then source deletion.

**A cross-FS move removes its staging folder on EVERY path out of Phase 4, cancel included.** Phase 5's
`remove_dir(&staging_dir)` sits after the Phase-4 source delete, which returns `Err` on cancel — so with a bare `?` a
cancel there left a stray `.cmdr-staging-<op>` folder in the user's destination for good (reproduced 3/3). The engine
now keeps Phase 4's result, runs Phase 5, and only then propagates. Safe by construction: Phase 3 already renamed the
staged tree into place, so the folder is an empty shell whichever way Phase 4 ended, and `remove_dir` refuses a
non-empty directory. Pinned by `move_op_tests.rs::a_cancelled_cross_fs_move_takes_its_staging_folder_with_it`.

**The source sweep reports itself.** Phase 4 (`delete_sources_after_move`) emits `Deleting`-phase progress: an opening
tick at zero before the loop, throttled ticks inside it, and a closing tick at full — the same shape `delete/walker.rs`
uses. It ran silently until it didn't, and the cost was a lie on every surface: the last progress the frontend had was
Phase 2's `files_done == files_total`, so the dialog sat at "17,238 / 17,238 (100%)" for the whole sweep, and a user who
pressed Pause here (the loop parks at its item boundary) got "Paused" over a full bar with the entire source removal
still ahead. The sweep is real, unbounded work — however many files and directories the move carried, over however large
a tree — so it owes a bar of its own.

The denominator is the TOP-LEVEL sources, not the leaves Phase 2 counted: the sweep's own unit of work is one source's
ledger, and a leaf-granular bar would re-count what Phase 2 already showed. A source SKIPPED at Phase 3 counts too — the
sweep is as finished with it as with a deleted one, and leaving it out would strand the bar short of full. Bytes stay
zero throughout, which is how the readout knows to drop its size bar instead of freezing it at whatever the copy left.
The frontend titles the phase "Removing the originals..." rather than "Deleting"
(`apps/desktop/src/lib/file-operations/transfer/DETAILS.md` § "Removing the originals"). Cross-VOLUME moves need none of
this: `volume/move.rs` copies and deletes PER FILE, so its bar already covers the deletion. Same-FS moves have no Phase
4 at all. Pinned by
`move_progress_tests.rs::cross_fs_local_move_reports_the_source_deletion_instead_of_sitting_at_full`.

**The cross-FS source sweep deletes a LEDGER, never a tree** (`move_op/source_sweep.rs`). Phase 2 records every source
file it actually staged; Phase 4 removes exactly that list, then the scanned directories under each source, deepest
first, each with `fs::remove_dir`. A directory still holding anything answers `ENOTEMPTY`, which the sweep treats as
success: the directory stays and so does whatever kept it alive. ❌ `remove_dir_all` has no place here — it acts on what
is on disk NOW, and the gap between the scan (taken when the transfer dialog opened, possibly a queue wait ago) and the
sweep is long enough for a download to finish, a sync client to land a file, or an editor to save. Removing the tree by
identity destroys those files without ever copying them; deleting the ledger is what makes the move invariant ("never
delete the source if the destination isn't fully in place") true of the whole source rather than only of the scanned
set.

Three kinds of original stay behind, and the sweep tells them apart. A **Skip** takes its original out of the ledger, in
either phase: `copy_single_item` records nothing for a leaf it walked past (a clash inside the staging area, a
type-mismatch parent Skip, a same-file no-op), and Phase 3's Skips arrive as `skipped_source_paths` (whole top-level
source for a single-file / type-mismatch Skip; per-child paths remapped from the staging prefix back to the source
prefix for a directory merge), which filters the ledger before the sweep plans. Those are what the user asked for, so
they are not reported as news. Anything else still standing **appeared during the move**: the sweep counts it against
`ScanResult`'s own paths (one count per unknown subtree, since that's the item a person recognizes in the pane) and
hands `AppearedDuringMove { item_count, changed_count, folder_name, folder_count }` to the completion event, which the
frontend words. The operation still completes as a success.

An original **saved over after its copy started** stays too. Phase 2 stamps each file (`SourceStamp`: size, mtime, and
`(dev, ino)`, via `symlink_metadata`) right BEFORE `copy_single_item` reads it, so a write that lands during the copy
counts as well as one after it, and the stamp rides the ledger as `LandedOriginal`. The sweep stats each original again
just before its `remove_file`; any difference, or a stamp that couldn't be taken, keeps the file, and its directories
stay through their own `ENOTEMPTY`. The count goes out as `changed_count`, named by the folder holding it (a folder
source's own name, a file source's parent's). Without it an app saving into the source between the copy and the sweep
(the whole rest of the copy, plus the flush) lost that save: the destination held what the copy read and the delete took
the only copy of the new bytes. **Decision/Why an mtime here**: the in-flight ledgers refuse one (§ "What the in-flight
ledgers record") because a destination filesystem can store a coarser clock than the source, but this compares the SAME
file on the SAME filesystem twice, so truncation hits both stats alike and can't invent a change. A same-size save
inside one coarse tick is the gap mtime can't see, which is what the node covers for rename-into-place saves. The
remaining window is the two syscalls between the recheck and the unlink; POSIX has no compare-and-unlink. Pinned by
`move_source_drift_tests.rs`. The cross-volume engine follows the same ledger rule with a listing-based stamp, and the
two share the tally (`left_in_source.rs`): `volume/DETAILS.md` § "Cross-volume move source-delete removes a LEDGER".

A source that survives the sweep, for any of these reasons, ends on `SourceItemOutcome::Skipped` with
`source_removed: false`, read off `symlink_metadata` after the sweep rather than off what the sweep intended —
`source_removed` is the vanished-path contract the search-snapshot purge acts on. The same-FS path (`move_with_rename`)
needs none of this: it renames originals directly, and anything it didn't carry just leaves the source dir non-empty.
Pinned by `move_source_sweep_tests.rs` and
`move_op_tests.rs::{cross_fs_move_skip_preserves_source_and_dest, cross_fs_move_dir_merge_skip_child_preserves_source_child}`.

**Empty directories land via the scanned-dirs pass (`copy/scanned_dirs.rs::create_scanned_dirs_at_destination`).** The
per-file loop creates directories only as FILE parents, so an empty directory — or a branch holding nothing but empty
directories — has no file to hang its creation on and used to complete "successfully" while never arriving (and on a
cross-FS move, Phase 4 then deleted the source: the empty dir was destroyed without ever landing). The pass runs over
`ScanResult.dirs` on the local copy's Completed arm and after the move's staging loop (destination = the staging dir, so
empty dirs ride the normal Phase-3 rename + cleanup). Mapping mirrors `FileInfo::dest_path`; created dirs are recorded
for rollback. Data-safety: a dest path that already holds anything (dir = merge, file = type clash) is left untouched —
an empty source dir never replaces user data. Nor does the pass build under a LEAF
(`copy/dest_chain.rs::leaf_in_the_way`, see "Symlinks are opaque to a move" below). Under a FILE, the dir sits inside a
folder→file clash that ended in Skip, and `create_dir_all` there would fail with ENOTDIR and take the whole operation
down over a clash it has already settled (pinned by
`cross_type_policy_tests.rs::a_skipped_folder_with_subfolders_finishes_and_leaves_the_file`). Under a LINK,
`create_dir_all` would succeed and make the directory inside the link's target (pinned by
`copy/copy_dest_link_tests.rs::an_empty_incoming_folder_creates_nothing_through_a_deep_dest_link`). The pass parks per
directory like every other loop (`../../DETAILS.md` § "Pause / resume"), which matters because it starts AFTER the
per-file loop: "Paused" is already on screen by then, so an ungated pass kept building the destination skeleton. Pinned
by
`copy_tests.rs::{copy_creates_empty_directory_at_destination, copy_creates_nested_empty_directories, copy_empty_directory_does_not_clobber_same_named_dest_file, a_paused_copy_stops_landing_scanned_dirs_until_it_resumes}`
and `move_op_tests.rs::cross_fs_move_preserves_empty_directories`. The volume (MTP/SMB) pipeline doesn't share the hole
— `copy_directory_streaming` creates each dir before walking its children. Right after this pass,
`copy/folder_metadata.rs::apply_created_dirs_metadata` puts every folder the operation created on its source folder's
metadata (§ "Folders keep their metadata").

**A stop in the source sweep says where the files are.** A cancel inside Phase 4 carries `originals_still_in_place`, and
that is the only honest thing it can carry. By the time the sweep runs, every source's copy has landed at the
destination (Phase 3) and been flushed (§ Durability), and each loop turn deletes one more original. So a stop there
leaves a state no other cancel in the app produces: the move effectively SUCCEEDED, some originals are gone for good,
and the rest are duplicates of files that now live somewhere else. Reversing it is out of scope by design — the bytes
are across a filesystem boundary, so putting an original back means copying it home, minutes of I/O the user never asked
for — which is why `outcome` says `NotRolledBack`, truthfully. Emitting a bare `CancelRollback::none()` was the problem:
the readout renders `notRolledBack` with nothing attached as SILENCE (a plain Cancel keeps what it wrote, and announcing
that would announce nothing happened), so a user who pressed Rollback after 200 of their 400 originals were gone was
told nothing at all.

- **Why a field, not a new `CancelRollbackOutcome`.** The count is the news, and `outcome` is a payload-free enum, so a
  variant would need the field anyway. Worse, the frontend reads `partiallyRolledBack` with empty `skips` as "the user
  stopped the reversal partway" (§ "Naming what a cancel left behind"), and every outcome that fits this state either
  carries that inference or is a lie. The field mirrors `staged_leftovers` exactly: independent of `outcome`, additive
  on the wire, and it obliges the readout to speak where `outcome` alone would stay quiet.
- **What the count counts.** Top-level sources, the same unit as the sweep's own bar: the ones the loop never reached
  PLUS the Skipped ones it walked past on purpose, both of which the user still has in the source folder. Never zero —
  the intent is read at the top of each turn, so a stop always leaves at least the item it was about to take. The
  already-deleted ones are deliberately absent: they aren't actionable (their content is whole at the destination), and
  a second number would need its own wording for the stop-before-the-first-delete case.
- **What it reads as.** `fileOperations.cancelRollback.moveAlreadyLanded`, at `info` level, in the order what-happened →
  why-that's-fine → the number: Cmdr stopped part-way through removing the originals (so the user reads that some are
  gone, rather than inferring it from the count), everything it moved is already at the destination and stays there,
  then how many originals are still in their old place. It claims nothing about an undo, because none is coming, and it
  carries no hint of a warning, because nothing went wrong.
- **`cross_fs.rs` has no post-loop intent check**, unlike `copy/mod.rs`'s `PostLoopIntent`. A Rollback pressed after the
  Phase-2 copy loop drains does NOT stop the operation there: Phase 3 renames the whole tree into place and the flush
  runs regardless, and the sweep's first `is_cancelled` is what catches the click. The report stays honest either way
  (every original is still in place, and the message says exactly that), but the operation carried on for one more phase
  after the click. Closing that gap means deciding what a Rollback in that window should DO, which is the same deferred
  product question as reversing Phase 4 itself.

Pinned by
`move_interruption_tests.rs::{a_rollback_during_the_source_sweep_reports_the_originals_it_did_not_reach, a_rollback_before_the_sweep_starts_reports_every_original_still_in_place, the_sweeps_account_counts_a_skipped_source_as_still_in_place}`
and, for the wording, `cancel-rollback-toast.test.ts`.

**Move rollback (same-FS).** `MoveTransaction` in `move_op/mod.rs` tracks `(original_source, landed)` pairs for each
rename and reverses them newest-first, under both guards above. Same-FS rename rollback is instant (just another
rename), so it runs synchronously. Cross-FS move rollback is handled by `CopyTransaction` (removes the staging
directory).

**Intentional duplication: `merge_move_directory` vs `copy_single_item`.** Both implement recursive merge with conflict
resolution, but differ in every detail: copy has progress tracking, symlink handling, byte counting, strategy selection,
and `CopyTransaction` recording. Move uses simple `fs::rename`. A shared abstraction would be forced and fragile.
Cross-references are in the doc comments of both functions.

**Cross-type Rename (file↔folder).** Rename on a type-mismatch clash follows the same cmdr Rename semantics as
file→file: the **existing** dest item stays untouched at its name; the **incoming** item lands under a fresh `name (1)`
with its full content. `conflict::apply_resolution`'s Rename arm calls `find_unique_name`, which announces each
candidate through the host and atomically reserves the chosen name by creating a 0-byte placeholder file (TOCTOU guard),
then returns `needs_safe_overwrite: true` so the caller's copy/rename lands _on_ that placeholder rather than failing
against it. (Pre-fix it returned `false`, and the same-APFS-volume copy path — `copyfile(3)` with `COPYFILE_EXCL` —
refused to overwrite the placeholder and lost the incoming bytes.)

- **Local copy `copy_single_item`:** the regular-file / symlink branches consume the placeholder via the overwrite path
  (`safe_overwrite_file` / `remove_file` + recreate). The folder→file case surfaces at the parent-creation site, where
  the incoming folder must land at the renamed root: the branch removes the reserved placeholder, `create_dir_all`s the
  renamed root, and records the redirect in a `dir_remap: HashMap<PathBuf, PathBuf>` so every subsequent child of that
  subtree (`copy_single_item` applies `apply_dir_remap` at the top) follows it. The Overwrite-vs-Rename split at that
  site is decided by `resolved.path == blocking` (Overwrite replaces in place) vs `!=` (Rename lands aside) — NOT by
  `needs_safe_overwrite`, which is now `true` for both. A Skip there answers for the whole incoming folder: the branch
  records `blocking` in `skipped_subtrees`, and every later child under it is skipped at the top of `copy_single_item`
  without a second prompt (pinned by `cross_type_policy_tests.rs::one_skip_answers_for_the_whole_incoming_folder`). The
  prompt names the source FOLDER that maps onto `blocking`, never the child that reached it first
  (`incoming_folder_for`; `../../DETAILS.md` § "Replacing one kind of entry with another needs a person's consent, per
  SHAPE").
- **Local move `move_resolved_into_place` (shared by `move_with_rename`, `merge_move_directory`, and the cross-FS
  staging Phase-3 loop):** same path-based Overwrite-vs-Rename split. Rename `fs::rename`s the source onto the reserved
  name (removing the placeholder first for a directory source, which can't rename over a file). Overwrite keeps the
  existing `safe_overwrite_dir` behavior for type mismatches.
- Pinned by `type_mismatch_rename_tests.rs` (copy + move, both directions, plus uniqueness-escalation cases that prove
  the TOCTOU reservation still escalates `name (1)` → `name (2)`).

**Volume-side Rename reserves the name on local-FS dests (TOCTOU guard).** `volume/naming.rs::find_unique_volume_name`
is the volume sibling of `conflict::find_unique_name`. The candidates it walks come from `conflict::NameCandidates` (see
`../../DETAILS.md`), the same sequence the local-FS namer walks, so a volume dest numbers identically and
`photo (1).jpg` continues to `photo (2).jpg`; this function owns only the reservation. When the destination volume is
local-FS-backed (`dest_volume.local_path().is_some()`), it reserves the chosen `name (N)` with an `O_CREAT|O_EXCL`
placeholder at the resolved local path (via `resolve_local_path`, which mirrors `LocalPosixVolume::resolve`) and
escalates on `AlreadyExists`. The streaming writer (`LocalPosixVolume::write_from_stream` → `std::fs::File::create`)
truncates the placeholder, so the new bytes land on it cleanly — there's no `COPYFILE_EXCL` in the volume copy path to
refuse the overwrite. For backends without exclusive-create semantics (MTP / SMB / InMemory, `local_path()` is `None`)
it falls back to the non-atomic `exists()` probe, with a documented narrow residual window. Pinned by
`volume/naming_tests.rs::{local_fs_rename_reserves_the_chosen_name_on_disk, local_fs_rename_keeps_extension_in_the_right_place, non_local_dest_does_not_reserve_a_placeholder}`.
**The reservation is the CALLER's to take back.** `find_unique_volume_name` answers a `ClaimedName` that says whether it
put a file on disk, because on disk that zero-byte placeholder is indistinguishable from an empty file the copy
produced, and only the caller knows whether its write ever happened. The top-level drivers already covered it (the
resolved write path is their in-flight partial, cleaned by the post-loop); a DEEP merge child did not, so a leaf that
gave up (an unreadable source, a cancel between chunks) left a `clash (1).txt` that `CreatedPaths` never learned about —
`merge.rs::copy_leaf` records on SUCCESS — and no rollback or sweep could reach. `copy_leaf` calls
`take_back_reservation` on its failure path, which deletes the placeholder ONLY while it is still empty: a size means
somebody else wrote there, and deleting a stranger's bytes to tidy up our own reservation would be the worse bug. Two
shapes never reach that path: a leaf whose FUTURE is dropped (the driver's cancel-drain deadline, or a failing sibling's
`LeafPool::submit` returning before the leaf is ever polled) runs no cleanup at all, and the plan-mode path
(`sequential_extract.rs`) reserves every clashing name at plan time and streams in a later pass that can stop partway.
So the namer also records each placeholder on the op (`ClaimedNames::hold_placeholder`, on `state.claimed_names`), every
landed write releases it (every landing in `stream_pipe_file`: the server-side copy, the runner's landing, and both
settles; the sequential data pass's commit), and the copy and cross-volume move post-loops take back whatever is left,
whatever the ending (`naming.rs::take_back_unfilled_reservations`, same empty-only rule). ❗ A write path that can land
on a reserved name owes the release: an EMPTY source that landed looks exactly like the placeholder, and on a move it is
the only copy. A same-volume move clears its own placeholder before renaming onto the name and releases it there; it
runs no sweep. Pinned by `volume/merge_placeholder_tests.rs` (including
`an_empty_file_that_lands_on_its_reservation_is_kept`) and
`volume/strategy_sequential_tests.rs::a_failed_sequential_extract_takes_back_every_unfilled_reservation`.

**Cross-type overwrites (file↔folder).** Reachable only from a plain Overwrite a person answered on a Stop prompt for
that SHAPE, its "* all" carry included; a policy nobody was asked about refuses across types (`../../DETAILS.md` §
"Replacing one kind of entry with another needs a person's consent, per SHAPE"). Move and the copy's symlink branch
route it through `overwrite::safe_overwrite_dir`; the copy's folder→file case has its own shape, below:

- **Local copy `copy_single_item`'s parent-creation site (folder→file).** The source tree wants a directory where a file
  is, or a link (`copy/dest_chain.rs::leaf_in_the_way` finds either; a link is renamed aside as the link).
  `overwrite::displace_with_directory` renames the file aside as `<name>.cmdr-temp-<uuid>` and creates the directory,
  and the CopyTransaction takes BOTH: the created dir as an ordinary `record_dir` row, and the aside as a
  `record_displaced` entry. `safe_overwrite_dir` is the wrong shape here and used to be the bug (finding 13): it drops
  the aside the instant its closure returns `Ok`, but this landing isn't finished then — the directory is empty and the
  subtree arrives leaf by leaf over the rest of the copy. The aside went the moment `create_dir_all` returned, the
  directory was never recorded (the `if !parent.exists()` walk below it sees the dir it just made), and a cancel or a
  failing first leaf left an empty `X/` where an `X` file had been, with nothing to restore and nothing a rollback could
  even remove. Now the transaction answers for it, and there are THREE answers. `commit()` discards the aside: what
  replaced it is staying and complete (the Completed path). Every reversal (`copy/rollback.rs::rollback_with_progress`
  and the `Drop` net) puts it back — LAST, after the files are removed and the dirs pruned, so the name is free when the
  original comes home; a `rename_no_replace`, so anything else standing there is left alone. A FAILURE or a plain STOP
  is the third, and neither of the first two fits it: both KEEP every file that landed (`PostLoopIntent::Failed`,
  finding 6), so the half-filled folder keeps the name too — discarding the aside would delete the user's only copy of
  that file, and restoring it has nowhere to go. `commit_keeping_displaced_aside` renames it to a ` (recovered)` sibling
  instead (`DisplacedEntry::keep_as_recovered_sibling`, over the shared `unique_name::recovered_sibling` the volume
  engine's landing rescue uses, continuing the house ` (N)` series when that name is taken), and
  `copy/mod.rs::fail_keeping_displaced_aside` wraps the failure in `WriteOperationError::OriginalsKeptAside` so the
  dialog names every path; a stop attaches the same paths to `CancelRollback::recovered` through
  `stop_keeping_displaced_aside`, so its toast names them too. The wrapped `cause` is the failure that actually stopped
  the copy, so a full disk still reads as a full disk. Pinned by
  `copy/copy_failure_tests.rs::{a_failed_folder_over_file_overwrite_keeps_the_users_file_beside_the_folder, a_failed_folder_over_file_overwrite_names_the_recovered_path}`
  and `folder_over_file_rollback_tests.rs::a_stopped_folder_over_file_overwrite_keeps_the_users_file_beside_the_folder`.
  The local MOVE engine has no such stretch: a same-FS move replaces across types with one rename inside
  `safe_overwrite_dir`, and a cross-FS move copies into a fresh staging tree (nothing to displace) and lands each item
  the same way. Only `blocking` itself is created here, so any deeper level stays with the ordinary create-and-record
  walk and reaches the ledger too. A restore is deliberately not tallied: the person counted the items they asked to
  copy, and one of their own files returning isn't one. Pinned by `folder_over_file_rollback_tests.rs` and
  `crates/cmdr-operations/src/tests.rs::{displace_with_directory_keeps_the_file_until_the_transaction_answers, a_committed_transaction_drops_what_it_displaced}`.
- The symlink branch's file→folder overwrite goes through `safe_overwrite_dir`, correctly: a symlink is complete the
  moment its closure creates it.
- Local move `move_with_rename` / `merge_move_directory`: when `resolve_conflict` returns Overwrite for a
  type-mismatched pair, the closure does `fs::rename(source, target)`.
- Volume copy/move via `volume::conflict::apply_volume_conflict_resolution`: a type swap can't temp-rename across
  backends, so **cross-type** Overwrite deletes the dest first (`remove_tree(…, UserChoseOverwriteAcrossTypes)` for
  folder dests, `Volume::delete` for file dests) before the streaming writer / recursive copy lands the source.
  **File→file** Overwrite does NOT delete first — it uses the safe-replace swap landing (see "Cross-volume file→file
  Overwrite is a safe-replace" below). Same-type dir-vs-dir still skips the delete to honor the merge-not-replace
  guarantee. Pinned by
  `volume/copy_tests/conflicts.rs::test_volume_overwrite_{file_over_existing_folder,folder_over_existing_file}`.

**Copy strategy selection** (`copy_strategy.rs`). `select_local_copy_strategy` names the mechanism, `copy_file_using`
runs it:

- macOS, same APFS volume → `LocalCopyStrategy::AppleClone`, `copyfile(3)` with `COPYFILE_CLONE` for instant clonefile
- macOS, everything else → `Chunked` (`chunked_copy_with_metadata`, 1 MB chunks, cancellation between chunks)
- Linux, network → `Chunked`
- Linux, local → `KernelCopyRange` (`copy_single_file_linux`, `copy_file_range(2)`, reflink on btrfs/XFS)
- Other platforms → `StdCopy`, the `std::fs::copy` fallback

The split exists so the landing discipline is written once instead of once per branch, and so a test can drive a
mechanism the machine's own filesystems would never select — on a Mac every tempdir pair is one APFS volume, so without
`copy_file_using` nothing would ever exercise the chunked branch that runs for an external drive or a Finder-mounted
NAS.

### Named destinations

`WriteOperationConfig::destination_name` and `VolumeCopyConfig::destination_name` supply an explicit leaf for one
source. The destination argument remains its parent directory. `validation::validate_transfer_destination_name` checks
the single-source count, leaf components, null bytes, and path limits before writes.

Native Copy seeds the shared `named_destination::initial_remap` before its identity check. The per-file loop and
empty-directory pass use that map, so staging, journal entries, durability, and rollback refer to the requested paths.
Native same-filesystem Move uses the leaf at its top-level rename and identity guard. A single-item Move inside its own
folder never reaches these engines from the dialog: the frontend runs it as a rename
(`apps/desktop/src/lib/file-operations/transfer/DETAILS.md` § "Single-item destinations"), so a case-only leaf, which
identity would drop as already in place, renames on every volume. A native cross-filesystem Move seeds the remap under
its staging directory and lands that same leaf at the destination; ordinary staging-to-final rebasing therefore journals
and flushes the named files and directories. Original-source stamps and deletion ledgers continue naming the originals.

Both-local volume facades forward the leaf into the native engines. Other volume copies and moves apply it in the serial
driver's landing probe before conflict resolution. Same-volume Move's no-op guard also checks the requested leaf.
Archive destinations carry it into the changeset's top-level inner path before conflicts and subtree expansion; archive
moves remove only originals whose bytes committed. Archive move-out passes the same config to its extraction engine.

The dialog refuses a complete target equal to its source; directory-targeted Paste/Duplicate callers retain the backend
self-collision behavior below. Frontend interpretation: `apps/desktop/src/lib/file-operations/transfer/DETAILS.md` §
"Single-item destinations". Regression coverage: `copy/named_copy_tests.rs`, `move_op/named_move_tests.rs`, the move
journal suite, `volume/named_move_tests.rs`, and archive `copy_into_tests.rs`.

### Self-collision (duplicating in place)

**Decision.** A top-level source that would land on ITSELF is a request to duplicate, never a conflict. Copy redirects
it to a free ` (N)` name and proceeds; move writes nothing and counts the item done. Neither consults the user's
conflict policy, and neither prompts. Why the suffix is ` (N)` rather than Finder's localized `copy` word:
`../../DETAILS.md`, the `unique_name.rs::numbered_name` bullets.

**A duplicate is same-volume by definition**, so on APFS it takes `LocalCopyStrategy::AppleClone` (§ "Copy strategy
selection") and a 10 GB video duplicates instantly, at no extra space. Worth not losing: anything that routes a
duplicate through a rewrite rather than the ordinary copy path pays full bytes for what the filesystem gives away.

**Why identity rather than paths.** `validation.rs::is_same_file` compares `dev+ino`, which settles a symlinked parent,
a case-differing path on APFS, and NFC/NFD normalization in one comparison. It uses `fs::metadata`, which FOLLOWS
symlinks — that is what makes the symlinked-parent case work, so ❌ don't "fix" it to `symlink_metadata`. (`rename/`
answers a narrower question with `symlink_metadata` because it must treat a symlink as its own entry; the two are
deliberately different.)

**The guard and the identity rule are one change, and a self-collision never reaches the conflict machinery.** An
operation-level lexical verdict (`source.parent() == destination`) answers the wrong question at the wrong altitude: it
refuses a whole batch over one item, and it misses every non-lexical route to the same file. Every answer the conflict
machinery can give for a self-collision is wrong, so a guard removed without the identity rule in the same change is a
data-loss change: `Overwrite` (reachable from a configured default or an apply-to-all latch on an earlier real clash)
sends the item through `stage_and_land_file`, whose replace path sets the destination aside and deletes it — when
destination IS the source, that leaves same name, same bytes, a NEW inode, broken hard links, and a reset birth time;
`Overwrite` on a directory is worse, since `safe_overwrite_dir` recreates the source empty and the copy then walks a
file list pointing at the vanished original; `Stop` shows "this file conflicts with itself" with identical size and
mtime on both sides; `Skip` silently does nothing.

**Copy asks per TOP-LEVEL source, before the loop.** `copy_files_with_progress_inner` walks `sources` right after the
transaction is opened and seeds `dir_remap` with `<dest>/<name>` → the free name. The loop below it iterates the scan's
LEAF files, and in a same-folder folder copy every leaf is its own self-collision, so asking per leaf would scatter
`a (1).txt` through the original `docs/` instead of producing `docs (1)/`. Nothing else has to change: `apply_dir_remap`
already runs at every per-file destination (`single_item.rs`, `scanned_dirs.rs`), and a plain file collapses into the
same rule because `starts_with` matches a path against itself. The scanned-dirs pass reading the same map is what lands
an EMPTY folder's duplicate, which has no `FileInfo` to hang on.

**How the name gets claimed.** A **file** uses `conflict::next_available_name` and reserves nothing on disk: the picked
name is free, and the non-overwrite landing refuses an occupied destination via `RENAME_EXCL`, so a racing writer
produces a loud failure rather than a clobber. Deliberately not `find_unique_name`: its `O_CREAT|O_EXCL` 0-byte
placeholder would sit at the destination, `path_exists_or_is_symlink` would call it occupied, and `copy_single_item`
would raise a conflict against it: the exact prompt this rule removes. A **directory** uses
`conflict::create_unique_dir`, whose `create_dir` loop IS the reservation (the directory analogue of the file
placeholder), and the claimed path is recorded on the transaction and in `created_dirs`.

**The batch remembers what it handed out** (`state.claimed_names`, a `unique_name::ClaimedNames`; see
`apps/desktop/src-tauri/src/file_system/DETAILS.md`). Both pickers consult it and record their pick, because a probe
alone can't see a name whose bytes haven't landed: selecting `photo.jpg` and `photo (1).jpg` and duplicating both sends
the first past the taken `(1)` to `photo (2).jpg` and starts the second's sequence at that very name. Without the ledger
both land in `dir_remap` pointing at one path, the first copy creates it, and the second finds its destination occupied
and drops into `copy_single_item`'s ordinary conflict path: a prompt naming a file Cmdr just made, or, under `Overwrite`
or an apply-to-all latch, two requested copies silently becoming one. The volume namer reads the same ledger for the
same reason, and needs it harder: its concurrent driver resolves several top-level sources at once, and a directory
there never reserves at all (`volume/DETAILS.md`). Pinned by `self_collision_tests.rs` on both engines
(`two_sources_in_one_series_duplicated_together_get_a_name_each` and the reverse-order three-source case;
`volume/self_collision_tests.rs::duplicating_a_whole_series_at_once_gives_every_source_its_own_name` uses three sources
precisely to reach the concurrent driver).

**A pre-known conflict never bulk-skips a duplicate.** The pre-flight matches by NAME, so a same-folder paste lists
every one of its own sources as conflicting; under `Skip` the bulk-skip prelude would drop them all and the duplicate
would silently do nothing. The prelude excludes the sources the redirect covers.

**Move drops the item above the same-FS / cross-FS split**, in `move_files_with_progress_inner`, so neither engine can
ever see a self-collision. `move_with_rename` would otherwise hand a directory to `merge_move_directory`, which threads
`dest_dir` down through recursion and would self-merge the tree (every leaf renaming onto itself or getting shuffled
aside to `name (1)`, depending on policy). `move_with_staging` is reachable for a self-colliding source only in a mixed
batch where another source sits on a different filesystem, and there it is destructive: Phase 3 lands the staged copy
over the original and Phase 4 then deletes that source, which is now the landed file. Dropped items still emit
`write-source-item-done` as `Done` with `source_removed: false`, and both engines add them to `files_processed` so the
completion tally matches what the user asked for.

**The pre-flight scan has to give the same answer, or the feature is unreachable.** `TransferDialog` runs a top-level
conflict check before confirm (`scan_volume_for_conflicts`), a single destination listing matched by NAME, so a
same-folder copy comes back with every source listed as its own conflict: the dialog announces a conflict count, shows
the overwrite/skip/rename radios, and hands the backend a pre-known-conflict list naming every source.
`scan_volume_for_conflicts_within` (`commands/file_system/volume_copy.rs`) drops those collisions with
`routing::transfer_would_land_on_its_source`. It lives one level ABOVE the per-backend `scan_for_conflicts`, which is
handed `SourceItemInfo` (a name and a size, no source path) and so cannot answer at all; widening that DTO would mean
touching three backends and every test double for a question one place can answer. That one place has to fork the way
`copy_between_volumes` forks, because the two engines answer identity differently: both-local goes to the local engine
and `is_same_file`'s `dev+ino`, everything else to the cross-volume engine and `is_the_same_item`'s one volume, same
parent, folded leaf. The filter is inert when the caller sends no `source_paths`, which is the pre-`sourceVolumeId`
name-only shape.

**A dry run answers it too**, per top-level source in `scan.rs::dry_run_scan_internal`: a self-colliding subtree lands
under a name nobody holds, so nothing in it can clash and the walk carries a `lands_on_itself` flag that suppresses both
conflict checks. Its files and bytes still count, the same way both engines count a self-colliding item in
`files_processed`. No production frontend caller sets `dryRun` today; it's reachable over IPC and MCP.

**Rollback needs nothing new, and is pinned anyway.** A duplicate is entirely the operation's own work: `created_files`
/ `created_dirs` hold only paths it created, so rolling back removes `photo (1).jpg` and can never reach the original.
Note the shape of the IN-FLIGHT rollback: `rollback_with_progress` deletes `created_files` only, so a rolled-back FOLDER
copy leaves its (now empty) directory skeleton behind, duplicates included. The journal-driven reversal doesn't:
`created_dirs` is journaled as `dir` rows on every terminal path, and it removes them empty-only after the files.

### Symlinks are opaque to a move

**Decision.** Every local move engine treats a symlink as a LEAF: it renames the link itself, and never walks through
one. So a link whose target is a directory does NOT merge with a same-named directory at the destination; that pair is a
type mismatch, and it goes through `resolve_conflict` like any other (prompt on Stop, leave both sides on Skip, land
aside as a link on Rename). The same holds mirrored: a real directory in the source meeting a LINK at the destination is
a type mismatch too.

**Why.** `Path::is_dir()` is `fs::metadata`-based and follows links, so the obvious "both sides are directories ⇒ merge"
test says yes for a link. The merge then `read_dir`s the LINK, which lists the target's entries, and renames each one
out of a folder the user never selected (`~/work/app/node_modules -> ~/shared/node_modules` against an
`~/archive/app/node_modules/` that already exists empties the shared folder). Afterwards `remove_dir` on the link fails
`ENOTDIR` and the link survives pointing at what it lost. The destination side is the mirror: renaming into
`<dest>/<link>` lands the user's files wherever the link points, outside the destination they chose.

**Where the one question lives, locally.** `validation.rs::is_real_directory` (`symlink_metadata` + `is_dir`), asked at
all four sites that could descend: `same_fs.rs`'s top-level dir-vs-dir test, `move_op/mod.rs::merge_move_directory`'s
per-child test, `cross_fs.rs`'s phase-3 test against the STAGED tree (a staged link is still a link), and
`move_op/mod.rs::move_resolved_into_place`, whose source/dest type split decides between a plain rename and the
`safe_overwrite_dir` type-mismatch dance. `conflict.rs`'s `is_file_to_folder` classification asks the same question of
the entries with `symlink_metadata`, so the apply-to-all latch buckets a link as the leaf it is. Pinned by
`move_op/move_symlink_tests.rs`.

**And on the volume side.** The same-volume rename-merge can't stat: it decides from the LISTING, and `cmdr-local`'s
`reading.rs` reports a link-to-directory as `is_directory: true` (`metadata.is_dir() || target_is_dir`).
`FileEntry::is_symlink` rides along for exactly this, so `rename_merge.rs::merges_as_a_directory`
(`is_directory && !is_symlink`) is the volume twin of `is_real_directory`, asked at both recursion sites (the per-child
dest-map hit and `late_detected_collision`'s case-folded one), at the `source_is_directory_hint` the resolver is given,
and at `apply_child_decision`'s entry-type branch. Nothing in the merge probes with `Volume::is_directory` to decide
whether to descend, because that follows links too. Pinned by `volume/rename_merge_symlink_tests.rs`.

**The TOP-level source of a volume move follows the same rule** (#140). `preflight.rs::top_level_move_hints` derives
each hint with `merges_as_a_directory` off the parent listing, and trusts a TransferDialog preview for FILES only: a
copy scan answers "does this stream as a folder", which a link to one does, so a preview can't tell the two apart. Where
there's no hint, and wherever a PATH (not a listed entry) needs typing, the question is `Volume::entry_kind` (`File` /
`Directory` / `Symlink`, a link reported as the link), ❌ never `Volume::is_directory`: whether that follows a link is
the backend's business, and an ADB phone's or an SFTP server's does. So `move_same.rs`'s source and destination probes,
`conflict.rs::resolve_dest_is_directory`, `rename_merge.rs`'s post-resolution `write_path` check, and
`cleanup.rs::remove_tree` all ask `entry_kind`. `remove_tree` is the one that loses data when it gets this wrong: every
cross-volume move's source sweep runs through it, and recursing through a link deletes the target's files.

**A cross-volume transfer materializes a link; it doesn't keep it.** The streaming engines (`copy_serial.rs`,
`copy_concurrent*.rs`, `move_cross.rs`) take the scan's answer, so a link to a folder copies as the folder it points at,
at every depth, and meets a same-named destination folder as a merge. That's deliberate: SMB, MTP, and WebDAV
destinations can't hold a link, and it destroys nothing, because a copy never touches the source and a move's sweep then
removes the LINK (above). Local-to-local transfers never get here; the local engines copy a link as a link.

**Decision: the rule for a link in a MOVE depends on where it goes** (#570). Same volume: the link itself, one rename,
never walked (above). Across volumes: FOLLOWED, exactly as a copy follows it, a SELECTED link included: the destination
gets what it points at, the source sweep removes the link, and the target keeps every byte. **Why**: that's what
"move this folder to my Mac" means to most people, and the alternatives are worse. Recreating the link elsewhere leaves
a link whose target is a path on the volume it came from (a phone's `/sdcard/DCIM` on a Mac points nowhere), and the
`Volume` trait can't make one anyway; refusing the move is friction for a result nobody loses data to. Pinned by
`volume/link_following_backend_tests.rs` (a link-following backend like ADB or SFTP, and an `lstat` one),
`volume/copy_selected_link_tests.rs` (through a dialog preview), the ADB and SFTP backend-suite cells
(`a_selected_link_moved_off_*`), and, for the same-volume half, the per-backend promise
`cmdr_fs::volume::conformance::assert_a_link_to_a_folder_moves_as_the_link`.

**So a local preview can't answer for a SELECTED link.** The copy dialog's local preview walks with the local engines'
stance (`scan_walker.rs`): a selected link is one leaf worth its own few bytes. Trusted by a cross-volume transfer, that
hint said "file" for `~/Dropbox -> ~/Library/CloudStorage/Dropbox`, the engine streamed it as one, and the read stream,
which follows the link, refused ("Cannot stream a directory"); the totals missed everything behind the link too.
`volume/preflight.rs::counted_a_selected_link_as_a_link` spots such a preview and rescans through the source volume,
whose scan follows a selected link. The dialog's pre-confirm numbers still show the link's stance, since the preview
doesn't know the destination. Pinned by `volume/copy_selected_link_tests.rs`, which runs the real walker and covers the
move's sweep (it removes the link, never the target).

A link that loops back to its own ancestor fails a cross-volume copy of the folder holding it, cleanly, with `ELOOP`
after the merge walk has descended about 16 levels through it (verified 2026-10-11 on macOS with a throwaway cell in
`copy_selected_link_tests.rs`; nothing landed).

Pinned by `volume/rename_merge_symlink_tests.rs` (the top-level cells, including the one arriving through a preview) and
`volume/link_following_backend_tests.rs`, which runs the move engines on a `LocalPosixVolume` stripped of its `lstat`
overrides so `is_directory` follows links the way a phone's does.

**A DESTINATION link is a leaf to a copy too, at every level** (#294). The mirror above holds for both copy engines: an
incoming real folder meeting a link is a type mismatch for the resolver (prompt on Stop, leave both on Skip and on a
blanket Overwrite, land beside it on Rename, replace the LINK on an answered Overwrite). Merging through it lands the
incoming files in the link's target, outside the destination the person picked, and an Overwrite then replaces that
folder's same-named files. Each engine had its own way of following the link:

- **The volume merge walk** (`volume/merge.rs::merge_level`) decides "folder meets folder, recurse" from the destination
  LISTING, which reports a link to a folder as `is_directory` on the `lstat` backends too (`LocalPosixVolume` included,
  so an MTP, SMB, or archive copy onto a local disk had it). It asks `rename_merge.rs::merges_as_a_directory` of the
  hit, which costs a real folder no probe, and routes a link through `resolve_volume_conflict`, whose
  `resolve_dest_is_directory` types it with `entry_kind`. With no `MergeCtx` (the scratch pull) there is no policy to
  ask, so a folder meeting any leaf is refused `AlreadyExists` rather than recursed into.
- **The local copy** (`copy/single_item.rs::copy_single_item`) never sees a directory: it walks FILES and creates their
  parents, so "is the parent a directory?" is its merge test, and `Path::is_dir()` follows links.
  `copy/dest_chain.rs::leaf_in_the_way` is `is_real_directory` for the whole chain from the destination root down to the
  parent, topmost first (everything below a link lives in its target and proves nothing), and what it finds goes through
  the same folder→leaf resolution a blocking file gets. The scanned-dirs pass asks it too. ❗ The walk stops AT the
  destination root: the root and everything above it are what the person navigated to, links and all (`/tmp` is one),
  which is why `copy_single_item` takes the root (the staging dir, for a cross-FS move) instead of walking every
  ancestor.

Pinned by the `deep_dest_link` cells in `volume/link_following_backend_tests.rs` (every policy, on a following backend
and an `lstat` one, plus the cross-volume move, which must also keep the unlanded source folder) and by
`copy/copy_dest_link_tests.rs`.

### A move's landings refuse an occupied name

**Decision.** Every rename a move makes onto a name it believes is FREE goes through
`move_op/mod.rs::rename_onto_free_name`, which is `overwrite::rename_no_replace` (`renamex_np(RENAME_EXCL)` on macOS,
`renameat2(RENAME_NOREPLACE)` on Linux). Three sites: `same_fs`'s top-level no-conflict branch, `merge_move_directory`'s
per-child one, and `cross_fs`'s staging-to-final rename. The inline rename (`../rename.rs`, the local branch) does the
same, keeping its `force` bypass for the caller who asked to replace and for a case-only self-rename that folds onto its
own target.

**Why.** "Is this name taken?" and "take it" are two operations, and a plain POSIX `rename` replaces its target without
a word. A file another process creates in that window is destroyed with no prompt, no conflict entry, and no backup: the
exact loss the conflict machinery exists to prevent, reached by walking around it. The window is one syscall wide, so
this is rare rather than theoretical — and the fix is the primitive every copy landing and bulk rename already used.
Refusing fails the item and leaves both copies on disk, which the user can still sort out.

**What the refusal looks like.** An `AlreadyExists` from the three transfer sites rides out as the engine's ordinary
`IoError` and fails the operation. `../rename.rs` maps it to `MutationError::AlreadyExists { name }` (❌ never a
sentence — the frontend words it in ten locales) exactly as its volume branch does. Pinned by
`move_op/move_race_tests.rs` and
`rename/tests.rs::{rename_managed_local_conflict_without_force_is_transparent, rename_managed_local_force_overwrites}`.

### Who may commit a `CopyTransaction` bare, and why exactly one caller may

`commit_journaling_created_dirs` (`copy/mod.rs`) is `record_created_dirs(op_id, &transaction.created_dirs)` followed by
`transaction.commit()`, journaling the recorded paths VERBATIM. That is right for every `copy/` arm, whose created dirs
already sit where they will live.

`move_with_staging` (`move_op/cross_fs.rs`) is the one caller that commits bare, and it has to. Phase 3 renames the
whole tree out of `.cmdr-staging-<op>/` into the destination, so by commit time every path on the transaction names a
directory that no longer exists. It therefore maps `created_dirs` through the same staging→final `remap` its leaf rows
got, journals THOSE, and only then commits. Routing it through the helper would journal `.cmdr-staging-<op>/…` paths
instead, and a reversal reading them would find nothing to remove and leave the moved folder's empty skeleton standing
at the destination. That is the bug this effort fixed; the `CLAUDE.md` rule names the exception so it doesn't get
"cleaned up" back into existence.

**The end state that would delete the rule** (not built, and deliberately out of scope for a change freeze): give the
transaction ONE commit entry that takes the rebase, `commit_journaling_created_dirs(op_id, rebase)`, with the copy arms
passing identity and `move_with_staging` passing its `remap`, and restrict `CopyTransaction::commit` so nothing else can
reach it. The staging rebase stops being an exception and becomes an argument, no arm can commit without journaling, and
the `❌` line goes away. A weaker "proof token" variant (a value `record_created_dirs` returns and `commit` demands)
only proves SOMETHING was journaled, not that the right paths were, so it wouldn't have caught this bug.

**Two sources sharing a basename stay consistent with the ordinary merge.** `dir_remap` is keyed by destination path, so
pasting `/a/docs` (a self-collision) and `/b/docs` into `/a` lands both in `/a/docs (1)/`, merged. That's the same
answer the engine already gives for `/a/docs` + `/b/docs` into `/c`, and `/a/docs` itself is untouched either way.

Pinned by `self_collision_tests.rs` (both engines, plus the `Overwrite` and apply-to-all-latch data-safety cases), by
`commands/file_system/volume_copy.rs`'s own tests for the pre-flight filter, and by `scan_tests.rs` for the dry run.

### Local copies stage

**Decision**: every local file copy writes to a `.cmdr-tmp-<uuid>` sibling of its destination and takes the real name by
one same-directory `rename(2)`, whether or not a conflict made it an overwrite. `overwrite::stage_and_land_file` owns it
and every `LocalCopyStrategy` arm goes through it, handing it a closure that puts bytes at a path.

**Why**: only the overwrite case staged before. A fresh copy wrote straight to the final name, so copying a 4 GB file to
an external drive and force-quitting halfway left a truncated file wearing the user's real filename, indistinguishable
from a complete one — the same failure the cross-volume staging work eliminated, still live on the most ordinary path in
the app, and reachable by a crash or a power loss too. It is also what makes ABANDONING a worker safe: a thread wedged
in a `read`/`write` we can't interrupt is writing to a temp nobody will rename, so the quit deadline can walk away from
it.

Two latent overwrite holes closed with it: the macOS non-APFS branch and the Linux network branch both ignored
`needs_safe_overwrite` entirely and truncated the destination in place.

**The landing refuses to clobber when it isn't an overwrite.** Moving the create off the destination name lost
something: a plain POSIX `rename` replaces silently, where the direct create's `COPYFILE_EXCL` / `O_EXCL` refused.
`overwrite::rename_no_replace` restores it (`file_system/volume/backends/DETAILS.md` § "Local renames are
atomic-exclusive"). The case it defends is a file appearing between conflict resolution and the landing.

**On error the temp goes.** A failed write, a failed aside, or a failed landing removes the temp and deregisters it: the
bytes there are a partial, or at most a complete copy whose source is still on disk (the item reports failed, so no
source is ever deleted). This differs from the async cross-volume `StagedWrite::commit`, which re-records the temp as a
landing before it clears a name, because a landing that fails there can leave the only complete copy of the new bytes (§
"File writes are staged"). Locally the landing is one synchronous syscall, so that window doesn't exist.

**What each arm puts on the temp.** Every arm but the clone goes through `cmdr_fs::metadata::apply_staged` on the temp's
open fd: xattrs, then the exact mode, then birth, mtime, and atime in ONE call, so APFS and HFS+ can't drag birth down
to an earlier mtime (`crates/cmdr-fs/DETAILS.md` § "`metadata`: what a copy keeps, and when it goes on"). The temp is
created `0600` and only takes the source's mode after its bytes are in, so a private file is never readable by others
mid-copy. The chunked arm reads everything off ONE `fstat` of the open source plus one `flistxattr`. Its data flush goes
BEFORE the dates: on a macOS SMB mount, writes still buffered when the dates go on reach the server at the handle's real
close, which is the landing rename, and the server stamps that close's time as the mtime (verified against the Docker
Samba share with `mount_smbfs`, macOS 27.0, 2026-10-11). `F_FULLFSYNC` answers `ENOTSUP` there, so the flush falls back
to `fsync(2)`. Through that mount, birth, mtime, Hidden, Locked, tags, comments, quarantine, and `user.*` xattrs all
land (the xattrs as `._` sidecars, since the fixture has no `vfs_fruit`); a resource fork doesn't, and the mode is the
mount's (`noowners`). Linux's `copy_file_range` arm carries `user.*` xattrs, mode, and dates the same way (no birth:
Linux can't set one). A copied symlink takes its source link's own birth, mtime, and xattrs (`FSOPT_NOFOLLOW`), macOS
only.

**Rename-blocking metadata goes on AFTER the landing.** `uchg`/`uappnd` on the temp (`EPERM`) or a `deny delete` ACL on
it (`EACCES`) makes the landing rename refuse. The chunked and Linux arms read their source's ACL (and the chunked arm
its user flags: Hidden, Locked, append-only, no-dump) and hand them to the landing instead of writing them on the temp.
The clone arm can't stop `clonefile` copying the flags and ACL in the same call, so the landing rename is the probe:
only when it's refused does `stage_and_land_file` clear the temp (`cmdr_fs::metadata::clear_blockers`), retry once, and
fold what it cleared into the set. A plain file stays one `copyfile` + one `rename`
(`metadata_matrix_tests.rs::a_plain_file_through_the_clone_arm_never_calls_the_metadata_helper`). A second refusal
surfaces the FIRST error, and the cleared temp is removable. The set then goes on the final name (`PostLanding::Apply`),
after the aside swap on an Overwrite, or back to the caller (`PostLanding::HandBack`): a cross-FS move's staging
landing, because Phase 3 renames the item again, and a failed Phase 3's staging cleanup must be able to remove it.
`move_op/cross_fs.rs` keeps a sparse map (staged path → set, only for items that have one) and applies it at each item's
final path after Phase 3, before the flush. **Accepted gap**: a crash between the landing and the post-landing step
leaves the file without its ACL or flags, and nothing records that; the bytes are safe, and re-copying restores it.

**Cost**: one extra rename per file, and it doesn't show. 2 000 × 4 KB local files measured a 394-400 ms median against
a 372-449 ms baseline (`transfer/copy/copy_tests.rs::local_copy_bench_many_small_files`). Pinned by
`volume/copy_crashsafe_tests.rs` § `local_staging`.

**Background cleanup is best-effort.** `remove_file_in_background` and `remove_staging_tree_in_background` run on
detached threads (used for temp/backup file cleanup, not for user-visible rollback). The staging one clears a Locked or
`deny delete` item and retries when its first `remove_dir_all` fails, since the tree is the move's own. If the network
mount disconnects or the app exits, partial files or staging directories may remain on disk. These use the `.cmdr-`
prefix, so they're recognizable.

### Folders keep their metadata

`copy/folder_metadata.rs`, for the local copy and the cross-FS move (cross-volume folders: `volume/DETAILS.md` § "Copies
keep the source's metadata"). A folder the operation CREATED takes its source folder's xattrs (tags, comments, the
FinderInfo of a custom icon), birth, mtime, atime, mode, ACL, and user flags, through the same helper as files
(`crates/cmdr-fs/DETAILS.md` § "`metadata`: what a copy keeps, and when it goes on"). A folder it merged into keeps its
own, on both engines.

- **Born no wider than its source, and fillable.** Every creation site (the per-file parent walk, the empty-folder pass,
  a folder→file Rename's root; a self-duplicate's and a folder→file Overwrite's folder are narrowed right after their
  plain `mkdir`) creates the folder with its source's permission bits plus owner `rwx`, under the umask. A `0700`
  folder's contents are never open to others mid-copy, and a `0500` one can't refuse its own children.
- **The rest goes on once the folder is full**, after the per-file loop and the empty-folder pass, deepest first,
  success path only: xattrs, then birth, mtime, and atime in one `setattrlist`, then the exact mode, then the ACL, then
  the flags (`uchg` refuses a later `chmod`). A copy that stops or fails dates nothing, so its folders stay at their
  creation mode: as private as their source, never wider.
- **On a cross-FS move, only what can't refuse Phase 3 goes on in staging**: the xattrs, the dates, and a mode that
  keeps owner `rwx`. A mode without it (`0500` can't be renamed to another parent, `EACCES`), the ACL, and the flags
  join the deferred set beside the files' (§ "Rename-blocking metadata goes on AFTER the landing" under "Local copies
  stage"), applied deepest first after Phase 3. ❗ Never to a staged folder a merge folded into the user's folder
  (`merged_staged`): `landed_at` maps it onto the user's folder, which keeps its own.
- **Undoing a copy that landed a Locked or `0500` folder** opens the folder for each child's removal (§ "What a reversal
  does with that identity").
- Pinned by the folder and move cells in `metadata_matrix_tests.rs`, `copy/folder_dates_tests.rs`, and
  `crates/cmdr-operations/src/rollback/planner_tests/blocked_items_tests.rs`.

## What a transfer says about each top-level source

Every engine, local and volume, emits `write-source-item-done` per top-level source, carrying an outcome as well as
`source_removed` (`../../DETAILS.md` § "Per-source outcomes", which also says which volume code reports which verdict).
Three of the local engine's four emit points are non-obvious:

- **A copy bulk-skipped by a pre-known conflict** emits `Skipped` from the bulk-skip prelude, before `SourceItemTracker`
  is built. Those sources are removed from `scan_result.files` up front, so nothing later in the run would ever speak
  for them and a caller tracking per-source outcomes would wait on a verdict that isn't coming.
- **A cross-filesystem move emits TWICE for one source**: `Done` with `source_removed: false` when it finishes staging,
  then either `Done` with `true` from the source-delete phase or `Skipped` when the rename phase left the source
  standing. Staging succeeding says nothing about where the item ended up, which is why the LAST event is the verdict.
- **A same-filesystem move's `source_removed` is an `lstat`, not an inference.** A directory MERGE whose children were
  partly skipped leaves the source directory on disk, so the flag is read rather than derived from the operation type.
  Pinned by `move_op_tests.rs`'s `a_same_fs_merge_that_skipped_a_child_reports_the_source_still_there`.

## Processing order

**Every driver STARTS files in tree order**, the order the transfer's progress tree lists its rows in
(`../transfer_tree/order.rs`): each folder's subfolders first, then its files, each group by the panes' name collation,
depth first, and the top-level sources ranked the same way whatever order the selection came in. Concurrency makes files
FINISH out of order; each row only claims its own state, so that's honest.

- **Local copy and cross-FS move**: the walk returns files in walk order; `ScanResult::order_by_transfer_tree`
  (`../scan_cache.rs`) installs the tree and sorts `files` by each one's rank in it (`TreeModel::tree_order_ranks`, one
  pass, then a `u32` key sort), so the copy loop starts them in exactly the order the tree shows. The preview's cached
  files are in walk order too; the sort happens when the operation takes them.
- **Volume drivers**: `volume/DETAILS.md` § "Processing order" (sources sorted after the preflight, each merge level
  sorted before dispatch, and the one exception: a solid archive's member order).
- **Compress**: `../archive_edit/fresh_plan.rs` settles the selection, then walks it the same way, every level sorted.
- **Delete and trash** build no tree and keep walk order: each source depth first, in selection order.

**Decision**: the pane's sort column doesn't steer copy order; `WriteOperationConfig` and `start_scan_preview` carry
none. **Why**: the tree displays one order, so the rows below the active one are only upcoming when the drivers start
files in that same order. A tree-ordered copy also finishes folders one by one, which is what the panes' gradual
deselection wants. The old flat sort by file name alone interleaved folders (`b/aaa.txt` before `a/zzz.txt`), so most
folders finished near the end.

## Durability (flush before reporting complete)

Copy and move don't report `write-complete` until the freshly written destinations are durable on disk — "complete"
means "you can eject now," not "buffered in the page cache." Two layers:

1. **Per-file, as it completes.** `chunked_copy.rs` calls `dst_file.sync_data()` (fdatasync) on each file before
   returning. On a long transfer, a crash mid-batch leaves every already-completed file safe.
2. **End-of-op pass.** Before `write-complete`, every handler that wrote bytes calls
   `durability::flush_created_destinations` (the same-FS move, which wrote none, takes the directory-only sibling
   below). It emits a `Flushing`-phase `write-progress` event (the FE renders **"Writing the last piece..."** instead of
   a bar frozen at 100%; see the FE doc), then:
   - `fdatasync`s every created file not in `already_synced` (chunked-synced files and clonefile/reflink dests, which
     are CoW-shared and moot to flush). A symlink has no data of its own.
   - `fsync`s every directory that gained an entry: the parent of every created file AND of every directory in
     `created_dirs`, each distinct directory once, whatever `already_synced` says. A directory that only received a
     `mkdir` is covered by its parent's fsync.
   - Answers `Result<(), FlushFailure { path, errno }>`, trying every path before answering the first failure. A
     directory fsync failing with the "this filesystem doesn't do that" class (`ENOTSUP`, `EOPNOTSUPP`, `EINVAL`,
     `ENOSYS`, `ENOTTY`, matched as raw `libc` values that differ between macOS and Linux) counts as done, with one
     `warn` per destination volume (by `st_dev`) per operation. Network and FUSE mounts are the likely ones, and failing
     there would turn every move onto them into a failed move with duplicated files. Any other errno (`EIO`, `ENOSPC`,
     `EROFS`, `ENXIO`, `ENOENT`, `EACCES`, and so on), and any file it can't stat, open, or sync, is an `Err`. Pinned by
     `../durability_tests.rs`, a per-platform errno table included.

❌ Never let `already_synced` skip a directory fsync: a Mac → USB copy is always chunked, so every file is in the set,
and a move would then delete its sources with no directory entry on disk. FAT/exFAT (the usual stick) keep no journal to
replay a lost entry.

**Per-path specifics:**

- **Local copy** (`copy_files_with_progress_inner`): `copy_single_item` populates `already_synced` from
  `StrategyCopyOutcome::already_durable`, so the data sync reaches only the strategies that don't flush themselves
  (Linux `copy_file_range`, the std fallback); the directory fsyncs run for every strategy. A copy deletes nothing, so
  an `Err` doesn't stop it: it logs a `warn` with the path and errno, and the copy completes with
  `WriteCompleteEvent.durability_unconfirmed` set (the destination's volume name, nothing else), which the FE shows as a
  persistent warning beside the completion (#461). Pinned by
  `copy/copy_tests.rs::local_copy_fsyncs_its_created_directories_and_completes_unconfirmed_on_a_flush_failure` and
  `a_local_copy_whose_flush_holds_completes_with_nothing_unconfirmed`. The volume copy handlers don't run this pass
  (each file's `write_from_stream` syncs itself and fails the copy when it can't), so only this path sets the field.
- **Cross-FS move** (`move_with_staging`): Phase 2 copies to staging (recording staging files and dirs), Phase 3 moves
  the staged items out. Before the flush, `landed_at` maps every recorded staged path to where it lives now. Phase 3
  records `landings` (staged → landed) for each conflict resolution and each rename a merge made
  (`MoveTransaction::renamed_items`), and `discarded_staged` for what a Skip threw away. The nearest recorded ancestor
  decides: a landing maps the rest of the path under it, a discard drops the path (nothing of it landed), and with
  neither the name carried over from `staging_dir` to `destination`. `destination` gets its fsync as the parent of every
  top-level landing.

**Gotcha/Why exact landing paths**: a bare `staging_dir` → `destination` prefix swap maps a folder Renamed onto a
same-named FILE to `destination/name/…`, under the user's file. That fsync answers `ENOTDIR` and fails a move that
landed fine, while the real `name (1)` goes unflushed; a Skip onto a file does the same. Pinned by the rename,
merge-rename, skip, and merge-skip cells in `move_op/move_durability_tests.rs`, each of which also checks the user's
clashing file keeps its bytes and is never flushed. The journal's created-dir rows still take the prefix swap.

**Decision**: `flush_created_destinations` runs BEFORE Phase 4's `delete_sources_after_move`, and Phases 4 and 5 run
only when it answers `Ok` AND the destination volume is still in the mount table. **Why**: The source originals are the
only other copy of the data through Phase 3. A power loss or pulled drive before the new directory entries are on disk
can leave the moved files absent from their final paths, recoverable only as orphaned blocks or a stray
`.cmdr-staging-*` entry; deleting the source in that window loses them. On `Err` the move keeps every source and
everything that landed (the transaction is already committed), logs a `warn` naming the path and errno, emits
`write-error`, and answers `WriteOperationError::MoveNotConfirmed { path, errno, volume_name }`, built from the
`FlushFailure` rather than re-deriving the errno, so the user reads "Cmdr kept your originals where they were" and not a
generic failure. Phase 5 is skipped too, so the empty `.cmdr-staging-<op>` shell stays behind.

**The destination-listed gate (the second half of the same decision).** An `Ok` flush is not proof the drive is still
there: the kernel can accept writes moments before a cable leaves, and the fsyncs that would have failed never ran. So
between the flush and the first delete, `move_with_staging` asks the mount table about the destination side's root
(`transfer_sides::TransferSide::has_left`, the same non-blocking `getfsstat` snapshot discovery and eject read). Gone ⇒
keep every source, emit `DeviceDisconnected` naming that volume, return. An unreadable mount table is NOT evidence of
leaving and lets the sweep run; a guess must never gate a delete either way. Pinned by `move_op/move_vanished_tests.rs`
(the gate, and its twin proving a mounted destination still completes).

**The sweep asks the same question at every "already gone" reading** (`source_sweep.rs`): `remove_landed_file`'s
`NotFound`, `remove_swept_dir`'s failed stat and its own `NotFound`, the per-source presence check, and the
`source_removed` flag the outcome event carries. All four answer identically for a file this move carried and for a
mount that left, and the second reading would report a user's untouched originals as deleted. On a drive that left, the
sweep stops with `SweepStopped { error, counts }`, and the counts (`removed` / `left`, in top-level items) ride out on
`WriteErrorEvent::progress_at_stop` so the copy can say which files are still theirs to find. This matches the
cross-volume move (`volume/move.rs`, which finalizes before deleting the source). Happy-path cost: one `fsync` per
distinct final directory, since Phase 2 already synced the data. Cancellation handling is unchanged: Phase 4 still owns
the in-loop cancel check and the `write-cancelled` emit. Pinned by
`move_op_tests.rs::cross_fs_local_move_flushes_final_dests_before_deleting_sources` (a custom sink snapshots that the
source still exists at the instant the `Flushing` event fires) and `move_op/move_durability_tests.rs` (which
directories, an `EIO` keeping every source, the unsupported class completing).

- **Same-FS move** (`move_with_rename`): flushes DIRECTORIES, never the moved files, through the sibling pass
  `durability::flush_touched_directories`. A `rename(2)` moves a directory ENTRY: the file's data blocks and inode are
  untouched, so an `fdatasync` on the file buys nothing, and on macOS `File::sync_data` is `fcntl(F_FULLFSYNC)` — a
  device-level barrier costing ~4.3 ms per file whether or not the file is dirty, versus ~0.03 ms for a plain `fsync(2)`
  (measured on APFS, internal SSD, 2,000 4 KiB files, 2026-09-02). What the move changed is the directory an entry left
  and the directory it arrived in, so `MoveTransaction::touched_directories` collects both sides of every recorded
  rename, de-duped, and the pass `fsync`s each once: on those 2,000 files, 8.53 s of per-file syncs became 10.1 ms of
  two directory syncs (847x). Both sides matter — fsyncing only the destination leaves the entry's removal from the
  source directory unflushed, which is half a durable rename. Emits the `Flushing` event too, so the UI is consistent
  across both move kinds. Pinned by `move_ledger_tests.rs::a_move_flushes_one_entry_per_directory_it_touched` (one entry
  per directory, and every entry IS a directory) and
  `move_progress_tests.rs::same_fs_local_move_emits_flushing_phase_before_complete`.

Only the cross-FS move acts on a failed flush; the local copy and the same-FS move log it under
`target: "write_durability"` and complete, since nothing is deleted after their flush. Delete and trash don't flush at
all; see `../delete/CLAUDE.md`.

## File writes are staged (no byte-incomplete file at its final name)

**Decision**: a cross-volume file write streams into a `.cmdr-tmp-<uuid>` sibling and is renamed onto its final name
only after its last byte, whether or not a conflict made it a safe-replace, unless the write is structurally incapable
of leaving a partial there (`volume/DETAILS.md` § "The single-shot exemption"). `staged_write.rs` owns it (its landing,
`land` and the ledger hand-off, in `staged_write_landing.rs`, its aside swap in `staged_write_landing_aside.rs`; the
generated-file publishes in `staged_write_publish.rs`). Its write sites: `stream_pipe_file`, the sequential extractor,
and the server-side copy here, plus a fresh compress and a remote zip edit's swap in `../archive_edit/`.

**Why**: the conflict layer only staged a file→file Overwrite. A NEW file has no conflict, so it streamed straight to
the destination path — and a transfer killed mid-stream left a truncated file wearing the user's real filename, which is
exactly what the 2026-07-31 wedge did to two phone backups (one at 0 bytes, one truncated at 4 MiB;
`docs/notes/incidents/2026-07-31-transfer-wedge/README.md`). Backends do delete their own partial on an error return,
but a force-quit, a panic, or a dropped future runs no error path at all. Staging makes the guarantee structural rather
than dependent on cleanup running.

**Who stages.** Every staged write stages here, and the temp has ONE owner, the `StagedWrite`: `WriteStaging::Stage`,
`StageOntoClaimedName`, or, for a file→file Overwrite, `StageOverOriginal`. The conflict layer only DECIDES that a write
replaces the user's file (`Replaces::Swap`) and hands back the original's own name as `write_path`, so `write_path`
names the file the user gets on every path and ❌ no layer above `StagedWrite` ever sees a temp-shaped destination. Each
call site derives the staging identically via `volume::strategy::staging_for(&replaces, landing)`, so there is one rule,
not four. Every write site then runs that choice through `resolve_staging`, the single place a staged write can become
`SingleShot`, and ❌ never for `StageOverOriginal` (the original surviving the whole write is the stronger guarantee).
The one write that is `SingleShot` from the start is an in-place Overwrite on a destination that publishes every write
whole (`staged_write::Replaces::InPlace`, `volume/DETAILS.md` § "Whole-publish destinations"). `WriteStaging::Unstaged`
is `stream_pipe_file`'s fallback for a destination that can't rename (below), ❌ never a swap's.

**Generated validated files reuse the same ownership.** `StagedWrite::begin_generated` always mints and records a
sibling stage, and announces the archive and its temp like `begin` (it takes the archive as an `Announced`). A publisher
whose volume declares `supports_atomic_replace_rename()` lands it with one force rename. Other publishers call
`commit_with_displaced_original`: move the old destination to the existing tracked `DisplacedDestination`, land without
force, discard the aside on success, or restore it on refusal. If restoration also refuses, the shared rescue gives the
original a stable ` (recovered)` name and the typed failure names it; the sole good copy never remains under a reapable
`.cmdr-tmp-*` name. Both live in `staged_write_publish.rs` and ❗ classify before acting, as the landing does: a publish
rename the transport cut off is settled once the destination is back (`CutOffPublish::resume`: a whole temp is renamed
again, a gone temp with the name at the written size is published), ❌ never answered by deleting the temp, which had
reported a failure for an archive the server had published.

**Landing** (`staged_write::land`) renames FIRST and only clears the final name if that rename said something is in the
way: it usually isn't, and a speculative delete would burn one extra round trip per file. The name can still be taken (a
`Rename` resolution's `O_EXCL` placeholder, a file that arrived after the caller last looked), which the second attempt
covers.

❗ **A swap lands the other way round** (`LandingName::ReplacesOriginal`, from `StageOverOriginal`): the original is
known to be in the way. Two shapes, picked by the destination:

- **One replacing rename** where the destination's forced rename replaces a file in one step
  (`Volume::supports_atomic_replace_rename`: a local disk's `rename(2)`, SMB's `ReplaceIfExists`;
  `staged_write_landing.rs::replace_in_one_step`). The name never goes empty, so the temp is never the only copy and
  nothing is recorded as a landing; a refusal leaves the original whole and takes the spare temp away; a transport fault
  cuts the landing off (`CutOffAt::ReplacingRename`), and the settle runs the replace again on a whole temp or counts a
  gone temp as landed only on the source's bytes (the name held the original, maybe at the same size). A wait that ends
  without the share leaves the original whole, the temp to the abandoned-write cleanup. One round trip instead of two.
- **Aside first** everywhere else (`staged_write_landing_aside.rs::swap_through_an_aside`): the original is renamed
  ASIDE (`<name>.cmdr-temp-<uuid>`, `StagingTemp::mint_aside`), the temp renamed onto the empty name, and the aside
  deleted last. ❗ The aside is recorded BEFORE the move (`DisplacedDestination::hold_for_swap` →
  `in_flight_temps::hold_aside`, an `ItemKind::FileAside { destination, expected_size }` at the write's size, through
  the same on-disk barrier as a landing record, failing closed the same way), so the original is whole under one of its
  two names at every moment, and a crash anywhere leaves a record the sweep settles: the name empty → the original goes
  back; the name holding exactly the replacement's size → the aside goes; anything else → the aside gets a
  ` (recovered)` name. The temp stays an ordinary `Temp` throughout: the original is never cleared, so the temp is never
  the only copy, and no landing record is needed. A refused move aside takes one stat (`what_a_refused_delete_left`):
  still at its name → the spare temp goes and the refusal is reported; gone after all → the landing goes on; can't tell
  → the aside record waits for the sweep. A refused landing rename puts the original back
  (`DisplacedDestination::put_back`) and discards the temp; only when the original can't go back either does the temp
  leave temp space (` (recovered)`, re-recorded as a landing first), the aside kept for the volume's next arrival. An
  aside the destination won't delete after a landing stays beside the new file, recorded, for the sweep to remove. ❌
  Never `rename(force)` there: MTP's force rename doesn't delete what's there and can leave a duplicate, and SFTP's
  (without `posix-rename`) deletes first in a step of its own.

The flags that wait for the final name go on after the commit, at the original's name, as for any staged write
(`volume/landed_flags.rs`). Pinned by
`staged_write.rs::tests::{a_swap_moves_the_original_aside_first_and_never_renames_with_force, a_swap_on_an_atomic_replace_backend_lands_in_one_replacing_rename, a_write_over_an_original_stages_and_is_never_single_shot}`,
`volume/swap_landing_tests.rs`, at all five write sites by `volume/safe_replace_grid_tests.rs`, through an outage by
`volume/swap_outage_tests.rs` (both shapes), and a crash after every step of a remote zip edit by
`archive_edit/remote_tests.rs::a_crash_after_any_step_of_the_swap_loses_no_archive`.

❗ **The moment the new bytes take the original's name (or may have), the operation stops being rollbackable**
(`staged_write_landing.rs::Landing::original_replaced` notes it on the journal, `NotRollbackableReason::Overwrote`): the
file an undo would restore is no longer at its name. A rename that can't be confirmed counts too. That's the ONE moment
for a failure path at every site; a landed overwrite's journal row also says `overwrote`, recorded by the site from its
`Replaces` (`CreatedPaths::record_overwrite` in the merge leaf and the extract, after the file landed), and a swap that
failed before it put the original back, so the op stays rollbackable. Pinned by the grid's failure cells.

❗ **Before the landing clears a claimed name, its temp is recorded as a landing.** From that delete on, the temp may be
the destination's only copy, so `land` re-records it as `ItemKind::Landing { destination, expected_size }`, at the byte
count the write confirmed (`StagedWrite::commit` takes it, so no stat measures the temp), through the non-temp barrier:
on disk before the delete goes out, then the `Temp` record retires and the path leaves `state.in_flight_temps`
(`in_flight_temps::hold_for_landing`). That order means a crash at any moment leaves a record the sweep finishes the
landing from, ❌ never one it deletes (the rule: `crates/cmdr-operations/DETAILS.md` § "What the sweep does with each
kind"). ❗ **When the record (this one, or a swap's aside record) can't be confirmed on disk, nothing is cleared or
moved** (fail closed): the writer is gone, the log refused the line, or the operation names no path space for it
(`in_flight_temps::OnDisk`, `NotOnDisk`). The landing then fails as a typed refusal (`LandingAftermath::NotRecorded`,
reaching the user as `WriteOperationError::LandingNotRecorded`) with the original in place and the temp taken away: a
crash after a delete with no record would leave the only copy on a temp the next transfer into that folder reaps on
sight (it wears the source's dates), and one after a move aside with no record would leave the user's file on a scratch
name nothing points back from. A ledger that keeps nothing on disk by design (one no claim opened, as a test's engine)
has nothing to confirm against and lands as before. Pinned by
`staged_write.rs::tests::{a_swap_whose_aside_record_has_no_writer_leaves_the_original_in_place, a_swap_whose_aside_record_the_log_refused_leaves_the_original_in_place, a_swap_with_no_path_space_for_its_aside_record_leaves_the_original_in_place}`.
The landing record retires when the rename lands it, when the rescue below gives it a real name, or when the refused
delete left the name taken (the temp is a spare copy then: it's discarded, or turned back into a temp if the discard
fails). A landing that can't tell whether the delete went through, a rescue that couldn't rename either, and a landing
cut off mid-way (a dropped task, a wait that ended without the share) leave it recorded, held for the volume's next
arrival. Pinned by
`staged_write.rs::{a_claimed_name_landing_is_on_disk_as_a_landing_before_it_clears_the_way, a_landing_cut_off_after_clearing_the_way_is_finished_by_the_next_launch}`.

❗ **A cleared way plus a refused second rename strands committed data**, and the landing gets it out of temp space on
the spot. The bytes are complete by then, the destination name is now EMPTY, and the temp wears a `.cmdr-tmp-*` name,
which a real name keeps out of every sweep's reach for good, recorded or not.
`recovered_name.rs::rescue_out_of_temp_space` renames it to `<name> (recovered)<.ext>` and the failure carries that path
as `FinalizeFailure::aftermath` (`LandingAftermath::NewDataKeptAt`), a claimed name's landing and a swap whose original
couldn't go back alike; `stream_pipe_file` and `sequential_extract` label it with the DESTINATION
(`PathedVolumeError::from_landing`), since a rescued file is one the user can't find on their own. A failure whose
`delete` didn't succeed rescues nothing and reports nothing: the destination still holds whatever was in the way (§ "A
landing that fails with nothing cleared takes its temp away" below). Pinned by
`staged_write.rs::a_landing_that_cleared_the_way_and_then_could_not_rename_rescues_the_new_bytes`.

❗ **And only for a name the CALLER claimed** (`LandingName`). `AlreadyExists` says something is in the way; only the
caller knows whose it is, because only the caller knows whether a conflict resolution put this write at this name. A
`Rename` pick reserved its name with a placeholder and a cross-type Overwrite already set its destination aside, so both
answer `ClaimedByTheCaller` and the delete-then-rename stands. A write nothing resolved answers `ExpectedFree`, and its
landing REPORTS the clash instead: the file in the way is the user's, under a policy (Skip, an unanswered Stop) that
never touched it, and our temp is taken away (below). Where each caller's answer comes from: the deep merge from whether
`resolve_merge_child` decided the child, the two top-level drivers from `TransferContext::dest_name_claimed` and
`CopyTask::dest_name_claimed`, the one-shot cross-volume file move from "the destination is assumed CLEAR"
(`move_file.rs`). Pinned by
`staged_write.rs::{landing_clears_a_claimed_name_that_is_genuinely_in_the_way, landing_onto_a_name_believed_free_leaves_it_alone}`
and, end to end,
`volume/merge_case_fold_tests.rs::a_name_that_fills_up_after_the_listing_is_not_cleared_by_the_landing`.

❗ **A `SingleShot` write has no landing, so the guard travels with the write instead.** `resolve_staging` keeps the
`LandingName` on the upgrade (`WriteStaging::SingleShot(landing)`), and `StagedWrite::write_mode` turns an
`ExpectedFree` one into `WriteMode::CreateNew`: the destination creates the file only if the name is still free and
refuses with `AlreadyExists` otherwise, atomically on SMB (the compound write's `FileCreate`). The refusal reaches the
driver as the same `AlreadyExists` a refused landing reports, is never retried, and cleans nothing (`abandon` is a no-op
for single-shot, and nothing at the name is ours). A `ClaimedByTheCaller` single-shot write stays `CreateOrReplace`,
because its placeholder sits at the name. The whole contract and its backends: `volume/DETAILS.md` § "The single-shot
exemption".

❗ **Only `VolumeError::AlreadyExists` earns the delete.** A rename over a network backend fails for plenty of reasons
that say nothing about the destination: a session that dropped, a server that refused, SFTP v3 folding an errno into its
one catch-all code. Clearing on every `Err` deleted a file the user still had and reported the blip that "justified" it,
and it also let a backend that can delete but not rename destroy the destination and then answer `NotSupported`. Pinned
by
`staged_write.rs::{a_rename_that_failed_for_another_reason_leaves_the_destination_alone, a_backend_without_rename_does_not_delete_what_it_cannot_replace}`,
which inject a failing rename through `InMemoryVolume::with_rename_failing`.

❗ **A landing that fails with nothing cleared takes its temp away at once.** The name still holds what it held, the
source still holds the new bytes, and the temp is a complete copy of ours nobody needs; left alone it would sit in the
user's folder as a visible `.cmdr-tmp-*` until the hourly reap. `recovered_name.rs::discard_unplaced_temp` deletes it
(best effort: a delete that fails too is logged and left to the sweeps) in every such exit of `staged_write::land`: a
rename refused for any reason but `AlreadyExists`, a clash on an `ExpectedFree` name, a refused delete of a claimed
name, and a swap whose original stayed at (or went back to) its name. ❗ **Classify before acting**: none of these rules
runs for a step that failed on the TRANSPORT (`transport_fault`), the first rename, a clearing delete, a move aside, or
the rename after either alike. The server may have performed it, so the landing is cut off right there
(`staged_write::LandingError::CutOff`) and settled after the outage wait (§ "A landing the drop cut off"); a stat, a
discard, or a rescue then would run on whatever session answers next, a sibling's fresh one on a stale error, and act on
a guess. The refusal rules run only for a refusal, or once the wait gave up (`CutOffLanding::give_up`), and a temp
nothing settled with nothing cleared goes with the abandoned-write cleanup. ❗ A refused delete is checked with one stat
before anything else (`what_a_refused_delete_left`), because over a network the server can act and the answer still be
lost: the name still taken means discard, the name empty after all means the way IS clear and the landing goes on
(discarding there would leave neither file), and a stat that can't answer leaves the temp alone. ❌ Never discard once
the way was cleared: from then on the temp may be the only copy at the destination, and it is rescued instead. Short of
the empty-after-all case, the copy fails with the typed refusal. Pinned by `volume/swap_landing_tests.rs` (the three
delete answers, and both drivers end to end), `staged_write.rs::tests`, `volume/copy_landing_race_tests.rs` (no temp
left on either driver), and on a live server by
`operations_host/backend_suites/webdav_refusal_test.rs::webdav_integration_a_refused_replace_keeps_the_users_file`.

**Finding the litter.** A staged temp is listed in `state.in_flight_temps` for exactly as long as it is a removable copy
of ours: while it is written, and through the landing until `land` releases it (it landed, it was discarded, or the way
is about to be cleared). So a landing that never finishes (a task the drain deadline dropped, a crash) leaves the temp
to the abandoned-write and startup sweeps, which is safe because the name was never cleared; and a temp that may hold
the only complete copy at the destination is never in the set, and is recorded as a landing, which no sweep deletes. The
NAME-based sweep (`volume/cleanup.rs::reap_stale_transfer_temps`) spares every path the ledger records, landings
included. A task that STOPS (a cancel, or a sibling's failure: § "Cancel and rollback reach a parked driver") takes its
own temp with it, through its backend's abort and `StagedWrite::abandon`, which retires the temp only once its delete
went through: a delete the destination turned down (a share that's out) leaves it listed, so it isn't forgotten on the
share. Whatever is still listed when the driver's loop ends belongs to a task the drain's deadline DROPPED mid-write, or
to such an abandon, so `volume::cleanup::clean_abandoned_staged_writes` takes those, deep-merge children and swaps
included. Pinned by `staged_write_tests.rs::an_abandon_the_destination_refuses_keeps_the_temp_listed`.

❗ **No driver-side partial list names a staged write's temp**, a swap's included: `failed_write_leaves_ours_at` answers
`false` for `Stage` and `StageOverOriginal`, because the name the drivers hold is the user's (a free name, or the
original), and the temp has the one owner above. ❗ **Nor a claimed name** (`StageOntoClaimedName`): all that's ours
there is a `Rename` pick's placeholder, which `ClaimedNames` takes back while it's empty
(`volume/naming.rs::take_back_unfilled_reservations`). Once the landing cleared the name, what stands there is a save
that came after our delete, or our own file, landed by the reconnect sweep from a kept landing before the post-loop ran,
and the partial cleanup deleted either (`#575`; pinned by `volume/copy_landing_race_tests.rs`). So a concurrent task dropped between a swap's move aside and its rename
leaves the original at its recorded aside, held for the volume's next arrival, whose sweep puts it back, and the temp in
the in-flight set for the abandoned-write cleanup (the failure still names whichever sibling ended the op). Pinned by
`volume/safe_replace_grid_tests.rs::a_concurrent_task_dropped_mid_swap_keeps_the_original_for_the_sweep_to_put_back`.

**What that sweep can't remove now waits for the server, in the background, and is swept on reconnect**
(`leftover_cleanup.rs`). ❗ A Cancel never waits on a dropped share, and a failure takes the same shape (the lead's
decision in `docs/specs/transfer-outages-and-cleanup-plan.md`): `clean_abandoned_staged_writes` deletes inline only what
a LIVE destination takes, within `CleanupPolicy::inline_budget` (5 s in all), and the terminal event goes out the moment
it returns. A temp it couldn't take (the destination read out, a delete met a `transport_fault`, or it refused) goes to
ONE background pass per stopped operation, on the engine's runtime, holding no lane:

1. A destination that's out gets one reconnect attempt (the operation's own, the only dial in the whole cleanup), then a
   short wait for someone else's (`CleanupPolicy::wait`, 20 s). An outage the operation already gave up on
   (`Outages::gave_up_on`) gets the attempt as its one look, ❌ no second wait.
2. A refused temp is asked again, up to three times, waiting `retry_after` (3 s) and doubling (3, 6, 12 s), whether the
   refusal came at the stop or the moment the destination came back: a write the drain's deadline dropped with no credit
   on hand sends its CLOSE a moment late, and the session an outage dropped holds its temp open until the server notices
   that session is gone. On Samba that's a moment after the new session is up, so the first delete on it can meet
   `STATUS_SHARING_VIOLATION`, and on a busy server one 3 s retry wasn't always enough (verified on the Docker `guest`
   fixture through `OutageProxy`: 3 of 48 cancel cells under parallel load refused at once, 1 of 64 still refused 3 s
   later; M3 Max, 2026-10-20, `#578`). Without the retries the temp waited for the share's NEXT reconnect, after the
   stop had said it goes once the share is back. A real drop sends the server no FIN, so it can hold the old session far
   longer; that temp waits in the ledger as step 3 says.
3. What's still there goes to the ledger's pending set ONCE, after the pass is done
   (`in_flight_temps::keep_temp_for_arrival`), so the pass and the arrival sweep never act on one record together. The
   sweep runs on the volume's next arrival or in-place reconnect (§ "Which path space a recorded partial lives in"),
   never dials, and by then the session that held a handle open is gone.

Tier 2 (`backend_abort`) and the quit (`LeftoverCleanups::stop`, fired by `abort_all_write_operations` for a pass whose
operation already unregistered) end the pass where it is, the records registered and on disk for the startup sweep. Only
removable temps reach it: a landing a stop left recorded (`state.kept_landings`) is ❌ never deleted, and its sweep rule
finishes it.

**And it REPORTS, by fate.** Both volume drivers hang the result on `write-cancelled` as
`CancelRollback::staged_leftovers`, one `StagedLeftovers` group per `StagedLeftoverFate`: `Refused` (the destination
answered no), `RemovedWhenBack` (it was out), and `LandsWhenBack` (a complete file on its way to its name, named by that
name and ❌ never worded as junk). Without the report a user who chose Rollback was told the destination was clear while
519 MB across four files (2.2 GB in another run) sat on their NAS. ❌ Don't fold them into `CancelRollback::skips` — a
skip is a LEDGER item a reversal walked to and chose to leave, and `reversed + skipped` is what the reversal's bar
drains over. A failed op's `write-error` carries no leftovers; its pass still runs. Pinned by `volume/cleanup_tests.rs`
(out at the stop, refused for good, refused once, tier 2, a kept landing), `leftover_cleanup_tests.rs` (the one look,
refused the moment it's back), and end to end by
`volume/copy_outage_tests.rs::a_cancel_while_the_share_is_out_reports_at_once_and_cleans_up_once_it_is_back`. See §
"Naming what a cancel left behind".

**Known gap, deliberately open: the abandoned SMB handle.** A refused sweep is about the handle itself, and only a task
the drain's deadline DROPPED leaves one: a cancel and a failure both let every task stop on its own first, so its writer
closes and deletes its temp the way a Cancel's does. Since smb2 0.32.0 a dropped handle (`FileWriter`, `FileReader`,
`FileUpload`, `FileDownload`) also queues its SMB2 `CLOSE`, with no flush, before the drop returns, so it reaches the
server after that handle's in-flight requests and before anything sent next: a copy task dropped mid-stream leaves a
closed temp, and the sweep's delete right after works. Two shapes still leak a handle the app owns and can't reach:

- **A drop with no credit on hand**: its `CLOSE` goes out a moment later, so a delete sent at once can still meet the
  open handle.
- **An `async fn` that holds a handle only for the call** (`write_file_compound` for a small staged temp, the pipelined
  reads and writes, `copy`): its future dropped between CREATE and CLOSE sends no `CLOSE`, and the server holds the file
  open for the rest of the session ([smb2#24](https://github.com/vdavid/smb2/issues/24)).

❌ **Retry with backoff within the session cannot fix the second** (measured: refused continuously to t+191 s on a real
share, then deleted instantly from a fresh session). The consequence is survivable rather than permanent: the background
pass's retries handle the first shape, and the second waits in the ledger for the share's next reconnect or the next
launch, when the session that held it is gone; the leftover toast is what tells the user in the meantime.

Recording goes through `cmdr_operations::in_flight_temps` (`track_temp` for a staged write's temp), which keeps the
operation's in-memory list AND a persisted log in the app data dir once the engine claims it. Local copies record there
too (`overwrite::stage_and_land_file`), but only the cross-volume drivers run `clean_abandoned_staged_writes`, since a
local op's temps are recorded and retired synchronously and anything left is a thread that never came back.

**Crash recovery, two sweeps.**

- **The recorded ones, at startup, with NO age gate.** The engine's ledger claim (`Operations::claim_ledger`,
  `crates/cmdr-operations/DETAILS.md` § "Who claims the ledger"), made before any copy can start, replays the persisted
  log and removes what an earlier run left in flight. The age gate below exists to protect a temp a CONCURRENT Cmdr is
  streaming into; a path this app recorded when it minted the UUID in it needs no such protection, and the instance lock
  already keeps two processes off one data dir. It only ever removes paths carrying the scratch marker, so a corrupted
  log can't become a delete-anything primitive. The deletes run on their own thread, ❌ never inline in `setup`: a
  recorded partial can sit on a Finder-mounted NAS that stopped answering, and an `unlink` there blocks for a minute or
  two, which on the startup thread reads as an app that won't launch. Granularity and why there is no fsync:
  `in_flight_temps.rs` module docs. Each record names the path space it lives in, and a volume's may be deferred; see §
  "Which path space a recorded partial lives in" below.
- **The rest, on a later transfer into that directory.** `volume::cleanup::reap_stale_transfer_temps` runs once at the
  start of each cross-volume copy AND each cross-volume move, over the destination directory only: one `list_directory`,
  then a `delete` for each `.cmdr-tmp-*` FILE whose mtime is at least `STALE_TEMP_MIN_AGE` (1 hour) old and whose path
  the engine's ledger doesn't record on that volume (`in_flight_temps::is_recorded_on`, any kind, live or pending). The
  age gate is what makes it safe against a concurrent instance — a streaming write touches its temp every chunk, and
  even a destination-side foreground park is capped at a second — and an entry with no reported mtime is spared. ❗ The
  ledger skip is what protects this instance's own complete temps: the write stamps the SOURCE's dates on the temp
  (`stamping.rs`), so one that finished streaming (on its way to land, or a landing that may hold the only copy) reads
  as old at once. Pinned by
  `volume/cleanup_tests.rs::the_stale_temp_reap_skips_what_the_ledger_records_however_old_it_looks`. It mirrors
  `archive_edit::remote::reap_remote_temps`. This is the backstop for anything the log missed (a power loss, a non-UTF-8
  path); a leftover deeper inside a copied subtree waits for a transfer into that directory, and there is no global
  filesystem sweep and there shouldn't be. ⚠️ It runs on both transfer verbs because a destination folder a person only
  ever MOVES into would otherwise keep its leftovers forever; the copy path pays for one extra customer of the listing
  (its `DestNameIndex`), the move path discards it.

  ⚠️ **The age gate means this is not "the next transfer clears it".** A person who cancels and retries straight away
  meets their own leftover: it is minutes old, and recorded, so the reap spares it. The ledger's reconnect sweep is what
  clears a stop's leftovers, which is why the leftover toast promises "the next time it connects", never a transfer.

**Cost**: one extra rename per staged file. On SMB that is one round trip, which roughly doubles the wire cost of a file
that would otherwise take the compound CREATE+WRITE+FLUSH+CLOSE fast path — the exemption below is what keeps a
10k-tiny-file copy to a NAS from paying it. A destination that can't rename can't stage; `stream_pipe_file` then re-runs
the file unstaged (a `NotSupported` landing), which no production backend triggers — Local, SMB, and MTP all rename.

Pinned by `volume/copy_staged_write_tests.rs` (abandon the copy future mid-stream — the in-process equivalent of the
force-quit — and assert nothing sits at a final name, for a fresh copy, an overwrite, and a merge child) and
`staged_write::tests`.

## Which path space a recorded partial lives in

A staged temp is a sibling of the destination file, so it lives wherever the destination does: on the local filesystem
for a local copy, and inside an SMB / SFTP / WebDAV / MTP volume's own namespace for a transfer to one. A path can't
tell those apart, so every persisted record carries its path space, and the sweep reaches it through the right one. An
operation that names neither has its partial kept in the operation's own in-memory ledger and NOT persisted: that
ledger's sweep holds the volume handle already, while a persisted path with no path space is one the next launch could
resolve against the local filesystem and act on there.

Both engines write KINDED records; the plain `+` line is only what an earlier build's log holds, still replayed and
swept. The cross-volume `staged_write.rs` records its temps (and their landings) volume-space homed, naming the volume
from `WriteOperationState::dest_volume_id()` (the destination half of `journal_volumes`), or the one a caller writing to
more than one volume names (`StagedWrite::begin_over_original`, a remote zip edit's swap). The local `overwrite.rs` and
`move_op/cross_fs.rs` take their home from `in_flight_temps::home_for` — the operation's typed destination side
(`transfer_sides.rs`), ❌ never a prefix match against the mount table. The three homes and why the local-mount /
volume-namespace split can't be derived: `crates/cmdr-operations/DETAILS.md` § "What the sweep does with each kind".

**A path on a removable drive is NOT local-homed**, even though it's an ordinary local path. A drive can be unplugged at
the next launch, and a record resolved against its mount point then reads as already gone, so the ledger forgets the
only trace of a leftover still sitting on the drive. Homed to the volume, it waits for the drive instead — and stores
its path relative to the root, since a returning drive can mount somewhere else.

**Decision: the record keys on the volume ID, not the mount root.** The ledger's whole point is surviving a restart, and
a root doesn't: macOS remounts the same share at `/Volumes/naspi-1` after a wedge. A volume ID is identity by
construction (`cmdr_fs::volume::ids`) and is what the index DB and `lastUsedPaths` already key on.

**Decision: an unreachable volume keeps its record; the sweep never goes looking.** The ledger claim runs before
`init_volume_manager`, and a NAS registers later still, so a volume-borne record is normally re-recorded and held
pending. ❌ The sweep must not mount, dial, reconnect, or authenticate to reach one: a launch that blocks on a dead
mount, or that pops a password box, is worse than the leftover it was chasing. Dropping the record instead would forget
the only trace of a multi-gigabyte partial.

**What makes the deferral terminate** is `OperationVolumes::on_volume_arrival` (the registry's own, in the app): the
ledger subscribes (once, and only when something actually waits), and the moment that volume ID lands in the registry,
or a registered one reconnects in place, the records waiting on it are claimed and settled on a task. A registration and
a reconnect are the two moments a volume is known-usable without probing anything; the app turns a backend's `Connected`
transition into the announcement (`file_system/volume/DETAILS.md` § "Telling someone a volume arrived"). `claim_pending`
hands each record out once, so a flapping link can't stack two sweeps on one record. The listener takes only the ID and
the registry hands the volume back through `resolve`, which keeps the dependency pointing one way: the registry knows
nothing about the ledger, so the two subtrees stay acyclic. A delete the volume refuses puts the record back, so a later
arrival or a later launch tries again.

**The sweep reports.** `SweepHandle::wait()` answers a `SweepTally` (swept / already gone / deferred / left alone) and
the same numbers go out as one `info` line. Every record lands in exactly one counter, which is what makes a "the sweep
quietly did nothing" regression visible instead of inferred.

## Pause in the local move engine

**Every per-item loop in `move_op/` parks** (cancel wins; the park returns immediately once the intent stops being
`Running`). Five loops, because a local move can spend all its time in any one of them: `same_fs.rs`'s top-level rename
loop, `mod.rs`'s `merge_move_directory` child loop (a folder-into-folder move does ALL its renaming there, so the
top-level gate never sees it), and `cross_fs.rs`'s Phase-2 staging copy, Phase-3 rename-into-place, and Phase-4 source
delete. Three of them ask `state.stop_or_park_sync()`, the one boundary question (`../../DETAILS.md` § "Pause /
resume"). The Phase-2 and Phase-3 loops call `pause_gate.wait_while_paused_sync` on its own, because their cancel checks
live one level down inside `copy_single_item` / the merge, so there is no cancel arm at the boundary to fold the park
into. Phase-2 is the one a user who hits Pause usually means: it's the phase that moves bytes.

A rename is a single syscall, so the item boundary is the only place a rename engine CAN park — there's no equivalent of
the streaming path's between-chunks checkpoint below.

**Why it matters**: `manager::set_paused` flips the record to `Paused` and `wait_reason` reports it, so the UI says
"Paused" whether or not the engine parks. An engine that keeps renaming makes that a lie, and the person who paused
because they picked the wrong destination is told they have time to intervene when they don't. Pinned by
`move_op_tests.rs::a_paused_move_stops_renaming_until_it_resumes` and
`a_paused_folder_merge_stops_renaming_its_children`.

## Pause reaches between chunks (cross-volume streaming path)

**A paused cross-volume copy stops MID-FILE, between chunks — not only between files.** The per-source loop top in the
serial drivers parks between files (after the `is_cancelled` check). But a single large file (e.g. an MTP→local import)
is one source: gating only at the loop top would let it stream to completion while the UI shows "Paused" (the confirmed
bug). The fix is `transfer/checkpoint_stream.rs`'s `CheckpointStream`, a `VolumeReadStream` decorator
`volume::stream_pipe`'s `stream_pipe_file` wraps the source stream in. Its `next_chunk()` runs a between-chunk
checkpoint once per chunk before delegating: (1) `state.park_while_paused()` parks while paused (it returns the instant
the work stops, a cancel or a failure drain, so stopping wins), then (2) `tokio::task::yield_now()` so a long transfer
doesn't starve foreground tasks (listings, navigation, progress) on the runtime.

**Why a stream decorator, not a new trait param.** The per-chunk progress callback (`on_progress`) is sync
(`ControlFlow`), so it can't `.await` to park or yield. The chunk loop lives inside each backend's `write_from_stream`.
Wrapping the SOURCE stream injects the async checkpoint into that loop at the single production wiring point
(`stream_pipe_file`) without touching the `Volume` trait or any backend — the loop already awaits `next_chunk()` once
per chunk.

**Data safety.** The checkpoint sits at a chunk boundary: the previous chunk is fully written and the next isn't yet
read, so a paused op holds only its in-flight `.cmdr-tmp-<uuid>`, never a torn target. The wrapper only gates progress —
it forwards each inner chunk untouched (no drop, double-write, or reorder) and forwards `total_size()` unchanged so the
destination still sees the real size. Cancellation is NOT enforced in the wrapper: the backend's existing `on_progress`
`is_cancelled` check after each write owns the cancel-then-cleanup ordering (drop the handle, remove the partial).
Pinned by `volume::strategy::tests::streaming_copy_parks_mid_file_while_paused_then_resumes` (paused multi-chunk copy
freezes, resume completes with correct bytes) and `streaming_copy_cancel_while_paused_mid_file_unblocks`
(cancel-while-paused unblocks, ends `Cancelled`, leaves no partial).

**A destination that buffers ahead of the wire parks on `stop_signal`.** Parking the SOURCE stops a backend that sends
each chunk as it reads it (SMB, SFTP, WebDAV, MTP). S3 instead fills a whole part (or a whole one-PUT object) from the
source before sending it, and a local source drains in milliseconds, so a pause parked a source that was already empty
while the parts kept going out: a 75 MB upload paused at 20% ran to 100% before showing as paused. So
`CheckpointStream::stop_signal` hands the destination the operation's own Cancel and pause (`cmdr_fs::volume::ScanStop`,
the scan's vocabulary), through an `UploadPark` adapter that announces `ParkedPause` to the probe; S3 parks its requests
on it (`crates/cmdr-s3/DETAILS.md` § "Pause"). `archive_edit/fresh_compress.rs`'s spool stream, also its operation's
checkpoint, answers the same way. Every other wrapper forwards its inner stream's answer. Pinned by
`volume::strategy::tests::a_destination_that_buffers_ahead_parks_on_the_operation_pause`.

**Pause for an MTP source (navigate the phone while a transfer is paused).** Park-in-place is correct for EVERY backend
now, MTP included. An MTP read is a sequence of bounded ~8 MiB windows (`mtp/connection` § "Bounded-window reads");
between windows nothing is in flight and the one-per-device PTP session is free, so a paused MTP copy that simply stops
starting the next window leaves the phone listable/navigable without releasing anything. `CheckpointStream` parks on
`state.park_while_paused()` and, on resume, the next `next_chunk` reads the next window from the current offset — no
`cancel_and_release`, no reopen-at-offset. The wrapper tracks `bytes_yielded` (== the destination temp's length); since
parking leaves the offset alone, the next window reads `[bytes_yielded, …)` with no gap or overlap, and the
destination's `write_from_stream` (and its safe-replace temp+rename) sees one continuous chunk stream. A cancel observed
while parked lets the next chunk flow through to `on_progress`, so the cancel/cleanup contract is the park-in-place
contract. Pinned by `volume::strategy::tests::paused_mtp_copy_parks_in_place_then_resumes_byte_exact` (pause freezes
mid-file, never releases the source, resumes byte-exact with a single open at offset 0),
`paused_mtp_copy_cancel_while_paused_keeps_no_partial`, and `unpaused_mtp_copy_streams_straight_through`.

**No release/reopen machinery.** This bounded-window model has NO `cancel_and_release`/reopen path, because there is no
held session to release between windows. `Volume::pause_releases_read_stream()` is `false` for every backend (kept only
as a trait extension point), and `Volume::open_read_stream_at_offset` keeps its offset parameter (MTP implements it
correctly) but the copy path only ever calls it with `offset == 0` via `open_read_stream`. The bounded-window read
itself lives in `mtp/connection` (it needs `mtp-rs`'s `Storage::download_partial_64`, in `mtp-rs` 0.21.0 — the minimum
`apps/desktop/src-tauri/Cargo.toml` pins).

### Foreground auto-yield (navigate the phone DURING a transfer, no pause)

The bounded-window mechanism and the why behind debounce/floor/Running-not-Paused are captured in this section and
`crates/cmdr-mtp/src/connection/DETAILS.md` § "Bounded-window reads". Design history is in git (former
`docs/specs/2026-06-25-bounded-window-mtp-reads-plan.md` and `2026-06-22-navigate-during-transfers-plan.md`).

The same park-in-place behavior is also driven by a SECOND trigger: foreground device work pending while the copy is
RUNNING. A long MTP→local copy could otherwise keep re-grabbing the device lock window after window and starve a
foreground listing/nav. The fix makes the per-window checkpoint a `background_yield_point`, exactly like the index scan
does at its unit boundary: a transfer becomes a yielding background user of the per-device `DevicePriorityGate`
(`crates/cmdr-mtp/src/connection/scheduler.rs`). Because the read is bounded windows that hold nothing between them,
"yield" means simply **don't start the next window** until foreground drains — no session release, no reopen — gated on
`foreground_pending` instead of the pause flag.

**The arm** (`CheckpointStream::auto_yield_to_foreground`, after the pause handling in `checkpoint`). It fires when ALL
hold: the source opts in (`Volume::supports_foreground_yield()`, MTP and SMB — this is the enable-switch, NOT a
release/reopen proxy), not cancelled, `bytes_yielded < total_size`, the min-progress floor is satisfied, and
`Volume::foreground_pending().await` is true (an atomic load behind the device gate). Then it parks (debounces, below)
and returns; the next `next_chunk` reads the next window from the current offset. The op stays **`Running`** the whole
time — this is a transient DEVICE yield, not a user pause, so it must NOT touch `OperationIntent` or the manager's
`LifecycleStatus` (the queue window's Pause/Resume button keys on those; flipping to Paused would misreport user
intent). Byte exactness and the cancel-wins contract are the same park-in-place contract as pause; the arm only adds
WHEN to park.

## Skipped work moves the bars, and stays out of the rate

A skip is progress: the child is decided, it will never be copied, and both bars have to reach their totals. So every
skip credits the same two counters a completed leaf does, and every skip also goes through
`WriteOperationState::note_skipped(files, bytes)`.

The two halves that keep it honest:

- **The event stays GROSS.** `state.rs::enrich_progress` samples the ETA estimator with `bytes_done` / `bytes_total` /
  `files_done` / `files_total` NET of `skipped_totals()`, while the emitted `WriteProgressEvent` keeps the gross numbers
  the two bars render. A skipped 4 GB file therefore advances the bar without reading as 4 GB/s of throughput. Remaining
  work is identical either way (`done` and `total` lose the same term), so this changes the RATE only, never the ETA's
  numerator. ❌ Don't reintroduce a "reseed the estimator baseline" call for this: the netting covers the bulk-skip
  preludes and the per-item skips with one rule, and a baseline reset can't express a skip that lands mid-window.
- **Every skip site calls it.** The bulk-skip preludes (`transfer_driver/sync_driver.rs`, `async_driver.rs`,
  `volume/copy.rs`'s concurrent one), the per-iteration skips (`TransferOutcome::Skipped`, `ConflictDecision::Skip`),
  and the deep-merge decline (`volume/merge.rs`'s `MergeChildDecision::Skip`, via `SourceProgress::skip_leaf`). Miss one
  and it spikes the reported speed instead.

**`SourceProgress::skip_leaf` is THROTTLED, where `LeafProgress::complete` is not.** The completion milestone bypasses
the throttle on purpose, so a big leaf's `N/N` always lands. A skip can't have that: a merge declines tens of thousands
of children back to back, and an unthrottled emit apiece is an IPC flood. The end-of-operation emit carries the final
numbers, so nothing is lost by letting the throttle eat the middle.

Why the deep-merge site is the one to guard hardest: a merge whose children all clash moves no bytes at all, so if its
skips don't credit the bars the dialog sits at `0 of N` for the entire run and reads as frozen. A user cancelled a
healthy transfer over exactly that (`ERR-AYVM4`).

## The stall signal

`TransferActivity` (`types.rs`) rides on every `write-progress` event: `in_flight`, `still_for_seconds`, and
`waiting_on`. It is attached in `state.rs::enrich_progress`, the one place every emit site already routes through, so no
signature grows and no caller has to remember.

**Why the backend classifies rather than the frontend.** The probe knows the distinction that matters — parked ON
PURPOSE (a user pause, a foreground yield that keeps the app responsive) versus genuinely stuck — because it holds each
task's phase. A frontend timer can only see "no events lately", which would call every deliberate yield a stall and
train people to ignore the warning. `OperationProbe::wait_reason` ranks them: a conflict prompt (`You`) outranks
everything because a person is being asked a question; a pause is authoritative from the pause gate; a device wait is
only claimed when EVERY in-flight task agrees, since one task still streaming means something else is holding things up;
otherwise `Unknown`, which is the shape the 2026-07-31 wedge took.

**The dump's in-flight count measures the WINDOW, and a walker is not in it.** Every row declares a `TaskRole`: `File`
for a merge leaf or a top-level file source, `Walker` for a directory source's walk. A walker deliberately holds no
`FileWindow` permit (one held across a recursive descent deadlocks the operation at width 1), so it was never one of the
writes the width bounds — yet counting it printed `in_flight=11/10` on a perfectly healthy 10-wide copy, which reads as
a broken limiter and costs a reader their first minutes during an incident. `render_dump` counts only `File` rows
against the width and reports `walkers=N` separately, and the walker's own row carries a ` (walker)` marker so the
header's arithmetic can be read back against the table. `TransferActivity::in_flight`, which the UI renders, still
counts every row: it answers "how many things are open", not "how full is the window".

**A row's number is unique within the dump, and says where the row sits.** Every row carries a `TaskRow`: a TOP-LEVEL
source renders as `#<its position in the source list>`, and a leaf its walker hands to the window renders as
`#<that source>.<leaf>`. So `#3.7` tells a reader which of the operation's sources is producing the work, and `#3` is
the walker to read for the walk it came out of, which a globally unique counter couldn't say. Depth doesn't enter the
label: one walker numbers every leaf of its whole subtree from a single counter (`LeafPool.next_leaf`, which only
climbs), so the number stays short and is never reused inside one operation.

Two shapes keep it that way. `begin_task` takes the `TaskRow`, the same way it takes a `TaskRole`, so a new row can't
join the table without deciding where it sits. And `MergeCtx` carries a `MergeProbe` (`strategy.rs`: the operation's
probe PLUS the walker's own row) rather than a bare probe, so a walk can't be handed the table without being told which
source to number its leaves under. ❗ Hand a walker a numbering scheme of its own and the collision is back: a source
and its first leaf both reading `#0`, once per walker, is what the ` (walker)` marker alone had to disambiguate. The
drivers' `transferring-source(#N …)` and `preparing-next(#N …)` details render through the same `TaskRow::label`, so the
phase and the table can't drift apart.

**The dump holds paths, never path text.** A row keeps its two ends as `PathBuf`s and the driver's step as a
`DriverDetail` (a fixed label, or a path under an optional row), and the dump renders each path through `LogPrivate`.
The dump reaches error reports (the stall warning, the wind-down report), so a path flattened to a `String` first would
ship as unbounded prose. `set_driver_phase` takes only a `&'static str` label; a path goes through
`set_driver_phase_at`.

**Every driver sets its `DriverPhase`, and one that forgets says nothing.** The field is initialised to `Starting`, so a
driver that never advances it reports `driver=starting()` however long it has been running. Both drivers set it now:
`PreparingNext` around the destination-metadata fetch, `ResolvingConflict` around the resolver (parking on a person is
unbounded by design, so naming it explains the whole stall, where `paused=` only covers the pause gate),
`TransferringSource` for the whole of a serial source, and `PostLoop` once the driver returns. The serial driver has no
window to drain, which is why it reports `TransferringSource` rather than the concurrent driver's `AwaitingTasks`: the
phase's job there is to say the wedge can only be in the rows below.

**A conflict prompt is read from the responder slot, NOT from task phases.** `wait_reason` and `watchdog_step` both
check `state.conflict_slot.is_awaiting()` first. `TaskPhase::ResolvingConflict` only ever covers a deep-merge child,
because TOP-LEVEL conflict resolution runs on the DRIVER, between tasks — so a scan of task phases misses the common
case entirely. Without that check, a transfer sitting on an unanswered overwrite prompt accrues stall time, starts
heartbeating after 3 s, and after `STALL_NOTICE_SECONDS` tells the user their transfer has stopped moving while it is
asking them a question (it reports `Unknown`, so the frontend's `you` suppression can't save it either). Pinned by
`a_transfer_waiting_on_a_conflict_answer_is_not_stalled` and
`the_watchdog_does_not_accrue_stall_time_behind_a_conflict_prompt`. The slot is authoritative: it is stored before the
`write-conflict` emit and taken when the answer lands, and it covers deep-merge prompts too.

**Why the watchdog emits.** Progress events are driven by chunk callbacks, so a wedged transfer emits nothing at all —
the UI keeps rendering the last event it received, confident ETA and all, for as long as the wedge lasts. That is
exactly what the incident's dialog did. Once the byte counter has been still for `HEARTBEAT_AFTER_SECS` (3 s), the
watchdog re-sends the last recorded event each tick with a fresh activity snapshot. The counters are deliberately
unchanged (nothing moved); only the activity is new. It goes through `emit_progress_via_sink`, so the ETA estimator also
sees the stillness and decays its own estimate to `None` rather than the FE having to special-case it.

**Movement is EITHER axis, and both counters are the ones on screen.** `watchdog_step` resets the stillness clock when
`bytes_done` OR `files_done` changes. A merge declining thousands of children advances the file count with the byte
total flat, and that is a transfer doing its job: judging on bytes alone called one healthy run stuck for a minute and
the user cancelled it (`ERR-AYVM4`). `OperationProbe::bytes_done` / `files_done` read `state.last_progress_bytes()` /
`last_progress_files()`: the aggregate totals of the newest `write-progress` event the operation published. The probe
keeps NO counter of its own, and ❌ must never be given one again. It had one — an `Arc<AtomicU64>` handed in at
registration — which only `volume/copy_concurrent.rs` ever fed. The serial path (fewer than three top-level sources, or
any backend reporting `max_concurrent_ops() == 1`, so every MTP transfer) keeps its running totals in the leaf ledger
and left that atomic at zero, so a perfectly healthy copy read as still and the dialog said "the transfer has stopped
moving" from the 10 s mark to the end of the transfer, over a bar that was visibly climbing. Reading the published event
instead makes "the watchdog measures the same number the user sees" true by construction rather than by review: every
emit site already has to route through `state.rs::enrich_progress`, which is what stores it. Pinned by
`a_transfer_publishing_progress_across_file_boundaries_is_never_called_still`, which drives three leaf boundaries
through a real `LeafProgressLedger`.

Two consequences worth knowing. Movement resolution is the progress throttle (`progress_interval_ms`, 200 ms by
default), two orders of magnitude finer than the 10 s notice and the 20 s log line. And the reading is `0` before the
first progress event, which no registered operation can observe: `copy_volumes_with_progress` emits its opening
`Copying` tick before it registers the probe.

**Thresholds, and why they differ.** `STALL_TICK` is 1 s (it sets the granularity of `still_for_seconds`, which the UI
reads). `STALL_AFTER` is 20 s for the LOG: a log line wants to stay rare. The UI speaks sooner (`STALL_NOTICE_SECONDS`
in the frontend), because a frozen bar with a confident ETA is a lie the moment it stops being true. Both read the same
`still_for_seconds`, so the dialog and the log can't disagree.

**Before the first byte, and while a response is still arriving** (ERR-CNK7M: a 377 kB SMB file read in ONE compound
request, so the byte counter could only move once, at 20 s, and the dialog sat on a silent 0% bar until then). Three
pieces, all backend:

- **`TaskPhase::OpeningSource` waits on `Source`.** The request is out and nothing has come back, so a stall past the
  notice names the source instead of falling through to `Unknown` ("the transfer has stopped moving"). The
  all-tasks-agree rule is unchanged.
- **`TransferActivity::opening_source`**: nothing has landed yet (`bytes_done() == 0`) and every FILE row is in
  `OpeningSource`. Walker rows don't count either way: a walk lists while its leaves open. False under a pause or a
  prompt. To say it early, `heartbeat_due` re-sends from the FIRST still second while nothing has landed, rather than
  from `HEARTBEAT_AFTER_SECS`: that 3 s wait exists so a heartbeat never lands between two healthy chunk callbacks and
  nudges the ETA, and before the first byte there is no ETA to nudge.
- **`TransferActivity::source_inbound_bytes_per_second`**: each tick the watchdog reads the SOURCE's
  `Volume::connection_bytes_received()` (SMB only, off `smb2`'s `Connection::inbound()`, which counts a frame's bytes as
  they land) into `InboundWindow`, and publishes the rate across the last five readings, but only on a still tick. It's
  `None` the moment bytes land (the ordinary rate says it with verified numbers), under a pause or a prompt, when the
  count drops (a rebuilt session starts from zero), and when nothing arrived. ❌ Never credit it to the byte bar: the
  bytes are unverified until their frame completes (signature or AEAD), and it's connection-wide, so a listing on the
  same share adds to it. The dump header carries it too (`source_inbound=NB/s`), so a stall log says whether the source
  was still sending.

**An event that moved the counters goes out as moving.** The stillness and the receive rate are the watchdog's readings
from its last tick, up to a second old. Attached as they stand, the first event after a chunk lands would carry "still
for 12 s" and a stale rate, and on a transfer emitting once per chunk the UI would hold that until the next chunk.
`activity_for(operation_id, Some(event))` compares the event's totals with the last published ones, and a mover goes out
with zero stillness, no rate, and `Moving`. Pinned by `transfer_probe_opening_tests.rs`.

**Which transfers register a probe, and why the list is what it is.** All three STREAMING cross-volume paths:
`volume/copy.rs` for both of its drivers (concurrent and serial), and `volume/move.rs`. Each also binds its own
`CURRENT_TASK_PROBE` around `copy_single_path`, which is the half that matters most — a path that registers but doesn't
bind looks wired and isn't: its rows sit at `spawned` forever, `wait_reason` can never answer `Source` or `Destination`,
and `stream_pipe_file` can't arm a stall-abort. Without registration at all, `state.rs::enrich_progress` misses the
lookup and every event goes out with `activity: None`, so the dialog shows a frozen bar with a confident ETA and says
nothing. That is what a cross-volume MOVE did until it was registered, and a silent failure on the operation holding the
user's only copy of the data is worse than a noisy one. The move also sets `TaskPhase::Finalizing` on its handle
directly once the copy phase ends, because its source sweep runs after the task-local's scope closes and a dump taken
during it must not still claim it is streaming.

**`TaskPhase::Walking`** (label `"walking"`) covers the same gap for a DIRECTORY source: `merge_level` sets it at every
level while it lists the source and resolves what is already at the destination, before any of that level's files reach
the window. Without it a folder copy's row read `spawned` for the whole walk, so a dump of a healthy transfer looked
like a task that had never started. It is a WORKING phase, so `wait_reason()` is `None` (the level waits on source and
destination in turn, and naming either would be a guess) and `is_abortable_on_stall()` is false.

**`TaskPhase::CopyingOnServer`** (label `"copying-on-server"`) covers a server-side copy, from its stage to its cleanup
(`volume/server_side_copy.rs`). It used to report `Streaming` with `0/0` bytes, so a dump couldn't tell a task stuck
there from one stuck mid-stream (ERR-PTRGY). It waits on `Destination`, the server doing the copy. It is NOT abortable
on a stall: a one-call copy (SFTP's `copy-data`) moves no counted byte from start to end, so stillness proves nothing,
and only an SMB end can prove a connection dead while no SMB pair copies on the server.

NOT registered, on purpose: the same-volume move (`volume/move_same.rs` + `rename_merge.rs`) is a rename, so it streams
no bytes and has no wedge of this shape; and the local-FS copy, delete, and trash keep no in-flight table at all, which
`WriteOperationState::activity` handles by falling back to its own pause gate and conflict slot. Pinned by
`volume/move_progress_tests.rs::{cross_volume_move_tells_the_dialog_what_it_is_doing, a_cross_volume_move_in_flight_shows_its_source_and_phase}`,
which sample the live table from inside the destination's write — the one window where a row exists.

**The probe surface.** Two backends opt in; `LocalPosixVolume`, `InMemoryVolume`, and `ArchiveVolume` use the trait
defaults (`false` / no-op) and never auto-yield.

- **`MtpVolume`** (which holds `device_id` and reaches the global `connection_manager()`):
  `supports_foreground_yield() → true`, `foreground_pending()` → `MtpConnectionManager::foreground_pending(device_id)`
  (the per-device gate's `foreground_pending()`, `false` if the device is absent), and `wait_until_foreground_idle()` →
  `MtpConnectionManager::background_yield_point(device_id)` (parks until the gate's pending count hits zero).
- **`SmbVolume`**: same three methods, delegating to `crates/cmdr-smb/src/volume/foreground_yield.rs`. **Decision/Why a
  lease AND a timestamp:** the share is busy while a foreground operation holds a lease on it, and for
  `TRANSFER_FOREGROUND_IDLE_THRESHOLD` (500 ms) after the last one ends. The two halves are composed in exactly one
  place, `cmdr_fs::volume::host::activity::volume_busy_for_user`; the signal itself lives in `priority::foreground`
  (which owns the full rationale). Each half covers what the other can't. A timestamp alone decays while the operation
  it protects is still running: SMB returns a whole listing in one round trip, so there is no per-batch hook to re-stamp
  from, and a folder that took 10.7 s was protected for its first half-second while the upload competed for the rest of
  the user's wait. A lease alone says nothing about the moment between two listings, which is what turns a burst of
  arrow-key presses into one park instead of one per keystroke. The window stays far shorter than the index scan's 2 s:
  a scan yield merely drops the listing budget, while a transfer yield PARKS, and `FOREGROUND_YIELD_DEBOUNCE` stacks
  another 400 ms on top.

  **Why a LISTING can hold a lease even though an SMB CONNECTION can't.** MTP answers "a foreground op is in flight
  RIGHT NOW" from a per-device gate, because a PTP session is a single scarce resource with an explicit holder. The SMB
  connection has no such holder — every `SmbVolume` clone multiplexes frames over one session, so there is nothing there
  to count, and that is why this signal was time-based to begin with. But a listing is a scoped operation with a
  beginning and an end, so it can hold a claim of its own; the holder is the WORK, not the transport. ❌ Don't reach for
  the connection to make this exact.

  **Decision/Why per-volume scope:** a transfer is work the user asked for and is watching a progress bar for, so it
  must only stand aside for the share it actually contends with; the app-wide signal would park a NAS copy because the
  user clicked around a local folder. Starvation is handled below this layer, so it needs no floor of its own, and a
  lease can't route around it: a streaming write earns forward progress from `MIN_PROGRESS_FLOOR_BYTES`, a single-shot
  write (exempt from the floor) from the per-park hard cap, and what bounds a held lease is in `priority/DETAILS.md` §
  "How long a lease can be held". Pinned by `smb::foreground_yield::tests::*`,
  `cmdr_fs::volume::host::activity::tests::*`, and
  `smb_test::{supports_foreground_yield_is_on, foreground_pending_tracks_navigation_on_this_share_only}`.

**The DESTINATION-side yield (uploads: local → SMB).** The source arm above probes the SOURCE volume, so an upload's
source (a local disk) never opts in and that arm is inert. A SECOND arm
(`CheckpointStream::bounded_yield_to_dest_foreground`, right after the source arm in `checkpoint`) stands aside for the
DESTINATION share instead, so an upload to a share the user is browsing doesn't make the pane sluggish. It fires when
ALL hold: the destination opts in (`Volume::supports_foreground_yield_as_destination()`), not cancelled,
`bytes_yielded < total_size`, the min-progress floor is satisfied OR the write is single-shot (below), and
`dest_volume.foreground_pending().await` is true (the same per-share signal the source arm reads). The floor is
otherwise shared with the source arm; in practice only one arm is active per transfer, since source XOR destination is
the SMB side.

**Decision/Why a SEPARATE opt-in, not `supports_foreground_yield()`.** The read flag can't be reused for writes: an MTP
upload streams chunks inside ONE `SendObject` PTP transaction, so parking mid-write would PIN the device session (the
opposite of the read side, where a bounded window holds nothing between chunks). So MTP must NEVER opt into the
destination flag, and it doesn't (default `false`). SMB writes are discrete SMB2 WRITE chunks with NO oplock or lease
requested (`create_file_writer` → `OplockLevel::None`, no durable context;
`crates/cmdr-smb/src/volume/write_stream.rs`), so a brief park between them is safe. Only `SmbVolume` overrides
`supports_foreground_yield_as_destination() → true`.

**Decision/Why the destination park is HARD-CAPPED (data-safety bound, load-bearing).** The source arm takes
`wait_until_foreground_idle` unbounded, because a read holds nothing scarce between windows. An upload can't: it holds
an OPEN SMB write handle across the park (the wrapped source read sits between two `writer.write_chunk` calls inside the
destination's `write_from_stream`), and under continuous browsing an unbounded park would let that handle sit idle long
enough for the server/OS to reap it (`smb2` logs "idle teardown" when a quiet session is reaped), breaking the transfer.
So `bounded_yield_to_dest_foreground` takes the SAME wait and races it, in one `select!`, against a single
`sleep(DEST_FOREGROUND_YIELD_HARD_CAP)` (`volume/stream_pipe.rs`, 1 s) and the cancel. At the cap it resumes and writes
the next chunk, keeping the handle warm, then re-parks at the next checkpoint if the share is still busy. The share's
own SESSION stays warm regardless (the user's navigation rides it), so the cap protects only the write handle. Resuming
leaves the source offset untouched, so no desync; bytes reassemble exactly.

The cap is a sleep to an instant rather than a condition anyone re-checks, so it can't be missed; the wait resolves on
the EVENT of the user's listing ending, so the upload resumes then rather than up to a tick later; and neither arm can
starve the other, because `select!` takes whichever lands first. ❌ Don't leave the wait without the cap racing it, and
❌ don't raise the cap toward any server idle-timeout. Cancel is the third arm and needs to be: no foreground signal
ever announces a cancel. Pinned by
`volume::strategy::dest_yield_tests::{dest_yield_parks_before_next_write_then_resumes_byte_exact, dest_yield_waits_on_the_share_instead_of_re_asking_it, dest_yield_hard_cap_bounds_the_park_under_continuous_browsing, dest_yield_cancel_while_parked_returns_cancelled_promptly, non_opting_dest_never_dest_yields}`
and `smb_test::supports_foreground_yield_as_destination_is_on`. The wait's own guarantees (no lost wakeup, one computed
sleep for the quiet window's tail) are pinned against a frozen clock in `cmdr_fs::volume::host::activity::tests`.

**Decision/Why a SINGLE-SHOT write skips the min-progress floor.** The floor is counted from zero for every file (each
file gets a fresh `CheckpointStream`), so a file SMALLER than the floor can never satisfy it and yields exactly zero
times. That made the protection weakest where it is needed most: a folder of photos or documents going to a NAS stood
aside for the user not once. The floor exists to keep an OPEN write handle warm, and a single-shot write has none while
it matters: `crates/cmdr-smb/src/volume/write_stream.rs` drains the source fully into a buffer and only then sends
CREATE+WRITE+FLUSH+CLOSE as one frame, so during the drain — which is where every checkpoint happens — nothing is open
on the server. So the arm skips the floor when the destination answers `Volume::write_is_single_shot` for this file, and
a small upload can stand aside. Pinned by
`dest_yield_tests::single_shot_upload_stands_aside_even_below_the_min_progress_floor`, which parks a 200 KiB upload
against the PRODUCTION floor.

**Why the carrier is the raw boolean, ❌ not `WriteStaging::SingleShot`.** `resolve_staging` upgrades only a staged
request, so a `StageOverOriginal` write (a safe-replace) reports `StageOverOriginal` however single-shot it is — the
enum under-reports. Staging is a different question anyway: what the arm needs to know is whether a handle is open,
which is a property of `write_from_stream`'s branch alone. `stream_pipe_file` therefore probes `write_is_single_shot`
ONCE, hands the answer to `resolve_staging` and to `CheckpointStream::new`, and both consumers see the same answer; two
probes could straddle a reconnect and disagree. What makes the answer trustworthy is the backend contract in
`volume/DETAILS.md` § "The single-shot exemption": `write_is_single_shot` and `write_from_stream` branch on the same
predicate.

**The cap still races the exempt park, and must.** The single-shot answer and the write happen at different moments; a
reconnect in between can renegotiate `max_write_size` and put the file on the streaming path, which does hold a handle
open. The cap makes that race harmless, so it is ❌ never made conditional on the answer. Pinned by
`dest_yield_tests::single_shot_dest_yield_is_still_hard_capped_under_continuous_browsing`.

**Deferred: the band between one compound write and the floor.** A file bigger than the negotiated `max_write_size`
(typically ~1 MiB) but smaller than the 4 MiB floor still never yields — it takes the streaming path, so the floor
rightly applies, and it is under the floor, so the floor never clears. Closing that needs a pre-file yield gate in
`stream_pipe_file` (stand aside BEFORE opening the file, where no handle exists yet for any write). ❌ Not built: it
trades upload throughput against browsing responsiveness for every file in a batch, and wants benchmarking on real
hardware first. Pinned as current behavior by
`dest_yield_tests::a_streaming_upload_below_the_floor_still_does_not_yield`.

**Debounce + min-progress floor (load-bearing, named constants in `volume/stream_pipe.rs`).** Each park suspends the
copy, so naive per-window yielding thrashes under rapid nav. Two guards: (1) **debounce** (`FOREGROUND_YIELD_DEBOUNCE`,
~400 ms) — after foreground drains, stay parked until the device is quiet for the window; if a new foreground op arrives
during it, re-park. A burst of listings is served as ONE suspension, not one park per window. (2) **min-progress floor**
(`MIN_PROGRESS_FLOOR_BYTES`, ~4 MiB) — after a resume, the copy must move at least the floor before honoring the next
yield, so continuous foreground nav can't starve the copy to zero throughput. The floor is currently SMALLER than one
read window (`MTP_READ_WINDOW`, 8 MiB), so it's effectively "one window"; re-tune both together on real hardware. The
destination arm exempts a single-shot write from it (§ above); the source arm never does, since a read window is a read
window whatever the destination promised. The floor baseline (`last_resume_offset`) resets at the end of the arm on
every resume. Both durations/sizes are injectable fields on `CheckpointStream` (defaulting to the constants) so tests
set debounce ≈ 0 and a tiny floor for determinism; the production constants are tuned against a real device.

**Cancel-awareness.** A cancel (or a sibling's failure: both fire `state.stop_work`) during an auto-yield must not be
slept through and must not hang. The debounce wait and every `wait_until_foreground_idle` park (both arms) is RACED
against `stop_work.cancelled()` in a `biased` `select!`, because the wait only resolves when foreground drains and a
stop doesn't clear the foreground signal, so it needs its own waker. No tick on this path: the token wakes the park the
moment the work stops. On a stop the arm bails out and lets the next chunk flow to the backend's `on_progress` cleanup
(`LeafProgress::on_chunk`) — identical to a stop while paused. Pinned by
`volume::strategy::tests::auto_yield_parks_before_next_window_then_resumes_byte_exact` (parks without releasing +
byte-exact assembly + single open at offset 0 + op stays Running), `auto_yield_debounces_a_burst_into_one_park` (the
copy stays parked across both listings in the burst), `auto_yield_min_progress_floor_prevents_starvation`,
`auto_yield_cancel_while_yielding_keeps_no_partial`, the regression guard
`non_mtp_source_never_auto_yields_for_foreground`, and
`yield_capable_source_with_no_foreground_pending_never_self_yields` (no self-yield livelock — a yield-capable source
with nothing pending must never park itself).

**Composition with the scan.** A transfer and the index scan both yield to foreground, so foreground always preempts
both; the two background users don't priority-invert (lane budget 1 on the MTP device means the only foreground
contender is a listing/nav/metadata op, never a second transfer). On MTP they share one signal (the device gate); on SMB
they read the same per-volume signal through different thresholds and different responses — the scan throttles its
listing budget (`indexing/network_scanner/scan_pace.rs`, which owns the reasoning for the whole yield-to-navigation
design), the transfer parks. The runtime-level `tokio::task::yield_now()` (worker fairness) is a different layer and
stays alongside this session-level yield.

**Scoped out: the local-FS sync chunk loop.** `chunked_copy.rs::copy_data_chunked` (and the macOS `copyfile` / Linux
`copy_file_range` strategies) receive only the cancel `intent` atom, not the `PauseGate` (which lives on
`WriteOperationState`). So a local→local copy of one huge file pauses only at the next file boundary, not mid-file.
Threading the gate through `copy_strategy.rs` + the native paths is the v2 follow-up; the user-reported case is
MTP→local (the volume streaming path), which is fully covered.

## Retrying one FILE, and the watchdog that ends a wait nothing else will

**Decision**: a transport blip re-runs the FILE from its first byte, up to three attempts with a short cancel-aware
backoff, and the retry lives in `stream_pipe_file` — the single place a file's bytes are streamed — not in any driver
above it. Policy in `retry.rs`.

**Why the file, not the operation**: a single failed write used to end the whole transfer. Twelve files into a 764-file
copy, one write that never came back took the other 752 with it
(`docs/notes/incidents/2026-07-31-transfer-wedge/README.md`). `smb2` surfaces a dead session as a typed error rather
than a hang — a send deadline, a response deadline, and `Error::ServerUnresponsive` for a link that answered nothing at
all — so the file that hit the blip can simply be run again and the batch carries on.

**Why THAT layer, and nowhere higher.** Everything a retry must not redo has already happened above `stream_pipe_file`,
which is what makes the retry safe by construction rather than by care:

- **Conflict resolution** ran on the driver (top-level) or in `copy_directory_streaming`'s merge walk (deep children). A
  retried file re-prompts nobody and re-decides nothing; it re-runs the write the user already approved. The other half
  of that decision — that the answer is not _stale_ either — is deliberate: it was given for this file, in this
  operation, seconds ago, so re-asking would be the surprising behavior, not the safe one. Pinned by
  `volume/copy_retry_tests.rs::a_retried_file_never_re_asks_the_conflict_the_user_already_answered`.
- **The rollback ledger** (`CreatedPaths::record_file`, which stores the destination path AND the byte count it was
  written with — that size is the snapshot the journal row carries, and what makes a leaf inside a copied folder
  reversible), **the journal** (`journal::record_volume_transfer_source`), and **the per-file progress milestone** are
  all driven from the CALLER's `Ok` arm, so a file is recorded exactly once whatever it took. Pinned by
  `a_retried_child_is_recorded_in_the_rollback_ledger_exactly_once`.
- **The merge invariant** is a property of the level walk, and a retry re-walks nothing: it re-runs one child's write
  into a destination the walk already resolved. A retry can never turn a merge into a replace.
- **Overwrite** is untouched, and a retry can only improve it. A safe-replace's swap is its landing, after the attempt
  that got every byte down, so the ORIGINAL is intact through every attempt — including a file that fails all three. A
  cross-type Overwrite's destination sits aside until the operation ends (`volume/displaced_destination.rs`), so a retry
  re-runs only the write and every ending still finds the original to put back.

**Staging across an attempt boundary.** Each attempt re-derives its staging and mints its own `.cmdr-tmp-<uuid>`, so
nothing partial survives an attempt and no byte is written twice. `StagedWrite::abandon_attempt` clears the previous
attempt's target first — one case WIDER than the terminal `abandon`, because between two attempts the next writer is us:
it also removes what an `Unstaged` attempt left at the final name, which is only the partial we just gave up on. Leaving
it would make the next attempt depend on how each backend treats a write onto an existing path (`LocalPosixVolume`
truncates, `InMemoryVolume` refuses with `AlreadyExists`, MTP can make a duplicate object). A `SingleShot` write is
still left entirely to its backend.

**What is retryable, and why by type.** Exactly a transport fault: `VolumeError::transport_fault`
(`crates/cmdr-fs/src/volume/transport_fault.rs`), the ONE classifier the retry, the outage wait below, the
unanswered-create settle (`interrupted_landing::create_went_unanswered`), and the local engine's
`error_classification.rs` all ask, so none keeps its own list. An exhaustive match, so a new variant can't inherit a
retry by falling into a wildcard — adding one forces the decision; its `TransportFault` names the kind (timeout,
connection lost, session reset, stale handle, transport errno). Retryable: `ConnectionTimeout` (where `smb2`'s
`SendTimeout`, response `Timeout`, and `CreditStarvation` all land, via `ErrorKind::TimedOut`), `DeviceDisconnected` (a
session-owning backend can rebuild its session in place, but ❗ NOT inside 1.25 s: once SMB marks a session
`Disconnected` every call fails at once, and nothing in this loop reconnects it. Only a sub-second blip, where the
session never read `Disconnected`, is rescued here; a longer outage is the next section's. On MTP the device really is
gone and each attempt fails immediately, costing a bounded 1.25 s), `DeviceSessionReset` (its own doc says retrying
works), `StaleDestinationHandle`, and an `IoError` whose errno `cmdr_fs::volume::is_transport_errno` accepts (EPIPE,
ECONNRESET, ECONNABORTED, ENOTCONN, ETIMEDOUT, ENETDOWN, ENETUNREACH, EHOSTUNREACH, decided by `io::ErrorKind`) — the
case where a write onto an OS-mounted share reports the blip as an errno rather than a typed backend error; the local
engine maps the same set to `ConnectionInterrupted`. `Cancelled` and `NotConnected` (nothing is dialing it) are never
faults. ❌ Everything else is a decision or a fact about the data: re-running the write fails the same way and only
delays the report.

**❌ A cancel is never retryable, and outranks a retryable error.** `should_retry` checks `work_stopped` first, the
backoff is a `select!` against `state.stop_work` (not a `sleep`), and a cancel that lands while an attempt is failing on
something we WOULD have retried is reported AS a cancel — otherwise the post-loop, which keys `write-cancelled` off a
`Cancelled`-shaped error, would log the user's own click as a failed transfer. A sibling draining after another task's
failure takes the same path: it never starts another attempt, and its stop is dropped in favor of the failure that
started the drain.

**Bounded, deliberately.** `MAX_ATTEMPTS = 3` and a 250 ms → 1 s backoff, so a file can add at most 1.25 s of waiting.
Three is the smallest number that survives the observed shape (a blip takes out the attempt in flight, the next runs on
a session the backend has since rebuilt, the third finds a healthy connection); past two failures the problem isn't a
blip. The bug this whole effort exists to kill is an infinite hang, so a retry loop that can spin forever would
reintroduce it in a new costume — the cap is pinned by call count, not by hope
(`a_destination_that_never_recovers_gives_up_at_the_attempt_cap`).

**Scope: streaming writes only.** `volume/sequential_extract.rs` (a compressed tar / solid 7z source) has its own write
site and deliberately gets NO retry. Its source is a one-pass decoder sitting at a fixed position, so a per-file restart
would mean re-decoding the whole archive prefix — the very O(n²) the sequential path exists to avoid. Its landing does
wait out an outage (§ "Where it's asked, and why only there"), since settling a landing streams nothing. The local-FS
engine (`chunked_copy.rs`) is likewise untouched: a local write that fails with a transport errno is either a dying disk
or a network mount the volume layer should be handling, and neither is improved by retrying inside the sync chunk loop.

### Waiting out a volume that went away (`volume_outage.rs`)

**Decision**: when a transport failure (`VolumeError::transport_fault`) meets a volume whose session reads
`ConnectionState::Disconnected`, the transfer WAITS for the volume instead of spending its attempts in a second: it
reconnects the volume itself on a growing cadence (at once, then 2, 4, 8, 15, and every 30 s), for up to
`OUTAGE_WAIT_BUDGET` (15 minutes, `outage_slot.rs`), then runs the work again with a fresh set of attempts. A volume
that isn't back by then fails the transfer exactly as before (`#398`).

**Why the attempts above can't do it.** Once SMB marks a session `Disconnected`, every call fails at once (`cmdr-smb`'s
`check_connection`), so all three attempts burn inside 1.25 s. And nothing else reconnects it in time: the frontend's
reconnect manager runs only while a pane shows the volume, on a 2/4/8/16/30 s cadence, and the backend's watcher-death
loop starts at 2 s. A router restart or a NAS waking its disks took the whole copy with it, and every queued job for the
same share then started and failed within a second.

**Why the transfer layer, and why no new `Volume` method.** The capability exists already: every session-owning backend
(SMB, SFTP, WebDAV, S3, ADB) answers `connection_state` and rebuilds its session in place, single-flight, on
`attempt_reconnect`. What was missing is PATIENCE that respects an operation: a budget, its Cancel, its Pause, and its
progress UI, and only this layer has an operation. An `await_reconnect` on the trait would mean five copies of one
backoff loop, none of which can see a Cancel. So a backend owns ONE attempt, and the transfer owns the waiting. A volume
with no session to lose (a local disk, an OS-mounted share, MTP) answers `None` and is never waited for.

**The guarantees, each load-bearing:**

- **Bounded.** The volume comes back, the budget runs out, the person cancels, or the backend says time won't fix it
  (`NeedsSignIn`, or an auth refusal from the reconnect: dialing again risks locking the account). A link that flaps is
  ridden out at most `MAX_OUTAGES_PER_ITEM` (3) times per item.
- **Cancelable at once.** Every await races `stop_work`, a reconnect attempt included; a cancel during the wait is
  reported as the cancel it is. A sibling's failure ends the wait the same way (the failure drain), and once the work
  has stopped a transport fault starts no wait at all and reads as the stop (`Cancelled`): a call the share cut off
  while the operation was stopping otherwise reached the person as "Lost the connection" after they clicked Cancel.
- **No single attempt holds the wait.** Each `attempt_reconnect` gets `OutagePolicy::attempt_timeout` (15 s, under the
  cadence's 30 s step); one that hasn't returned is dropped and counts as still down, and the next tick dials afresh.
  Without it, one SMB handshake that never returned held a copy for the whole budget after the NAS was back (`#406`).
  It's a safety net now: SMB's own handshake deadline (`HANDSHAKE_RESPONSE_TIMEOUT` in `cmdr-smb`, 10 s) ends a
  silent-server attempt first with a typed error, pinned under 15 s by a `cmdr-smb` test (`crates/cmdr-smb/DETAILS.md` §
  "deadline and keepalive defaults"). Dropping an attempt is safe: every backend's reconnect is single-flight and
  rebuilds from scratch.
- **Pausable.** A pause parks the wait on the pause gate and dials nothing; a Resume is a person's action, so the wait
  gets a fresh budget from there.
- **One outage per lane**, shared by every waiter on it (`Outages`, one per engine, reached through the op's scope): the
  concurrent window's tasks all meet the drop at once, and they share ONE deadline. A job queued BEFORE an outage gave
  up looks once (the volume may be back, with nothing else dialing it) and then fails promptly, so a queue behind a NAS
  that's really gone doesn't add fifteen minutes per job. A job created AFTER the give-up (a Retry, a new copy) is a
  fresh question and gets a fresh wait. A give-up is forgotten one budget later.
- **Named, not stalled.** The wait sets `state.outage` (`outage_slot.rs`): the activity reads
  `TransferWaitReason::Reconnecting` plus the volume's name on every progress event, `OperationStatus`, and
  `cmdr://state`; the probe heartbeats it to the UI every tick, accrues no stall time, dumps nothing, and restarts every
  task's abort clock (`TaskPhase::WaitingForVolume` is not abortable); and the ETA's working-time clock leaves the
  outage out (`human_wait.rs`), so the first sample after it doesn't divide one file's bytes by the outage's length.
- **A late error is the old session's, and is run again at once.** In the concurrent window one task's reconnect
  installs a new session while a sibling's call is still in flight on the one it replaced; that call's transport error
  then arrives with the volume already live, which used to read as "not an outage" and end the copy (`ERR-VEZUB`, #489).
  So every call snapshots `Volume::connection_generation` BEFORE it runs (`StartedOn`: `through_outages` per call,
  `stream_pipe_file` per step), and a volume that reads live on a NEWER generation answers `Back` with nothing to dial.
  The guarantee: a generation bump is proof of a stale error ONLY while the volume reads live; live on the SAME
  generation stays `NotWaitable` (the error is real), and a backend that answers `None` keeps the old behavior exactly.
  The shortcut spends one of the item's `MAX_OUTAGES_PER_ITEM`, so a flapping link stays bounded, and ❌ never touches
  the lane's outage record (another waiter may be in a newer outage on it). The snapshot over-approximates on purpose: a
  call that snapshotted while the volume was down, then ran on a sibling's fresh session and failed for real, gets one
  re-run it didn't need; that's safe (only re-runnable work reaches these waits) and bounded. Every connecting backend
  answers the generation from the counter its own loss funnel guards on, so the same late error also never moves the
  fresh session's state: SMB (`crates/cmdr-smb/DETAILS.md` § "Which session a loss is about"), and SFTP, WebDAV, S3, and
  ADB through the shared `cmdr_fs::volume::ConnectionCell` (`crates/cmdr-fs/DETAILS.md` § "Three questions a remote
  volume gets asked"; ADB, with no session, counts the times a reconnect brought the device back).
- **A wait that ends the work says it was the connection.** When the wait gives up (`OutageEnd::GaveUp { waited }`, the
  lane outage's length, fixed by the first waiter to give up so every task reports one wait) or has nothing to wait for
  (`NotWaitable`: a sign-in, or the item's outages spent), `volume_outage::lost_connection` reads what it saw: a
  transport fault while a SERVER volume (`BackendKind::is_server`: SMB, SFTP, WebDAV, S3) reads not live. The runner and
  `through_outages` attach that to the `PathedVolumeError` (`lost_connection`), and its mapping words it
  `WriteOperationError::ServerConnectionLost { volume_name, gave_up_after_secs }`, the file as context only. ❌ Never
  from a path or a message. A phone over ADB, MTP, and a local drive keep `DeviceDisconnected` (a device that left is
  not a server to check on), and a server that reads LIVE keeps the error's own words: nothing proves its connection is
  gone. The verdict names the volume it finds out, so a download from a dropped NAS names the NAS. A failure that only
  carries a bare `VolumeError` through the wait keeps its old wording; every transfer call site that waits carries a
  `PathedVolumeError`.

**Where it's asked, and why only there.** Only where a transfer meets the share, and only for calls that are safe to run
again:

- `stream_pipe_file`, a step runner: each pass walks the file's checkpoints (settle an unanswered create, open the
  source, write, land), and a failed step hands its `Fault` to ONE decision, `FileRun::on_fault`, which waits with that
  step's own `StartedOn` snapshot and resumes at the step's checkpoint. The open resumes by opening again (nothing
  staged yet); the write by clearing the failed attempt's temp AFTER the wait, when the delete can reach the share, then
  running the file again; a landing step the transport cut off (`LandingError::CutOff`: a first rename, a claimed name's
  clearing delete, a swap's replacing rename or move aside, the rename after either) by settling it first, on the next
  pass's first checkpoint (`resume_landing`, § "A landing the drop cut off" below): what landed counts as done, a whole
  temp lands without the file streaming again, and only what nothing proves runs the file again. A wait that ends
  without the share gives the landing up under the refusal rules (`CutOffLanding::give_up`: nothing cleared leaves the
  temp to the abandoned-write cleanup; a cleared way rescues the bytes or keeps them recorded as a landing), a cancel
  included, which is reported as the cancel it is. The server-side copy's landing (`server_side_copy.rs`) takes the same
  wait. A new step gets the wait by becoming a `Fault` variant, ❌ never with a `wait_after_failure` call of its own.
  The landed flags stay a best-effort tail after the landing, outside the runner: a flag a drop cut off reads as
  `Refused`, the same as a server's refusal, so it's a counted, reported metadata loss, ❌ never a wait.
- `volume/sequential_extract.rs::land_member`, the one-pass extract's landing: the same wait and settle, outside the
  runner because a one-pass decode can't stream a member again. So whatever would run the file again fails the member
  instead, and with no source to compare against, a cleared-way landing whose temp is gone ❌ never counts as landed.
- `through_outages` around each merge level's prepare (a create whose second run answers `AlreadyExists` and reads as
  "there", plus two listings), around every driver's top-level destination probe (a read), and around the conflict
  resolver's destination type probe (`volume/conflict.rs::resolve_dest_is_directory`, the stat an Overwrite clash asks
  before the swap; pinned by `volume/conflict_tests.rs::a_share_that_drops_as_the_destination_is_probed_is_waited_for`
  and the live Overwrite cell in `smb_outage_test.rs`). ❗ Only a call that's safe to run twice goes through it.
- The start gate, `volume/preflight.rs::wait_for_gone_volumes`, at the top of all three volume drivers: a transfer that
  begins with a volume already gone (a Retry while the NAS is still down) waits before its first step rather than
  failing on it. It announces the wait itself (no probe is registered yet). Only a cancel stops the transfer there;
  every other ending carries on, and a volume still gone fails the first step exactly as before.
- Every rename aside (`volume/displaced_destination.rs::displace_destination`, a cross-type Overwrite's and a fresh
  compress's displaced publish): the rename the transport cut off is settled by one stat of the aside after the wait
  (`volume/DETAILS.md`, the aside bullets).
- Outside the transfer engine, the archive edits take the same wait and settle: a remote zip edit's pull, upload, and
  swap (a `StageOverOriginal` landing), and a fresh compress's production, upload, validation, and publish
  (`../archive_edit/DETAILS.md` § "Remote edit: the data-safety contract" and § Compress).
- After the copy: a cross-volume move's source sweep (`volume/source_sweep.rs::sweep_carried_source`, whole, through
  `through_outages`: every run re-lists and re-stamps before it deletes), and each folder's date stamp
  (`volume/folder_metadata.rs`, through `wait_after_shortfall`, since a backend reports a drop under `apply_metadata` as
  a refused class rather than an error). Both: `volume/DETAILS.md`.

**Not covered, deliberately.** The pre-flight SCAN of a big tree (its own walker, its own failure path), trash, the
local engines, the instant ops (rename, new folder, new file), and a same-volume move's renames past the start gate
(`volume/move_same.rs`, `volume/rename_merge.rs`: a rename the drop cut off can't be told from one the server refused
without a settle of its own). Each fails on an outage as it always did. A volume delete has its own wait, which
re-checks each entry against its walk before deleting again (`../delete/DETAILS.md` § "Waiting out a dropped share"),
and an archive edit's planning walk is listed in `../archive_edit/DETAILS.md` § "The driver, op by op" (its compress
bullet).

**A landing the drop cut off (`interrupted_landing.rs`, `staged_write_landing.rs::CutOffLanding`).** A rename or a
delete whose reply never arrives may have happened or not: the drop can come before the server saw it, or after it
performed it. Run again blind, a rename that happened re-copies the file and its landing meets OUR file at a name the
caller believed free, which `land` rightly refuses as an unresolved clash: a copy that waited out the outage then failed
with `DestinationExists` over its own file. Not rare: the live-SMB cell
(`operations_host/backend_suites/smb_outage_test.rs`) hit it whenever the cut fell inside a landing of the concurrent
window. And after a claimed name's clearing delete that went through, the temp is the destination's ONLY copy. So the
landing stops at the step the transport cut off, and once the share is back it is settled from what the destination
shows (`CutOffLanding::resume`):

- **The temp is there and whole** (exactly the size the write confirmed): it lands from where the cut left off, ❌ never
  streamed again. A first rename lands again from the top. After a clearing delete whose answer was lost, the name says
  what happened: empty means the delete went through and the rename alone finishes it; taken by the very entry the
  landing noted right before its delete (`interrupted_landing::NameHeld`: kind, size, mtime, and node id, from one stat
  in `clear_the_way`) means it never reached the server, and the way is cleared again; taken by anything else means it
  went through and someone saved there since (`#576`). After a delete that went through, ❗ the rename alone,
  `rename(force = false)`, ❌ never another delete: a file someone put at the name during the outage makes it refuse,
  and the new bytes are rescued and reported, their file untouched. After a swap's move aside whose answer was lost,
  the aside says what happened: there means it moved, and the rename alone lands; gone means it never did, and the move
  runs again. After the rename onto the vacated name, the rename again (`rename(force = false)`; a refusal puts the
  original back).
- **The temp is gone**: the landing counts as DONE only on proof, all three of: nothing of ours discarded it (the
  landing notes every discard it asks for, which is why `land` never discards after a transport fault: on a late error
  the delete would reach a sibling's fresh session, find the landed temp gone, and the re-run would meet our own file as
  `DestinationExists`); the temp is gone from the destination, and the only move ever asked of that `.cmdr-tmp-<uuid>`
  was ours; and the name holds what we wrote: a file of exactly our size after a rename-first landing, which cleared
  nothing, and ❗ exactly the SOURCE's bytes, compared in full (`holds_the_source`), after a cleared way or any swap,
  since a swap whose temp someone removed can leave the user's OLD file at the name, maybe at the same size. With no
  source to compare against (the one-pass extract), that case ❌ never counts as landed. A landed swap's aside goes.
- **Anything else** (a temp that isn't whole, a name that doesn't prove it) runs the file again, a swap's original put
  back at its name first (`run_again_with_the_original_home`), and a name someone else took meets the ordinary
  late-clash refusal (`DestinationExists`, their file untouched).

**Decision/Why only a claimed name carries a note of what's at it.** A claimed name's landing EMPTIES the name before
its rename, so after a lost delete reply a file at the name may be one someone created there afterwards, which the
person never chose to replace. A swap never empties the name before the replacement is whole at it (the one-step
replace, or the original moved aside first), so its resume can't meet a newcomer at a name we emptied: a whole temp
after a cut-off one-step replace or move aside means nothing of ours happened, and what's at the name is the original,
or a save over it during the outage. Replacing that save is the Overwrite's own window between the conflict and the
landing, the same one a save during the copy falls into, and the outage only lengthens it. Closing that window would
need the conflict's view of the original carried down to every swap site and checked at every landing, ❌ not at the
resume alone.

A step of the settle that meets the transport again cuts the landing off again (bounded by `MAX_OUTAGES_PER_ITEM`); one
that meets a refusal settles by the refusal rules. A Cancel, a give-up, or a drop that ends the wait AFTER a claimed
name's way may have been cleared leaves the temp recorded as a landing, rescued when the share answers and kept for the
next sweep that reaches it when it doesn't (`Landing`'s `Drop` hands a dropped one to the volume's next arrival), ❌
never cleaned as a partial. One that ends a swap after its move aside puts the original back when the share answers, and
otherwise leaves the aside recorded for the volume's next arrival, whose sweep puts it back (or removes it, once the
name holds the replacement whole); the temp, never the only copy, goes with the abandoned-write cleanup. ❌ Don't loosen
any proof to "something is at the name": a file claimed by mistake enters the rollback ledger as ours, and an undo
deletes it. No mtime check: whether a backend keeps the source's mtime, and at what precision, varies, and the temp's
fate already proves whose file it is. Pinned at all five safe-replace sites by `volume/swap_outage_tests.rs` (a drop
before the move aside, after it, at the rename after it on either side, the temp removed with the old bytes at the same
size, a stale error on the move aside, a give-up, and a cancel whose original the arrival sweep puts back), plus a
claimed name's stale clearing delete there, and a server-side copy's landing in
`volume/strategy_server_side_copy_tests.rs`.

**A single-shot write the drop cut off.** A write with no temp (`WriteMode::CreateNew` at the final name: SMB's compound
CREATE+WRITE+FLUSH+CLOSE, which on a fresh connection carries any file up to 512 KiB and on a measured fast link up to
`max_write`) has the same ambiguity, and it is the live cell's commoner failure: the rerun's `CreateNew` meets the file
the server already completed. There's no temp to read the answer from, so when an attempt's `CreateNew` fails on the
transport (`interrupted_landing::create_went_unanswered`), the NEXT attempt (after a blip retry or an outage wait) first
runs `settle_unanswered_create`: the file counts as done only when the name holds a file of exactly our size whose bytes
equal the source's, compared in full by re-reading both. ❌ Never on size alone (a same-size file someone saved stays
theirs, pinned), and never on mtime: the compound write stamps the source's dates in a separate request after the frame,
which an unanswered write never reached. Anything less runs the file again and meets the ordinary clash, which also
covers a 0-byte file our CREATE made without its WRITE: it can't be told from someone else's empty file, so it's
reported, never claimed. A settled create gets its stamp again: the source's date and what the destination keeps on a
write (staged flags only) go on by path through `Volume::apply_metadata` (idempotent), read off the settle's own source
stream, and what's refused joins the loss note, ❌ never dropped silently. The SMB write fails its compound path with
`DeviceDisconnected` when the stamp itself met the drop (`crates/cmdr-smb/DETAILS.md` § "Dates on copies"), so this is
the path that puts its dates on. Pinned by
`copy_outage_tests.rs::a_settled_single_shot_write_gets_the_date_its_stamp_lost`.

**Residual risk, accepted.** A byte-identical file someone else saved at the name during the outage is claimed as ours,
so it enters the rollback ledger and an undo would remove it. Content-identical to the source and saved in the seconds
the share was away, at a name that was free when the copy looked: we accept that over failing every NAS copy whose reply
a drop swallowed.

Pinned by `volume_outage_tests.rs` (the state machine: back, give-up, cancel, pause, sign-in, a queued job after a
give-up, a Retry after one, a shared deadline, `through_outages`'s cap, a hung attempt abandoned, a cancel dropping one,
the queued job's one look bounded too, and a late error: waited past at once on a newer generation, not on the same one,
and capped) and `volume/copy_outage_tests.rs` (the real copy pipeline: both drivers, the give-up, a cancel during the
wait, the start gate, a landing cut off before the server saw it, after it performed it, and after it performed it with
the name then taken by someone else, a single-shot write the drop swallowed, with the name then taken by a
different-size and a same-size file, and a late error in the concurrent window). `volume/copy_landing_race_tests.rs`
pins a performed landing answered by a late error landing once, in both drivers.

### The watchdog ACTS

**Decision**: the mechanism to end a wedged wait is built and wired, and its trigger is **gated on positive evidence
that the connection is dead** — `Volume::connection_liveness() == Some(Dead)`, AND `STALL_ABORT_AFTER` (180 s) of zero
byte movement inside a backend call. On both, the watchdog trips the task's `stall_abort` token, the streaming write
races it, and the park becomes a typed `ConnectionTimeout` the retry above treats as a blip.

**Status: the teeth are ON for SMB, and inert everywhere else.** `SmbVolume::connection_liveness()` answers from
`smb2`'s own pollable reading (`Connection::liveness()`, 0.23.0+), read off the handle the share swaps in lockstep with
its client (`crates/cmdr-smb/src/volume/liveness.rs`, which owns the mapping). Every other backend answers `None`, so
for them the watchdog dumps the in-flight table, heartbeats the UI's stall signal, and acts on nothing.

**Why gated, and why elapsed time is not allowed to be the evidence.** Telling "slow but alive" from "dead" needs a
keepalive: an ECHO the server either answers inside a window or does not. A silence deadline ALONE cannot do it, because
a large write to a loaded spinning-disk NAS is legitimately slow, and killing it trades a rare wedge for frequent
spurious failures — the worse bargain. Inventing a verdict out of elapsed time would reintroduce, one layer up, the
failure mode the keepalive exists to prevent.

**Why `smb2`'s reading qualifies where its earlier signals didn't.** `Liveness::Unresponsive` is the conjunction the
crate's own `Error::ServerUnresponsive` rests on (keepalive armed, a request outstanding, NOTHING on the wire for
`LIVENESS_WINDOW_PROBES × keepalive_after`, 15 s by default), but pollable: readable before any request has burned its
deadline, and reading it tears nothing down. Every inbound byte counts as it lands, the bytes of a response still
arriving included, so a large read trickling over a slow link reads `Alive` rather than silent (verified by `smb2`
0.23.0's `a_response_still_arriving_is_not_silence`, 2026-09-22). The mapping:

- `Unresponsive` → `Dead`; `Alive` → `Alive`.
- `Idle` and `Quiet` → `None`: nothing outstanding, or silence that proves nothing yet (too short, or no keepalive).
- `Disconnected` → `None`. A torn-down connection has already failed every waiter on it, so the task this watchdog would
  unstick has its error in hand and the retry above has it; a `Dead` would arrive strictly after that error.
- ❌ Still never from a missed probe (`MetricsSnapshot::keepalive_failures` counts non-events: a real NAS drops ECHO
  precisely while it writes), `keepalive_probes_skipped`, or a rising `sent_age`.

**What the reading can't see.** It covers the share's MAIN session, which every copy rides through `clone_session`. A
task still parked on a connection a reconnect has since replaced is answered for by the new session; the old one is torn
down, which fails that task's request on its own. A tree `smb2` resolves through a DFS referral to another server lives
on an extra connection `SmbClient::connection()` doesn't return, so a copy riding one reads no verdict (`None`), which
is the safe default.

❌ Do NOT drop the stillness window and trust the verdict, and do NOT assume the 180 s can come down now: with a
fallible verdict the debounce is doing real work rather than just waiting, so it is a tuning call to make against the
keepalive's measured false-positive behavior. Pinned from both sides by
`transfer_probe::tests::{a_live_connection_with_a_flat_byte_bar_is_never_aborted, the_watchdog_acts_once_a_still_task_s_connection_reads_dead}`:
an `Alive` connection holds the abort off for a task whose bar has been flat for a minute (the ERR-CNK7M shape, a small
SMB file in one compound read), and the first tick that reads `Dead` past the window acts.

**Why the two conditions are ANDed, and why that is load-bearing rather than belt-and-braces.** The liveness verdict
this gate reads is a keepalive result, and a keepalive false-positives under exactly the load a transfer creates.
Measured against David's QNAP TS-464 (2026-08-02, `smb2`'s live-hardware suite): under heavy write load an ECHO probe
reported **`2 answered, 1 unanswered`** — a `Dead` verdict on a NAS that was demonstrably fine — while **five
consecutive runs on the same idle box reported `0 unanswered`**. So the signal is least trustworthy precisely when it
matters, and acting on it alone would kill healthy transfers to busy servers: the failure mode this whole gate exists to
prevent, one layer up.

The stillness window is what makes the pair sound. A NAS that drops a probe because it is busy writing has not ALSO
moved zero bytes for `STALL_ABORT_AFTER`; a genuinely dead session has done both. Each condition covers the other's
false positive. ❌ **Never collapse this to "trust the `Dead` verdict"** — the comment at the conjunction in
`transfer_probe.rs` carries the same warning, and `transfer_probe::tests::a_task_that_keeps_moving_is_never_aborted`
pins it (its probe reports `Dead` on every tick and the moving task is still never touched; removing the movement check
turns that test red).

**Why 180 s for the second condition.** It is a LAST resort even once armed: a backend that can bound its own waits gets
there first, so a dead SMB session errors on its own and the file's retry picks it up without the watchdog being
involved. Which `smb2` deadline actually does that, and why `SEND_TIMEOUT` is not the one: the doc comment on
`STALL_ABORT_AFTER` in `transfer_probe.rs`. What is left is the case with no deadline anywhere: an OS-mounted share, a
USB stack, a future backend that forgot. The number clears the slowest HEALTHY gap between two byte reports, which is
one chunk: a 1 MiB SMB read window needs a link under 6 KB/s to take this long, an 8 MiB MTP window needs USB at 45
KB/s.

**The guards, each load-bearing** (the liveness verdict above is the first and most important; these are the rest):

- **Per-task, not per-operation.** A batch where any task still moves bytes leaves every other task's clock running on
  its own merits.
- **Only `OpeningSource` / `Streaming`** (`TaskPhase::is_abortable_on_stall`; `CopyingOnServer` is reported only, see
  its paragraph above). Every park is deliberate and self-limiting — a pause ends when the user resumes, a source yield
  when foreground drains, a destination yield at its hard cap, a conflict when the human answers, a backoff on its timer
  — so aborting one would break something working as designed. A deliberate park also RESTARTS the clock, so parked
  seconds are never charged to the budget.
- **Never once the work stopped.** Cancel, rollback, and the failure drain own their teardown via the driver's drain
  deadline; a second abort path racing them would only make the stop harder to reason about.
- **Never for a `SingleShot` write** (the arm isn't even armed in `stream_pipe_file`). Those land in one indivisible
  frame at the file's FINAL name, and only the backend can tell "the server created the file and then refused the bytes"
  from "the file was already there". Aborting one from outside would add a client-initiated instance of the transport
  hazard the single-shot exemption already documents as unfixable.
- **Re-armed per attempt** (`TaskProbe::arm_stall_abort`, which also restarts the stillness clock). One task copies one
  top-level source, which for a directory is many files with their own attempts; a token that stayed tripped would abort
  every remaining child instantly and turn the retry budget into three no-ops.

**Residual risk, accepted (an abandoned write handle).** Ending the wait drops the destination's `write_from_stream`
future mid-write, so an SMB `FileWriter` goes away without `finish()` or `abort()` and its handle leaks until the server
reaps it on idle timeout. The `abandon_attempt` delete of the staged temp may then hit a sharing violation and leave a
`.cmdr-tmp-*` behind. This is the same trade the driver's drain-deadline abandon already takes, and it is narrower here:
the abort only fires on a path that has been silent for three minutes, so the session holding that handle was not
working anyway. What it cannot cost is data at a real name — the write was staged, so the user's filename was never
involved.

**The worst case, stated.** A file on a proven-dead path would be aborted, retried, aborted, retried, aborted: three
`STALL_ABORT_AFTER` windows plus the backoff, so **about nine minutes** before the operation reports the failure.
Bounded where the incident was not (it needed a force-quit), and the UI heartbeats "stalled" throughout, but it is the
obvious knob to tune: cap the stall-aborts per file at one, or shorten the window against the keepalive's own. On SMB it
is rare in practice, because a session that reads `Dead` for long also fails its own requests
(`Error::ServerUnresponsive`) well inside 180 s, and the retry gets there first.

**A cancel does not reach a wedged write itself; a second tier does.** The backend learns about a cancel through its
`on_progress` callback, which a wedged write never calls, so on the SERIAL path a Cancel is only observed once the write
returns. That is what § "Two tiers of cancel" below exists for. The concurrent path bounds it independently too, by
dropping its in-flight futures at the drain deadline.

The gate itself is pinned by `transfer_probe::tests::a_connection_with_no_liveness_verdict_is_never_aborted` — a volume
answering `None` is never acted on however long it stays still, while the watchdog keeps reporting. That test is the one
guarding against a future change quietly re-arming the teeth on a timer; ❌ don't delete it. The tests that DO exercise
the abort script the verdict through `liveness_test_support::ScriptedConnectionVolume` (or its
`dead_connection_volume()` shorthand), because the one real backend that answers needs a server to.

Also pinned by
`transfer_probe::tests::{the_watchdog_ends_the_wait_on_a_task_that_stopped_moving, a_task_that_keeps_moving_is_never_aborted, a_deliberately_parked_task_is_never_aborted, time_spent_paused_does_not_count_toward_the_abort, the_watchdog_stands_down_once_the_operation_is_cancelling, a_new_attempt_gets_a_fresh_signal_and_a_fresh_budget}`
and, end to end, `volume/strategy_retry_tests.rs::the_watchdog_ends_a_wedged_write_and_the_file_runs_again` (a write
that never returns, never errors, and never reports a byte; the watchdog ends it and the retry lands the file).

How progress stays honest across a retry, on both drivers: `transfer_driver/DETAILS.md`.

### Seeing it in a log

A silent retry would make "this file took three tries" and "this file never happened" the same log line, so every
attempt is visible: a `warn` per retry with the attempt number, the error, and the wait; an `info` when a file lands
past attempt 1; an error-level line with the full task row when the watchdog ends a wait; `TaskPhase::WaitingToRetry`
naming the backoff; and `retries=N` (plus `stall-aborts=N`) on every task row in `render_dump`.

### Not done here: continuing PAST a file that fails every attempt

A file that exhausts its attempts still ends the operation, exactly as before. Skipping it and carrying on would need a
terminal event shape that can say "finished, with N files missing", a frontend that shows which ones, and journal
semantics for a partially-successful op — and, more importantly, a product decision about whether a user wants 700 files
copied with three quietly absent. That is a bigger change than the retry policy asked for and a worse default to guess
at, so it is deliberately left for David to call.

## Two tiers of cancel

**Decision**: stopping a transfer has two tiers, and they are not the same signal with different urgency. Tier 1 asks
the backend to stop; tier 2 stops waiting for it. Tier 1 is the default for every user-initiated cancel and tier 2 is ❌
unreachable from anything a person clicks.

**Tier 1, cooperative (`state.backend_cancel`).** The cancel travels to the backend through the per-chunk `on_progress`
callback, and the backend answers by dropping its own handle and deleting its own partial
(`crates/cmdr-local/src/local_posix.rs`, `crates/cmdr-smb/src/volume/write_stream.rs` — the `writer.abort()` +
`delete_partial()` sequence). Writes are ❌ deliberately NOT raced against this token: dropping the write future would
skip exactly that cleanup, on the healthy path, for every cancel. Every Cancel button, the queue window's stop, a
Rollback, and the frontend teardown walk are tier 1 and stay tier 1.

The transfer's own checks (`on_progress`'s `LeafProgress::on_chunk`, the retry and outage waits, the walkers' next
child, the concurrent driver's spawn loop) don't read `backend_cancel` itself: they read `state.stop_work`, its child
token. A person's stop fires it through `backend_cancel`, and the concurrent driver's first failure fires it alone (§
"Cancel and rollback reach a parked driver"), so a failure stops a sibling's write through this same tier-1 path while
the backend's own token, which MTP bridges into the device's cancel, stays quiet. ❌ Never hand `stop_work` to a
backend; the intent, not the token, says which of the two it was.

**Tier 2, hard abort (`state.backend_abort`).** What tier 1 cannot do is bound the WAIT. A write that never calls
`on_progress` never sees the cancel, and `smb2`'s own deadline for a chunk is 30 s of server silence, stretching to 180
s while an ECHO keeps proving the connection alive (see the `STALL_ABORT_AFTER` doc comment in `transfer_probe.rs`), so
one chunk can hold a quit for minutes. Tier 2 is a deadline holder's answer: `volume/stream_pipe.rs::stream_pipe_file`
races the source open and the destination write against it, so the wait ends on our clock whether or not the backend
comes back. Fired only by `state::abort_write_operation` / `abort_all_write_operations`, which today means the quit
deadline and nothing else. Both fire tier 1 first — an abort is a cancel that ran out of patience, never a cancel with a
shorter fuse — and both work on an already-`Stopped` operation, which is the real sequence (cancel, wait a beat, abort
what's left).

**What tier 2 costs, and where the cleanup goes instead.** The backend runs none of its own cleanup, and ❌ nothing may
go back through that connection to tidy up: the delete would hold the very deadline the tier exists to keep, a second
time. So the staged `.cmdr-tmp-*` stays registered in both `in_flight_temps` ledgers and the next launch's ledger claim
(`Operations::claim_ledger`) removes it, with no age gate, off the launch thread.
`volume::cleanup::clean_abandoned_staged_writes` stands down for the same reason, and so does a background cleanup pass
already running (`leftover_cleanup.rs`, stopped by the quit too). Nothing is ever left at a real name, because the write
was staged.

**Why the two phases it races are the source open and the write, and no others.** They are `TaskPhase::OpeningSource`
and `Streaming`, the same two the stall watchdog considers abortable, and for the same reason: every other await on the
path is either a deliberate, self-limiting park (a pause, a foreground yield, a conflict prompt, a retry backoff) or
short enough that the drain deadline covers it. The LANDING rename is deliberately left un-raced, so a task wedged there
is ended by the driver's drain deadline rather than by an abort mid-rename.

**Armed for a single-shot write too**, unlike the stall watchdog. The watchdog's exemption exists because an abort there
would be a _client-initiated_ instance of the transport hazard, on a path that is otherwise fine. Tier 2 only ever fires
when the process is seconds from exiting, so dropping one indivisible compound frame produces exactly what the process
dying produces — the case `volume/DETAILS.md` § "The single-shot exemption" already accounts for.

**Cost on the happy path**: two already-live atomics polled per wakeup of the write future. No allocation, no timer, no
syscall, no backend change, and ❌ nothing added per chunk (the `select!` wraps the whole `write_from_stream`, which the
chunk loop lives inside).

Pinned by `volume/strategy_abort_tests.rs`: an abort ends a wedged write and a wedged source open promptly, never
retries the file, and leaves the partial registered for the sweep — plus
`an_ordinary_cancel_still_routes_through_tier_one_and_the_backend_deletes_its_own_partial`, which is the guard for the
invariant above and asserts all three of "the backend removed its own partial", "`backend_abort` never fired", and "no
temp left behind". `state_tests.rs` carries the negatives at the state layer
(`an_ordinary_cancel_never_fires_the_hard_abort`, `cancel_all_never_fires_the_hard_abort`).

## Cancel and rollback reach a parked driver

**Decision**: the concurrent copy driver observes its work stopping on the await it actually sits on, and bounds how
long a stopped operation waits for its tasks. A failure stops the work the same way a cancel does: the window DRAINS, it
never drops.

**Why**: `state.intent().is_cancelled()` was consulted only in the spawn loop (`while in_flight.len() < concurrency`). A
driver whose window is full — or whose sources have run out — parks on `in_flight.next().await` and never returns to
that line while its tasks are parked. On 2026-07-31 that made Rollback a no-op: the intent was set, `write-cancelled`
never came, the dialog would not close, and the app had to be force-quit. An escape hatch that only works while the
thing it is escaping is healthy is not an escape hatch.

**How.** The `in_flight.next()` await is a `biased` `select!` against `state.stop_work.cancelled()`. `stop_work` is the
right signal because it's a child of `backend_cancel`, which `cancel_write_operation` fires on EVERY transition out of
`Running`, so one arm covers Cancel and Rollback, and the driver fires it itself on its first failure; `biased` makes
the stop win deterministically, and a task result that was also ready is simply re-polled on the next pass. Observing
the stop arms `drain_deadline`; from then on the same await is a `timeout_at`, so every task gets to stop or finish
(their results keep flowing through the normal handler, which is what keeps the rollback ledger complete), while the
drain deadline caps the wait.

**Drain, don't drop, on a failure** (`copy_concurrent.rs::ConcurrentDriver::keep_the_first_failure`). The first failed
task (or a conflict resolution that refused) calls `WriteOperationState::stop_work_after_failure`: it fires `stop_work`
and nothing a person's stop would also say, drops a sibling's pending Stop-mode prompt (its waiter reads a stop), and
wakes a paused, parked task so it can stop rather than sit on the pause until the deadline. From there it's the cancel's
shape exactly: the driver prepares no more sources, a sibling mid-stream stops at its next chunk (its backend runs
`abort()` + delete, tier 1's cleanup), the merge walkers start no next child, an outage wait or a retry backoff ends,
and a sibling inside its landing (no chunk left to stop at) finishes it and is recorded. A sibling in a swap landing's
outage wait ends like any wait and gives the landing up under the cut-off rules (`CutOffLanding::give_up`: rescue, or
leave it recorded as a landing, ❌ never cleaned).

- **The failure stays the failure.** The intent stays `Running`, so the post-loop's cancel reclassification never fires,
  and the terminal event is `write-error` with the FIRST failure. Every later one, a sibling's `Cancelled` from the
  drain included, is logged and dropped.
- **Why a token, not the intent.** The intent is what tells the two apart (rollback or not, which terminal event); a
  failure that wrote `Stopped` would read as the person's click. Every place that stops for EITHER reason reads
  `stop_work` (sync: `work_stopped`); every place that must tell them apart keeps reading the intent.
- **A Rollback clicked during the drain** still rolls back, and the operation ends as that one rollback: the post-loop
  drops the failure after `write-cancelled`, so no `write-error` follows it as a second terminal event.
- Only the concurrent driver drains its window: the serial path and `move_cross.rs` run one source at a time, and a
  directory's own walker already waits out its leaves on an error (`volume/DETAILS.md` § "One window for the whole
  operation").

**The deadline belongs to whoever asked for the stop** (`copy.rs::drain_deadline(aborting)`): 15 s for a cooperative
cancel or a failure, because a task that hasn't answered yet might still come back with its own cleanup done; 1 s once
the hard-abort tier is live, because that caller has already given up on exactly that and is holding a promise to a
person. It stays re-negotiable WHILE it runs: the quit sequence cancels first and aborts a beat later, by which point
the driver is already sitting on the cooperative deadline, so tier 2 has its own `select!` arm that cuts it short. That
arm is latched (`drain_shortened`) because a fired token is ready forever and an unguarded arm would spin.

**When the deadline fires** the driver reports `OperationErrorReport::WindDownAbandoned` (after a person's stop) or
`FailureDrainAbandoned` (after its first failure), which the app logs at error level with the full `transfer_probe` dump
— naming every task and what it is awaiting — and breaks. `ConcurrentDriver::finish` then aborts the window (the tasks
are spawned into a `JoinSet`) and waits up to `ABORTED_TASKS_GRACE` (1 s) for them to stop, so each one's teardown (a
backend writer's abort, its probe row) runs before the post-loop sweeps what it left; a result that raced the abort is
recorded like any other, since it carries the only copy of its ledger. A task stuck INSIDE a poll can't be stopped by
anything, and the driver lets go of it after the grace rather than freeze with it. The staged partials are removed by
`clean_abandoned_staged_writes` in the post-loop, or by its background pass when the destination can't take them right
then (§ "File writes are staged").

**Decision/Why abandon rather than hold**: an abandoned task can leave an open SMB write handle behind. Servers reap
idle handles on their own, and the alternative is the incident's outcome — the user's only way out being a force-quit,
which is what cost them two files. Getting the user unstuck outweighs some server-side handle churn. Nothing healthy
reaches the deadline (a task observes the cancel within one chunk), so hitting it is genuinely news and is logged that
way.

**Not covered**: the SERIAL path has no equivalent gap — it awaits each per-file transfer directly, so there is no
"driver parked while tasks run" state — but a serial transfer wedged inside a backend call still has no escape. Bounding
that would mean racing every per-file transfer against a timeout, which risks killing a healthy slow transfer, so it is
deliberately not done here.

Pinned by `volume/copy_cancel_tests.rs`: cancel and rollback each reach a driver parked on genuinely wedged tasks and
return, rollback undoes the file that already landed, a task that never winds down is abandoned at the deadline with
nothing left at a real name, and a conflict prompt refused mid-copy (the resolver's `Cancelled`, reaching the post-loop
through `ConcurrentOutcome`) sweeps the wedged tasks' partials and fires `write-cancelled`. The failure drain by
`volume/copy_failure_drain_tests.rs` (each sibling's backend removes its own partial and the event is the first failure,
a sibling that never stops is abandoned at the deadline, a paused copy's siblings wake to stop, a Rollback during the
drain ends as one rollback),
`copy_concurrent_driver_tests.rs::a_sibling_landing_when_another_source_fails_lands_and_is_recorded`, and the token
itself by `state_tests.rs` (§ "the stop-work token").

## Key decisions

**Decision**: `copy_volumes_with_progress` scan phase calls `scan_for_copy_batch` once instead of `scan_for_copy` per
source **Why**: Network-backed volumes (SMB) pay 1 RTT per top-level source in the scan phase. A per-source loop
serializes those: for 100 tiny files at ~60 ms RTT, ~5 s of pure stat latency before the copy phase can start.
`scan_for_copy_batch` surfaces both the aggregate (file/dir counts, total bytes) and a per-path vec (is_directory, size)
in a single trait call; the copy engine folds the per-path vec into its `source_hints` map and skips a per-source
re-stat. `SmbVolume` overrides `scan_for_copy_batch` to pipeline N stats over one SMB session; measured 6.5× wall-clock
win at 100 files (6.11 s → 947 ms) on a Tailscale link. `LocalPosixVolume` / `InMemoryVolume` inherit the default serial
per-path loop; it's cheap for them. See `docs/notes/phase4-rtt-investigation.md`. Scoped to the scan phase, which wants
the tree: ❌ the conflict pre-check deliberately does NOT use this method, because a batch of exactly one path takes a
fast path into `scan_recursive` on SMB and SFTP and walks a single directory source's whole subtree. It stats top-level
paths instead (`commands/file_system/volume_copy.rs`'s `stat_source_paths`).

**Decision**: a LOCAL volume's `max_concurrent_ops()` doesn't bound a REMOTE peer (`transfer_concurrency` in
`volume/copy.rs`); the 32 ceiling stays. **Why**: measured, not argued
(`operations_host/backend_suites/copy_concurrency_bench.rs` against a real QNAP over direct smb2 at ~3.7 ms RTT, plus
Docker Samba for corroboration; tables in `docs/notes/transfer-concurrency-window-bench-2026-08-02.md`). The two sides
report that number for unrelated reasons: `LocalPosixVolume`'s `clamp(logical_cpus / 2, 4, 16)` is a guard-rail against
spawning hundreds of tasks and says nothing about a peer, while `SmbVolume`'s IS the `network.smbConcurrency` setting
and `MtpVolume`'s 1 is a single USB bulk transport. A plain `min()` let the guard-rail win on every Mac Cmdr ships to (8
on a 16-core M3 Max, 4 on an 8-core Air), so a setting advertised as 1-32 did nothing above 8. Worth 25% on an 8-core
Mac (window 4 → 10, 4.700 → 3.522 s, spreads disjoint) and nothing measurable on a 16-core one — it is a defect fix, not
a tuning change, and anyone measuring it on a high-core Mac will correctly see no difference and be wrong about the
machines most users have. `Volume::operations_are_local()` (default `false`) carries the distinction; a remote cap
always binds, in both directions, which is what keeps MTP's 1 routing a phone to the serial driver. ❌ Don't collapse
this back to a `min()`, and ❌ don't "simplify" it by raising `LocalPosixVolume::max_concurrent_ops()` instead — that
number also governs local→local copies, which nothing has measured. The 32 ceiling stays because the NAS plateaus at 12
on both corpus shapes, so it is nowhere near binding on the deciding target; only Docker keeps climbing past it, and
that is the loopback artifact below.

**Decision**: nothing sizes the window off `smb2`'s credit budget, and there is no credit-based cap to add. **Why**: the
two count different things. Credits are connection-wide flow control over write FRAMES, granted and spent per request
across everything sharing the session, while `transfer_concurrency` counts concurrent FILES for one `source → dest`
pair. So there is no file-level number a credit budget could produce, and the choice between the two was never a choice.
Credit starvation is already answered where it lands: it surfaces as `ErrorKind::TimedOut`, which
`VolumeError::transport_fault` treats as a retryable `ConnectionTimeout` alongside the two smb2 deadlines. Credits stay
a diagnostic: `SmbVolume::session_credits` for the soak suite (a leak bleeds it down between iterations), and
`CreditInfo` in the SMB diagnostics dashboard.

**Decision**: the window was NOT the throughput story; the per-file destination pre-check was, and it is now skipped for
a destination directory the operation created and answered from one listing for a merge (see "Answering the pre-check
from one listing" above). **Why**: on the NAS the curve is flat from window 12 up (12/16/24/32 medians 3.302 / 3.245 /
3.278 / 3.224 s for 500 × 16 KiB, spreads overlapping) and 8 → 16 buys only 1.16×; the few-large shape is link-bound
(~97 MB/s, saturated gigabit) and shows no measurable difference at any width from 4 up. What the flat part IS: the
concurrent spawn loop's `dest_volume.get_metadata(&dest_item_path)` runs once per top-level source ON THE DRIVER before
the task is spawned, so a batch carries a hard floor of `N × RTT` no window width can overlap. Measured directly
(`serial_precheck_floor`, outside the driver): **2.378 s of a 3.224 s best run, 74%**. ❌ Don't reach for the window
formula to make transfers faster; the ceiling was never there.

**Decision**: the window's WIDTH formula is unchanged, but the window now reaches inside a directory source — one
`merge_ctx.rs::FileWindow` per operation, shared by every merge walker and by the concurrent driver's top-level file
tasks. **Why**: the driver only ever fanned out across TOP-LEVEL sources, and selecting one folder is one source, so a
folder copy streamed strictly one file at a time however wide the setting was. It is a reach problem, not a formula
problem — which is why the width still comes from `transfer_concurrency` and there is no second number. ❌ Never a
window per level or per source (`W` sources × `W` leaves is `W²` files on one connection). Full account, including what
deliberately did NOT change and the progress-bar bound it accepts: `volume/DETAILS.md` § "One window for the whole
operation".

**Gotcha**: a Docker SMB number is a correctness and regression signal, NOT a latency signal. **Why**: on loopback the
per-file probe costs 492 µs instead of 4.76 ms, so it never becomes the ceiling and the extra window keeps buying
parallel work — Docker many-small improves all the way to 32 where the NAS plateaus at 12. A Docker-only sweep therefore
recommends "widen the window, it's still climbing", for a reason that doesn't exist on a real network. Same code, same
harness, opposite conclusions. Anything whose cost is "one round trip per item" needs a real network before it means
anything.

**Decision**: `drive_transfer_serial_async` bounds its closures as
`for<'a> FnMut(...) -> Pin<Box<dyn Future<...> + Send + 'a>>`, not `AsyncFnMut(...) -> T`. **Why**: Production callers
live inside a spawned task (`host.runtime().spawn`, see `volume::copy::copy_between_volumes`), so the driver's returned
future must be `Send`. `AsyncFnMut`'s HRTB-bound `CallRefFuture<'a>` is not provably `Send` for all `'a` when the
closure body captures `&Arc<...>` or similar refs — the compiler emits "implementation of Send is not general enough"
because it can't discharge `for<'a> CallRefFuture<'a>: Send` (rust-lang/rust#100013-class). The explicit boxed-future
shape moves the Send obligation inside the per-call return type, where it's discharged at each call site, and `+ Send`
on the trait object is what makes the driver's awaiting-this-future state Send. An `async ||` + `AsyncFnMut` shape
passes the driver's own `#[tokio::test]`s but breaks at the spawn boundary on real callers, so don't reach for it.
`transfer_driver_async_tests.rs::driver_future_is_send_across_spawn` pins the contract by routing the driver call
through an explicit `tokio::spawn` boundary.

**Decision**: `transfer_driver.rs` ships as two sibling entry points (sync + async), not one
generic-over-AsyncFnMut-or-FnMut driver. Conflict resolution lives in the driver for the async path, in the closure for
the sync path. **Why**: `copy_files_with_progress_inner` is sync inside `spawn_blocking`; the three volume ops are
async. A single generic driver would either force the sync path through a `Pin<Box<dyn Future>>` per source (allocation
per call, no real benefit since the I/O is sync) or use a trait so gnarly that the closures stop reading as
straight-line transfer code. Two siblings share `TransferContext`, `TransferOutcome`, `TransferLoopOutcome`, and
`build_pre_skip_set` / `emit_progress_and_status` helpers — the duplication is small. For conflict resolution: local-FS
conflicts surface mid-flight at parent directories inside `copy_single_item` (a file blocking `create_dir_all`), which
the driver can't pre-detect via top-level `dest.get_metadata`; so the sync driver delegates conflict resolution to the
closure entirely. Volume ops have only top-level conflicts that always reduce to `resolve_volume_conflict`, so the async
driver owns that dispatch (uniform shape across all 3 volume ops, exactly what we want to deduplicate). The data-safety
contract (closure never invoked for pre-skipped / resolved-as-Skip / post-cancel) is enforced in both shapes by the
driver's loop structure and pinned by the `transfer_driver_*_tests.rs` suite. Volume copy's two paths are sibling
modules over that shared driver: `volume/copy_serial.rs` supplies the copy-specific closures, and
`volume/copy_concurrent.rs` runs its own window of spawned tasks rather than becoming a third driver mode (one-of-four
abstraction not worth its weight).

**Decision**: `copy_files_with_progress_inner` aligns `scan_result.files` to the driver's `&[PathBuf]` API via a paired
`Vec<&FileInfo>` and a closure-captured `slice::Iter` advanced in lock-step with the driver iteration. **Why**: The sync
driver iterates a generic `&[PathBuf]`, but the local-FS copy loop needs the full `FileInfo` (for `dest_path`,
`is_symlink`, `size`, and the `SourceItemTracker` key). Three alternatives were rejected: (a) indexing into
`scan_result.files` by `ctx.files_done_so_far` — wrong, the cumulative counter is bytes-affecting and includes
bulk-skipped files, so the index would shift; (b) extending `TransferContext` with a generic associated payload —
couples the driver to local-FS specifics; (c) cloning the `FileInfo` slice for `sources` — copies on the hot path. The
iterator approach is O(0) memory beyond the path vec and matches the driver's iteration order exactly (`pre_skip_paths`
is empty because we pre-filter `scan_result.files` ourselves, so the driver invokes the closure once per surviving
file). The `.expect()` is justified inline; if the driver ever stopped calling the closure once per source the test
suite would break.

**Decision**: Cross-FS local move reuses the scan-preview cache via `config.preview_id`. **Why**: `move_with_staging`
used to ignore `preview_id` and always re-run `scan_sources`. The FE had just paid the cost of a full scan in
`TransferDialog` (which emits cumulative `scan-preview-progress` events), so the second BE-side scan starting from
`filesDone=0` made the count visibly reset in `TransferProgressDialog` ("scan again from 0, climb to N, then phase flips
to Copying and the bar jumps to total"). Now the function checks `config.preview_id` first: on cache hit the
`ScanResult` is consumed directly (same shape `copy.rs` uses), skipping the redundant scan and going straight to the
active phase. On miss (no preview at all) the original `scan_sources` path stays — so MCP triggers and programmatic
moves still work.

**Decision**: Volume-path progress is LEAF-granular, and the accounting is split by scope in
`transfer_driver/progress.rs`: a `LeafProgressLedger` per operation, a `SourceProgress` per top-level source (the name
its events carry, plus its throttle clock), and a `LeafProgress` per file, minted where the file starts and settled
exactly once. `copy_single_path` takes the `SourceProgress` and every file below it mints its own handle. `bytes_total`
/ `files_total` are the preflight LEAF totals, so the emitted numbers climb across leaves AND across the many leaves one
directory streams at once: reported bytes are `finished + sum(in-flight leaves)`, and an operation-wide
`Arc<AtomicUsize>` counts completed leaves. The serial drivers reseed `finished` from their own `bytes_done` per
iteration (they count skipped sources the leaves never see); the concurrent driver doesn't, because its sources overlap
and the ledger IS its tally. **Why**: The serial path originally captured FROZEN snapshots (`bytes_done_so_far`,
`files_done_so_far`) per top-level source. For a directory source every inner file emitted against the same `(0, 0)`
snapshot, so the Size bar reset to 0 at each inner file and the File bar sat at 0 for the whole folder (observed moving
a 9-file folder, ~10.6 GB, USB → SMB NAS). The frozen snapshot also can't survive multiple directory sources. The byte
axis predates this: the move site once shipped a no-op `Continue(())` callback because the old move code sent
`bytes_total = 0`; once `volume/preflight.rs` populated `bytes_total`, the Size bar pinned at 0 through a multi-minute
upload (SMB dest, 3.7 GB file, "Moving... 0 bytes" the whole time) — DON'T reintroduce a no-op progress callback. One
shared high-water slot per source came next, and it broke the other way once a directory started streaming its leaves
concurrently: the bar showed whichever leaf was furthest along, and the next leaf to finish wiped it, so a 664 MB folder
whose largest member was 259 MB sawtoothed between ~300 MB and ~80 MB. Hence a share per leaf. A leaf's `complete` also
credits its exact size whatever the callback did, which is why no "the volume never invoked on_progress" compensation is
needed any more. Pinned by `a_small_leaf_finishing_does_not_wipe_a_big_leaf_still_in_flight`,
`leaves_in_flight_sum_instead_of_reporting_the_largest`,
`test_cross_volume_copy_directory_source_progress_is_leaf_granular`,
`cross_volume_move_directory_source_progress_is_leaf_granular`, `cross_volume_move_emits_intra_file_progress`,
`test_cross_volume_copy_serial_emits_intra_file_progress`, and
`test_cross_volume_copy_concurrent_emits_intra_file_progress`.

**Decision**: The per-file milestone (the bypass-throttle emit that guarantees the axes cross `N/N` even when chunked
emits ate the throttle window) lives at the unit-of-work layer, not the driver. Serial volume paths:
`LeafProgress::complete` fires it per leaf. Concurrent path: the task-complete branch
(`copy_volumes_with_progress::Some(Ok(...))`). Sync local-FS path: `copy_single_item` via `copy::record_file_done`. The
async driver's `Transferred` arm emits a per-source milestone ONLY when `DriverConfig::emit_per_source_milestone` is
`true` — set for the same-volume rename-merge, whose `transfer_one` does a bare `rename` with no streaming and thus no
closure emit, so the driver milestone is its only Copying event. The streaming paths set it `false`: a
top-level-granular driver milestone after a directory source would regress the File bar (9/9 → 1/9).
`drive_transfer_serial_sync`'s `Transferred` arm never emits (the closure owns it). Mirrors the sync/async
conflict-resolution split (sync driver delegates to closure, async driver dispatches itself). **Why**: Chunked
`on_progress` emits inside `transfer_one` (async) or `copy_single_item` (sync) carry `files_done_so_far` (the driver's
iteration snapshot taken before this file started), so for single-file ops the chunked path never crosses `N/N` — the
user would see "Copying... 99% / 0 of 1 files" jump straight to the complete toast, never observing the final "1 of 1"
milestone. Putting the milestone in `copy_single_item` (a function called by both `copy_files_with_progress_inner` _and_
`move_with_staging`'s direct copy loop) means a cross-FS local move sees the same milestone shape as a regular local
copy — without `move_with_staging` needing its own duplicate emit. The throttle bypass is deliberate: per-file
milestones are bounded by file count (not noisy), and throttle suppression of this event is exactly the bug being fixed.
The emit fires at every `Ok`-return site in `copy_single_item` (regular copy, symlink copy, per-file Skip, type-mismatch
parent Skip, same-file no-op) via a `PerFileCtx` struct that bundles the six operation-wide values so the six call sites
stay one-liners. `cross_volume_move_cancel_mid_batch_preserves_completed` still passes because the async driver's
milestone observes `files_done >= N` between sources before the next-iter cancellation check fires. Pinned by
`cross_volume_move_emits_intra_file_progress`, `test_cross_volume_copy_serial_reaches_files_done_n`,
`test_cross_volume_copy_concurrent_reaches_files_done_n`, `local_copy_single_file_reaches_files_done_n`, and
`cross_fs_local_move_single_file_reaches_files_done_n` (the last two in `move_op/move_progress_tests.rs`).

**Decision**: `scan_volume_sources` emits climbing `Scanning`-phase tallies via `scan_for_copy_batch_with_boundary`.
**Why**: Without a cached `preview_id` (programmatic moves, MCP-triggered ops), the operation's scan phase used to emit
a single `0/0/0` event up front, then sit silent through the entire `scan_for_copy_batch` call before flipping to
`Copying`. On slow sources (cold MTP listing, large SMB tree) the FE shows "Scanning... 0 bytes / 0 files / 0 dirs" the
whole duration. The scan-preview pipeline emits its own climbing tallies into the preview's event channel, not the
operation's, so the operation needs to wire its own. The fix: pass a throttled `Fn(ListingProgress)` callback through
`scan_for_copy_batch_with_boundary` (the boundary-taking trait method) that emits a `Scanning`-phase event per tick,
plus a final throttle-bypassed emit with the aggregate totals so a fast scan whose per-listing emits all got throttled
still lands on the right number. The kickoff + final emits frame the per-tick stream. Pinned by
`cross_volume_move_emits_scan_phase_tallies_during_walk`.

**Decision**: Cross-volume move runs the same preflight scan as volume copy (`scan_volume_sources` in
`volume/preflight.rs`); the SAME-volume move does NOT — it's a rename-merge with top-level hints only and
`bytes_total = 0`. **Why**: The two move paths have different cost models. **Cross-volume move**
(`move_volumes_with_progress`, copy+delete) genuinely transfers bytes, so it shares `scan_volume_sources` with copy: a
cached `TransferDialog` preview when available, else `volume.scan_for_copy_batch_with_boundary`, giving the real
`(total_files, total_bytes, source_hints)` triple so the FE's Size bar tracks. **Same-volume move**
(`move_within_same_volume_with_progress`) is a server-side rename — it moves ZERO bytes — so a deep recursive pre-flight
scan there was pure waste that cost 30–40 s of "Verifying before move…" on a NAS (a real field incident: a same-NAS
folder move blocked for 30–40 s before the 100 ms rename). It now calls `top_level_move_hints` instead: top-level
`is_directory`/size hints only (one `list_directory` per distinct parent, O(distinct parents), never a subtree walk),
`files_total` = number of selected top-level items, `bytes_total = 0`. The FE hides the Size bar on `bytes_total == 0`,
which is honest — a rename moves no bytes, so a Size bar would be a lie. `known_directory_paths` (keeps bulk-skip
file-only) comes from the same top-level hints. The perf contract is pinned by
`volume/rename_merge_tests.rs::non_conflicting_move_does_no_subtree_walk` (a counting volume asserts no interior
listing + O(top-level) stat count) and the SMB integration pin
`smb_integration_same_share_nonconflicting_move_no_subtree_walk`.

**Decision**: Same-volume folder collisions merge via a recursive rename-merge (`volume/rename_merge.rs`), entered
directly — no resolver round-trip for the folder. **Why**: A flat `rename(source_dir, dest_dir, force=false)` onto an
existing same-named dest dir fails `AlreadyExists`, so the move used to ERROR on a top-level folder collision instead of
merging. The rename-merge walks the source folder level by level: it lists the source level + (pre-existing) dest level
once each, then per child — no dest map hit → one `rename(child_src, child_dest, force=false)` (a whole subtree rides
along, never descended); dir-vs-dir hit → recurse; file / cross-type hit → route through `resolve_volume_conflict`
(folders never prompt; only files do). A file Overwrite collapses to the legacy delete-then-rename shape (the
safe-replace temp dance buys nothing for a non-streaming rename). All renames are `force=false`. **Late-detected
collisions** (case-insensitive SMB/APFS, TOCTOU): an unexpected `AlreadyExists` from a child rename is treated as a
conflict routed through the resolver, NEVER a hard error — scoped to children with no exact-match hit, branching on a
per-level `name → resolution` map so an already-resolved child finalizes its stored decision instead of re-prompting
(this per-level map is orthogonal to the op-wide apply-to-all latch). **Source-dir cleanup is inside-out, empty-only**:
after a level completes, `volume.delete_directory(source_dir)` is attempted; `Volume::delete`'s "file or EMPTY directory
only" contract means a non-empty source (skipped/errored/unmoved child) fails benignly and the dir — plus all its
ancestors — survives. An all-moved spine deletes deepest-first via the recursion unwind. Symlinks move as opaque
entries, one rename and never descended, on both sides of every clash (§ "Symlinks are opaque to a move"). The
downloads-watcher hook fires on BOTH halves of every child rename. Cancel mid-merge keeps already-renamed children
(existing contract) and never deletes a dir still holding unmoved content. Pinned by `volume/rename_merge_tests.rs`
(zero-folder-prompt, skip/overwrite/rename/stop policies, cancel, source-dir cleanup matrix, dest-inside-source,
case-fold prompts-once / no-double-prompt, symlink) and the SMB integration pin
`smb_integration_same_share_move_merges_with_no_folder_prompt`.

**Decision**: `exacl` for ACLs (read, applied after landing, and stripped before a reversal's removal, all in
`cmdr_fs::metadata`), not custom FFI bindings. **Why**: `exacl` adds zero new transitive dependencies (all of its deps,
`bitflags`, `log`, `scopeguard`, `uuid`, are already in our tree). It provides cross-platform ACL support (macOS, Linux,
FreeBSD) and full ACL parsing/manipulation for potential future UI features. The crate appears unmaintained (last
release Feb 2024) but ACL APIs are stable and don't change. Our usage is best-effort with graceful fallback: if `exacl`
ever breaks, files still copy, they just lose ACLs. MIT licensed (compatible with BSL).

**Decision**: On macOS, use `copyfile(3)` only for same-APFS-volume copies; use chunked copy for everything else.
**Why**: The only practical benefit of `copyfile(3)` is APFS clonefile (instant copy-on-write, zero extra disk usage),
which only works on the same APFS volume. We evaluated `copyfile` on other filesystems:

- **HFS+**: No clonefile, and no metadata edge: `copyfile` without a clone doesn't set birth time, and the chunked arm
  carries birth and flags itself.
- **exFAT / FAT32**: No clonefile and no ACLs; xattrs land as `._` sidecars and birth is settable either way.
  `COPYFILE_ALL` fails the whole file on a resource fork there (notes file above), where the chunked arm drops only that
  xattr. No practical benefit.
- **NTFS-3G**: FUSE-based, so `copyfile` goes through userspace with the same I/O buffering issues as network mounts.
  `COPYFILE_QUIT` is unreliable. No benefit.
- **Network mounts (SMB, NFS, AFP, WebDAV)**: `copyfile` ignores `COPYFILE_QUIT` while draining buffered I/O, causing
  cancellation to take 30+ seconds or complete the copy entirely. This applies when _either_ the source or destination
  is on a network mount (for example, NAS-to-local copies).
- **USB / external drives**: Typically exFAT or HFS+, no clonefile. Different volume from the internal drive, so no
  same-volume benefits.

Our chunked copy (1 MB read/write chunks) provides: identical speed for non-clonefile copies, reliable cancellation
between chunks, and granular progress callbacks. It keeps everything `copyfile` would, birth time included (which
`copyfile` without a clone and `ditto` across volumes both drop,
`docs/notes/transfer-metadata-experiments-2026-10-11.md`), and the user flags. Detection uses `st_dev` (device ID) for
same-volume and `statfs.f_fstypename` for APFS. See `copy_strategy.rs` for the implementation.

## Gotchas

**Gotcha**: In `copy_volumes_with_progress`, a per-task `VolumeError::Cancelled` populates `copy_error` and would
otherwise skip the `write-cancelled` emit. **Why**: Both the concurrent path (`Some(Err((failed_dest, e)))` arm) and the
serial path (`PostLoopIntent::Failed`) feed any per-task error into `copy_error` via `map_volume_error`.
`VolumeError::Cancelled` → `WriteOperationError::Cancelled`, so a mid-flight cancel that surfaces through a streaming
reader lands in `copy_error` as a Cancelled-shaped `WriteFailure`. The post-loop gate
`if copy_error.is_none() { emit_cancelled }` then silently swallows it, the outer `copy_between_volumes` wrapper's
`matches!(Cancelled)` arm assumes the inner already emitted, and the FE never sees a terminal event — Copy dialog hangs
until restart. The post-loop reclassifies a Cancelled-shaped `copy_error` as cancellation (`copy_error = None`) whenever
`state.intent().is_cancelled()` is true, so the emit gate fires; the synthetic `Err(Cancelled)` at the bottom of the
function still propagates so the outer wrapper continues to skip `write-error`. Repro: copy 13 SMB files concurrent
path, cancel mid-stream → no `write-cancelled` on the wire pre-fix.

**Gotcha**: Cross-type Overwrite (file↔folder) is neither a merge nor a safe-replace: the destination goes ASIDE for the
rest of the operation. **Why**: A type swap can't stage the new side (a folder lands leaf by leaf), so
`apply_volume_conflict_resolution` renames the dest to a `.cmdr-temp-<uuid>` sibling and the operation settles it when
it ends. `volume/DETAILS.md` § "A cross-type Overwrite renames the destination ASIDE". Same-type dir-vs-dir never
reaches `apply_volume_conflict_resolution` for the folder — it short-circuits to merge in `resolve_volume_conflict`
before any policy dispatch.

**Test harness: the conflict responder is an event sink, prompt counts come from the sink.** The folder-merge suites
(`volume/merge_tests.rs`, `volume/rename_merge_tests.rs`) drive Stop-mode prompts with `ConflictResponderSink`
(`conflict_responder_test_support.rs`): it wraps a `CollectorEventSink`, forwards every event, and the instant it
observes a `write-conflict` it answers `state.conflict_slot` with the scripted response, or
(`ConflictResponderSink::cancelling`) refuses the prompt by cancelling the operation, the dialog's Cancel. This works
because the Stop branch arms the slot BEFORE emitting the event (`volume/conflict.rs`), so the answer can't miss.
Assertions derive the prompt count from the recorded conflicts via the shared counters in
`conflict_responder_test_support.rs` — `file_conflict_count`, plus `folder_conflict_count_both_dirs` (source AND dest
are dirs; pins the copy-side "dirs never prompt" contract) and `folder_conflict_count_any_dir` (source OR dest is a dir;
pins the rename-merge contract) — race-free once the op future returns, never from a side-channel counter. The pattern
is order-independent by design, so there's no polling loop and no answer-accounting race to defend.

## Gotcha: a skip-guard `return` above a macOS-only block breaks the Linux lane

`copy_integration_test.rs`'s macOS tests are shaped as setup followed by one `#[cfg(target_os = "macos")] { … }` block
holding every assertion. A `return` placed ABOVE that block (the usual "skip if the tool isn't available" guard) is a
normal early exit on macOS, but on Linux the block vanishes and the `return` becomes the function's last statement, so
`clippy::needless_return` fails the `Desktop (Rust)` CI job. **A local macOS run cannot see it**, which is what makes it
worth writing down.

Keep the guard INSIDE the `cfg` block, where it disappears along with the code it guards (`test_copy_preserves_xattrs`).
Verified by reproduction rather than by reading the lint docs: compiling the old shape with the macOS block stripped
reproduces the exact CI error, and the new shape is clean.

This is the same class as `3d4c9816f`, where two `use` statements sat above their macOS-only use sites. Any code whose
only consumer is behind `cfg(target_os = "macos")` needs the same `cfg` on the thing above it.

## An Overwrite or a move never removes a mounted drive

A cross-type Overwrite (a file onto a folder) and the set-aside-and-materialize landing remove the replaced FOLDER as a
tree once the new item lands: `remove_dir_all` locally (`overwrite.rs`: `stage_and_land_file` step 4,
`safe_overwrite_dir` step 3), `cleanup::remove_tree` across volumes (`volume/displaced_destination.rs::discard`). A
drive, disk image, or share mounted anywhere inside that folder would be emptied with it. So each refuses up front,
before the temp is written or the folder set aside, with `WriteOperationError::MountedVolumeInside`
(`mount_boundary::refuse_a_mount_inside`, or `refuse_a_volume_mount_inside` for a `LocalPosixVolume` destination).

The check reads the OS mount table for a mount point at or under the folder's resolved path, so it costs one `lstat` for
a file being replaced and never walks the folder. A table that can't be read refuses. A remote backend's destination
isn't asked: what's mounted on a server is the server's. The dialog words it as a replacement
(`errors.write.mountedVolumeInside.message.replace`). Pinned on a real APFS image mounted inside a test folder
(`real_image.rs`, the disk-image lane). The delete's side of the same wall: `../delete/DETAILS.md` § "A mount is a
wall".

A move onto another volume (`move_op/cross_fs.rs::move_with_staging`, `volume/move_cross.rs`) or into a zip
(`materialize_sources` in `../archive_edit/copy_into.rs`) copies each tree, then deletes what it copied, so a source
that is or holds a mount point would have the drive's files carried off it. All three refuse up front, before the scan
(the zip: before the source is stamped or read) and before a cached preview's file list is trusted, with the same check
over every top-level source in one mount-table read (`mount_boundary::refuse_moving_a_mount` /
`refuse_moving_a_volume_mount`). A same-volume move renames and moves nothing off a drive, so it isn't asked. The
refusal carries `MountedVolumeAction` (`delete` / `replace` / `move`), set by the refusing site, because a move can
refuse both as a move and as an Overwrite; the dialog keys its sentence on it, ❌ never on the operation type.
