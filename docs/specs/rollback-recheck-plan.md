# Cancelling an operation shouldn't delete a file something else changed

**The problem.** Cmdr has three rollback mechanisms. The journal-driven one (the Roll back button in the history
dialog) rechecks every item against a recorded snapshot and refuses to touch anything that changed since. The in-flight
ones (the Rollback button on the transfer dialog, which undoes the operation you're cancelling) recheck nothing: they
delete and move back unconditionally, from an in-memory list of bare paths.

That assumption held when the list was written seconds ago and nothing else was running. It doesn't hold in general: a
copy can run for hours, and the user, another app, or a sync client can touch a destination in that window. Cancelling
then deletes a file that is no longer the one Cmdr wrote. Separately, the in-flight **move** rollback renames back over
whatever now sits at the source path, destroying it silently.

This plan gives the in-flight paths the two data-safety guards the history path already has, and then gives the
history path's own rollback the Pause and Cancel buttons its engine already supports. Principle 1 (protect the user's
data) in `AGENTS.md`.

**❌ This is deliberately NOT a unification of the three mechanisms.** Routing the cancel path through the journal would
make it depend on the operation-log database having opened and no row having been dropped under backpressure, where
today it works from memory regardless. Trading a dependency-free safety net for a journal-dependent one, on a
data-safety path, is the wrong direction. There's a second reason worth writing down: for a directory merge the
in-memory ledger is strictly **more** capable than the journal, which marks merges `not_rollbackable`
(`transfer/move_op/same_fs.rs:138`) while `MoveTransaction` holds the exact rename list and can reverse it. The
in-memory ledgers stay; they just learn to verify.

All paths below are relative to `apps/desktop/src-tauri/src/file_system/write_operations/` unless stated otherwise,
with one exception worth naming because it trips people: `operation_log/` is a SIBLING of `file_system/`, at
`apps/desktop/src-tauri/src/operation_log/`, not a child of `write_operations/`.

**Line numbers in this plan have drifted and some were never exact. Locate every symbol by NAME**
(`codegraph_search`, or grep for the `fn` / `struct`), and treat a line number as a hint about which end of the file to
look in. Two known-stale ones: `merge_move_directory` is not where the milestone 1 bullet says, and
`transfer/copy/mod.rs`'s failure-cleanup arm calls the bare `CopyTransaction::rollback()`, not the progress helper the
milestone 2 list implies.

**On the "six entry points" in milestone 2:** they collapse to FOUR functions, because the direct
`CopyTransaction::rollback()` call, the failure-cleanup arm, and the `Drop` net are all the same function. So the
destructive acts to guard live in `rollback_with_progress`, `CopyTransaction::rollback` (which needs a mode, since the
`Drop` net stays unconditional), `MoveTransaction::rollback`, and `volume_rollback_with_progress`. Confirm that before
you start; if it's still true, the milestone is smaller than its bullet list suggests.

## The decisions already made

- **A drifted file is skipped, not deleted.** David confirmed this knowing it's a visible behavior change: Rollback
  will sometimes leave files behind where today it always removes them. It matches the history path, and "something
  else touched this file, so I left it" is the right instinct here. Don't revisit it; do make the UI say what was left
  and why.
- **A local snapshot is size plus inode; a volume snapshot is size only.** See "The identity check" below. David
  reviewed and confirmed this.
- **The ledgers are poppable stacks, not lists you iterate.** See milestone 1.
- **The reversal bar keeps counting DOWN, and every item it processes advances it.** See milestone 2 item 6 and
  milestone 3.
- **The bar counts forward.** Don't re-derive this: the history path hit the same problem and answered it. See
  milestone 3.

## What already exists (don't rediscover it)

**The in-flight mechanisms:**

- **Local copy**: `CopyTransaction` (`state.rs:803`) holds `created_files: Vec<PathBuf>` and
  `created_dirs: Vec<PathBuf>`. `transfer/copy/rollback.rs:27` `rollback_with_progress` is the progress-reporting
  reversal; it emits *decreasing* `files_done` / `bytes_done`.
- **Local move**: `MoveTransaction` (`transfer/move_op/mod.rs:40`) records `(source, dest)` rename pairs. It's a
  required `&mut` parameter of several functions, not a free-standing ledger.
- **Volume**: `transfer/volume/cleanup.rs:56` `volume_rollback_with_progress`. Volume-aware, emits progress, honors
  cancel-the-rollback, prunes created dirs only when empty, and gates recursive deletes behind a `TreeRemoval`
  capability type so a wrong "is this a directory?" belief can't reach a recursive delete. Well built; respect its
  shape.

**There is no fourth mechanism.** `archive_edit/` reaches none of these (only `CreatedPaths::skipped_file_count`), and
`write_operations/rollback.rs` is the history executor, not a ledger.

**But there are six entry points, not three** — milestone 2 must cover all of them or the same operation will
delete-or-skip depending on which terminal arm it takes:

- `rollback_with_progress` (the Rollback button), reached from `transfer/copy/mod.rs:512`
- `transfer/copy/mod.rs:671` — the `PostLoopIntent::Failed` arm routes **error** cleanup through the same helper
- `transfer/copy/mod.rs:564` — a direct `CopyTransaction::rollback()`
- `state.rs:862` — the `Drop` panic-cleanup net
- `MoveTransaction::rollback`
- `volume_rollback_with_progress`

**The verification to reuse**: `verify_snapshot` (`operation_log/rollback.rs:207`) with `SnapshotVerdict`
(`:190`): any recorded field must match, at least one must verify, an absent live counterpart is `Unverifiable`. Both
paths must agree on what "changed" means, so share it. **Reuse is a visibility bump, not a refactor**: it's one crate,
`write_operations` already imports `operation_log::types` and `operation_log::rollback` already imports `FileEntry`, so
there's no cycle — `verify_snapshot` and `SnapshotVerdict` just need `pub(crate)`. **Generalize rather than adapt**:
change `live: &FileEntry` to `(live_size: Option<u64>, live_mtime: Option<u64>)`. It reads exactly two of `FileEntry`'s
28 fields, both existing call sites already hold the entry, and it keeps the local path from having to synthesize a
28-field serde/specta type out of `std::fs::Metadata`.

**The typed skip reasons**: `SkipReason` (`operation_log/types.rs:218`): `UnverifiablePrecondition`, `Drift`,
`RestoreTargetOccupied`, `DirNotEmpty`, `AlreadyGone`, `Failed`. Reuse this enum; ❌ don't invent a parallel one. Its
doc comment at `types.rs:212-215` currently says it's stored "ONLY by the rollback engine"; **amend that in the same
commit** or the next reader treats the new usage as a bug.

**The reporting shapes to reuse**: `RollbackReport` and `SkipBreakdown` (imported at
`write_operations/rollback.rs:42-44`), already carrying per-reason counts with an example file name.

## The identity check (read before milestone 1)

The obvious rule is "size and mtime locally, size only on volumes", because local copies preserve mtime deliberately
(`COPYFILE_STAT` in `macos_copy.rs:38`, `filetime::set_file_times` in `chunked_copy.rs:280` and `linux_copy.rs:133`)
while the volume write path doesn't (see the `❌ Don't add an mtime here` note at `journal.rs:208`).

**That rule is wrong here, and recording mtime locally would strand whole copies.** It keys on how Cmdr *writes*, but
the failure keys on what the destination *filesystem stores*:

- **Coarse granularity.** Snapshots are whole seconds. FAT32 stores mtime at 2-second granularity, and network mounts
  round too. Copy to a USB stick, cancel, roll back: every preserved mtime reads back truncated, every file drifts, and
  the entire copy is left on the stick. That's a common cancel-and-rollback scenario, not an edge case.
- **Symlinks always drift.** The snapshot comes from `fs::symlink_metadata` (`transfer/copy/single_item.rs:359`), so
  it's the *link's* mtime, but `copy_symlink` creates a fresh link with no mtime preservation. Every copied symlink
  would be left behind.

So **mtime is never recorded**, on any path. That leaves size, which the volume path and the shipped history path
already rely on and which `verify_snapshot`'s own doc blesses. Also decide whether the recheck stats with `metadata` or
`symlink_metadata` and say why; the former on a dangling copied link yields `Unverifiable` and the same leftover.

**On local paths, record the inode alongside the size.** Size alone would let a file replaced by a *different file of
exactly the same size* verify and be deleted. `(dev, ino)` closes that: it's an exact "is this still the file I wrote"
check, it survives both traps above (a rename-into-place preserves nothing, so the inode changes and we correctly skip;
`symlink_metadata` on a symlink reads the link's own stable inode), and nothing changes an inode without someone
touching the file, so it adds no false positives. It catches the most common real drift by far, an editor saving via
write-temp-then-rename, which size alone often misses.

The inode is local-only: SMB, MTP, and archives have no stable equivalent, so volumes stay size-only and carry the
same-size exposure the history path already ships. Model the two as **distinct cases in the type**, not one struct of
`Option` fields a call site can silently fill with `None`.

## Milestone 1: the ledgers carry a snapshot

**Intent.** A bare path can't be verified. Each recorded entry needs the identity the file had when Cmdr wrote it.

**Shape the ledger as a stack you pop from, not a list you iterate.** Today `CopyTransaction::rollback` walks
`created_files.iter().rev()` without mutating, and `volume_rollback_with_progress` walks a `&[PathBuf]`; a reversal
that stops halfway leaves the ledger still claiming files that are already off the disk. Popping each entry as it's
reversed makes the ledger a truthful statement of what this operation currently has on disk at every instant. That's
worth having on its own (it's what lets a partial reversal report honestly), and it's the shape a future
pause-and-resume of an in-flight reversal would need, so getting it right while all four ledger sites are already open
costs nothing extra.

**This is not uniform across sites.** Handle each explicitly:

- **Local copy — free.** `transfer/copy/single_item.rs:355-359` already stats and holds the metadata, and
  `record_local_leaf` is called with `Some(write_weight)` on the line above every `transaction.record_file`.
- **Top-level same-FS move — free.** `transfer/move_op/same_fs.rs:86-89` already stats and holds `source_size` before
  the rename. A rename preserves the inode, so the recheck is exact.
- **The directory-merge move branch — needs one stat per child, and you must add it.**
  `transfer/move_op/mod.rs:318` `merge_move_directory` and `move_resolved_into_place` (`:109`, `:124`, `:127`) record
  renames with no metadata anywhere in scope. Recording `None` there and letting the recheck call it unverifiable
  would mean **a cancelled folder-into-folder move rolls back nothing**, which is worse than the bug this plan fixes.
  Take one `symlink_metadata` per child: it's cheap, the code is about to `rename` that child anyway, and the merge is
  the case where the in-memory ledger beats the journal, so it has to work.
- **Volume — four discard sites across three files, plus a carve-out.** `copied_paths` (the list handed to
  `volume_rollback_with_progress`) is a **separate ledger** from `CreatedPaths.files`, which is already
  `Mutex<Vec<CreatedFile { path, size }>>` (`transfer/volume/strategy.rs:232`). The sizes are dropped at
  `transfer/volume/copy_serial.rs:528` and `:546` (where `bytes_copied` is in scope two lines up) and
  `transfer/volume/copy_concurrent.rs:377` and `:462`. Reconcile the two ledgers rather than bolting a parallel map on.

**The partials carve-out — get this right or you break a shipped invariant.** `transfer/volume/copy.rs:1064-1078`
pushes `last_dest_path` and every `in_flight_partials` entry into `copied_paths` immediately before the rollback. These
are partial writes, so **no size could exist for them by construction**. Under a uniform "unverifiable ⇒ skip" rule
every one is skipped and the truncated file stays at the destination — which is exactly the failure the shipped
mid-file-cancel work exists to prevent, and which
`test/e2e-playwright/operation-log-rollback.spec.ts:836` ("cancelling inside one large file leaves no partial behind")
asserts today. So: **mark these entries as this operation's own in-flight partials and delete them unconditionally.**
Nothing else can plausibly own a destination path that was never a complete file. Make the distinction explicit in the
type so it can't be lost — a partial is not "a file whose size we happen not to know". The local path is fine here:
`chunked_copy.rs:131-134` removes its own partial and never records it.

**`write_weight` is the scan-time size, not the bytes written** (`transfer/copy/single_item.rs:355` says so), and
`outcome.bytes` is unreliable too (clonefile reports 0, `:547-549`). So the ledger and the journal should record the
same value and the test should assert *that*, not "the size the write actually produced".

**Tests.** A recorded entry carries the size the ledger and the journal agree on; a merge-branch child gets a real
snapshot; a partial is recorded as a partial rather than as an unverifiable file. Per `docs/testing.md`, a new
cross-boundary fixture type needs a named constructor (`desktop-rust-no-hand-rolled-fixture`), and `state.rs`'s 30+
transition tests are a named hot spot: drive them through the public interface, ❌ never by mutating an atomic.

**Docs.** `CLAUDE.md` + `DETAILS.md` for `write_operations/` and `transfer/`.

## Milestone 2: recheck before acting, and be able to say you didn't

**Intent.** Never delete or move back a file that isn't the one this operation wrote — and never do that silently.

**The reporting channel ships with this milestone, not the next one.** Landing the recheck alone would be a behavior
change that leaves files on disk with nothing in the UI saying so, on a data-safety path. Specifically,
`WriteCancelledEvent.rolled_back` is a two-state boolean where `false` currently means "the user cancelled the
reversal"; it now needs a third state, and that's a wire contract crossing specta and `bindings.ts` with consumers in
at least `TransferProgressDialog.cancel-settle.test.ts`, `operation-event-fanout.test.ts`, and
`operation-session.svelte.test.ts`. Widen it here.

**Work.**

1. Before each destructive act, in **all six entry points**, stat the target and run the shared verification. `Match` ⇒
   act. `Drift` or `Unverifiable` ⇒ skip, recording the typed reason. Already gone ⇒ `AlreadyGone`, which is success
   (the desired end state holds), not a skip worth telling the user about.
2. **Add the second guard on the move path.** `MoveTransaction::rollback` does a bare
   `fs::rename(moved_to_dest, original_source)`, so anything the user created at the original source since is destroyed
   silently. The history engine pins **two** guards (`operation_log/rollback.rs:5-19`): the snapshot recheck *and* a
   non-destructive restore that skips with `RestoreTargetOccupied` rather than overwrite, with a carve-out for a
   case-only self-collision. Both belong here. This is a live data-loss path today.
3. **The `Drop` panic net stays unconditional** (`state.rs:862`). It runs when a thread panicked mid-copy; a net that
   skips on drift leaves partials after a panic. Say so in a comment so nobody "fixes" the inconsistency later.
4. **Decide the failure-cleanup arm deliberately** (`transfer/copy/mod.rs:671`). David's decision was about the
   Rollback button; this arm is error cleanup, where the user never asked to keep anything and the alternative to
   deleting is a half-copied tree. Recommendation: apply the same recheck, because deleting a file someone else
   modified is wrong regardless of why Cmdr is cleaning up. State the choice and its reasoning in the commit.
5. **Fix the ETA.** `eta.rs:257` computes `remaining_bytes = bytes_done` during `RollingBack`, hard-assuming the
   reversal drives the counters to zero. Skips make it predict a completion that never arrives, and as deltas go to
   zero the rate collapses and the estimate balloons.
6. **The bar advances for every item the reversal processes**, whether that item was removed, skipped, or failed to
   delete. Decided, don't re-open it. `transfer/volume/cleanup.rs` already increments `paths_deleted` after a delete
   that failed, so the counter already means "items I walked past"; make that honest and deliberate rather than
   accidental. The alternative, a bar that strands at 94% and stops, gets read as a crash, and a user who thinks the
   app crashed never reads the summary line that would have explained things. So the bar always completes, and the
   truth lives in milestone 3's summary. The pre-existing failed-delete case folds into the same reporting as
   `SkipReason::Failed`.

**Watch for.** Verify immediately before acting, ❌ never verify a batch then act on it: a stale verification must not
authorize a destructive act. The history engine got this right and `operation_log/DETAILS.md` explains why.

**Tests.** Real red→green — these are the data-safety claims, so write them first and watch them fail:

- A destination file modified after the copy wrote it is **not** deleted when the copy is cancelled and rolled back.
- Its unmodified neighbours **are** deleted, so one drifted file doesn't abort the reversal.
- The same pair for a cancelled move's restore, plus: a move-back whose source path is now occupied skips rather than
  overwrites.
- The same pair on the volume path. Per `docs/testing.md`, a backend that reports a wrong size must come from the named
  `FaultyVolume` / `InMemoryVolume` fixtures; ❌ don't hand-roll a forwarder.
- A partial is still removed (guarding the carve-out above).
- A file already gone counts as done, not as a skip.

Before assuming this milestone is green, re-read the in-flight cancel assertions in
`test/e2e-playwright/operation-log-rollback.spec.ts:836`, `conflict-edge-cases.spec.ts`, and `conflict-move.spec.ts` —
they pin behavior this milestone changes.

## Milestone 3: the wording

**Intent.** A user who cancels and sees the reversal stop short needs to know what stayed and why, or they'll
reasonably conclude it broke.

**Work.** Surface the outcome: how many came back, how many were left, and why. Reuse `RollbackReport` /
`SkipBreakdown` and follow the pattern the history dialog established — typed reasons plus wording that sets the
expectation up front ("Cmdr skips anything it isn't sure about, so a few may stay behind"). Read
`apps/desktop/src/lib/operation-log/DETAILS.md` and `apps/desktop/src/lib/file-operations/DETAILS.md` for the wording
decisions already recorded there, including two constraints a later copy pass must not undo.

**On the bar: the two reversals count in OPPOSITE directions, on purpose.** ❌ Don't "fix" the inconsistency.

- **The history dialog's reversal counts forward.** It's a fresh operation opening a fresh bar, so filling toward a
  total is the honest motion. `ReversalRunner::frame` (`write_operations/rollback.rs`) already does this, pinned by the
  E2E "a reversal reports honest forward progress". Leave it alone.
- **The in-flight cancel's reversal keeps counting DOWN.** Here the user has been watching one continuous bar fill as
  the copy ran; draining it is the legible picture of undoing the thing they just watched happen, and resetting it to
  fill a second time would read as a new operation starting. The objection that a decreasing bar earns its meaning only
  by reaching zero is answered by milestone 2 item 6: every processed item advances it, so it always reaches zero.
  Zero means "this reversal is finished", and the summary says what finished means.

**Copy.** All new strings go through the message catalog with `@key` descriptions in **all 13 catalogs** (source `en`,
10 full translations, plus the `en-GB` / `en-AU` overlays). Follow `docs/style-guide.md` exactly: sentence case, active
voice, contractions, Oxford comma, no em-dashes, gender-neutral, never the words "error" or "failed", don't trivialize
with "just" / "simple" / "easy", spell out one through nine and numerals for 10+, thousands separators on user-facing
counts. Anchor each translation to that locale's existing rollback vocabulary and its glossary under `docs/i18n/`.
David reviews the copy later and ships it; make it good.

**Tests.** Component tests for the reporting. Extend
`test/e2e-playwright/operation-log-rollback.spec.ts` only if it genuinely earns the seconds — it runs 10 specs in ~7s
and was verified to kill mutants, so keep both properties. A cancel-then-drift E2E is the one most likely to be worth
it, since it's the join no unit test can prove.

**Docs.** The `C+D.md` pairs for the transfer and file-operations areas, frontend and backend.

## Milestone 4: Pause and Cancel on a history rollback

**Intent.** A rollback started from the history dialog can run for a long time over a slow mount, and today there's no
way to stop it from the dialog that started it. The engine already supports stopping and pausing one; only the buttons
are missing.

**This milestone is mostly frontend.** Verify each of these before building on them, then don't rebuild them:

- The reversal runs as its OWN managed operation under `inverse_op_id`, with a registered `WriteOperationState`
  (`operation_log/rollback.rs`, in the deferred task that builds the `ReversalRunner`).
- It polls `should_stop()` per item through `StopMeans::IntentLeavesRunning`, which exists precisely so that any move
  off `Running` stops the REVERSAL rather than being read as "reverse yourself".
- It parks on the pause gate per item (`wait_while_paused`), and cancel wins over pause by construction.
- Stopping midway lands `partiallyRolledBack`, a real `RollbackState` that already carries a notice
  (`operation-log-labels.ts`) and already offers to finish.
- The resume is safe and re-entrant: a second pass re-streams the original op's rows, an item the first pass removed
  reads `AlreadyGone` and is credited with no filesystem call, and both data-safety guards still stand.
  `apps/desktop/src/lib/operation-log/DETAILS.md` § "a partly-reversed operation offers to finish" holds the reasoning.

**Work.** Surface Pause / Resume and Cancel on the rolling-back row in the history dialog, wired to the existing
pause / resume / cancel commands with the reversal's `inverse_op_id`. The real question to answer first is whether the
dialog currently holds that id where the buttons need it; if it doesn't, plumbing it through is the bulk of the job.
Match the affordances the transfer dialog already offers so the two don't diverge. If an existing surface (the queue
row, the corner chip) already offers these for the same operation, reuse rather than duplicate the wiring.

**Watch for.** A paused reversal must not look finished, and a cancelled one must land on the `partiallyRolledBack`
wording that already exists rather than a new string that says something subtly different.

**Tests.** Component tests for the buttons' presence, disabled states, and dispatch. A backend test that a paused
reversal parks and a cancelled one lands `partiallyRolledBack` only if one doesn't already exist.

**Docs.** `apps/desktop/src/lib/operation-log/` `C+D.md`.

**Copy.** Same catalog rules as milestone 3.

## Out of scope, deliberately

- **❌ Unifying the three mechanisms** onto the journal, for the reason at the top. If you find yourself reaching for
  the journal from the cancel path, stop.
- **Pre-finalize rollback eligibility** in the operation log (a header opens as `not_rollbackable` on purpose so a
  crash before finalize leaves it honestly unrollbackable). That only serves the unification above.
- **Yo-yoing an in-flight transfer** (flipping between copying and rolling back at will). David considered and
  declined it for now. It would need `OperationIntent` to lose its one-way `Running → RollingBack/Stopped` shape, and
  it would need forward and reverse to become two directions of ONE cursor over one ordered work list, where today
  forward walks the source list in the transfer driver and reverse walks the created-path ledger in a terminal
  epilogue. Milestone 1's poppable stack is the one piece of groundwork being laid now; don't lay any more.
- Three history-dialog items David has deferred: the row badge staying "Rolling back" until the dialog is reopened,
  reversals not being marked as undos in history (the backend stores `rolls_back_op_id`; the UI ignores it), and an
  operation and its reversal reporting item counts that differ by one.

## Finishing

Scoped checks while iterating, then `pnpm check`, then `pnpm check --include-slow` at the end (this touches transfer
paths). ❌ Never pipe checker output through `tail` / `head` / `grep`; it's concise by design. Commit per milestone,
conventional-commit style, leading with impact, no `Co-Authored-By`.

Two standing rules that bite here: **don't touch any allowlist** without David's explicit consent (`file-length`,
`claude-md-length`, `jscpd-*`, `e2e-duration`; leaving a warn is always safe, surface it instead), and **a retry-pass in
an E2E lane is a flake, not a pass** — chase the race rather than raising a timeout.
