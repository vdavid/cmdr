# Cancelling an operation shouldn't delete a file something else changed

**The problem.** Cmdr has three rollback mechanisms. The journal-driven one (the Roll back button in the history
dialog) rechecks every item against a recorded snapshot and refuses to touch anything that changed since. The in-flight
ones (the Rollback button on the transfer dialog, which undoes the operation you're cancelling) recheck nothing: they
delete and move back unconditionally, from an in-memory list of bare paths.

That assumption held when the list was written seconds ago and nothing else was running. It doesn't hold in general: a
copy can run for hours, and the user, another app, or a sync client can touch a destination in that window. Cancelling
then deletes a file that is no longer the one Cmdr wrote. Separately, the in-flight **move** rollback renames back over
whatever now sits at the source path, destroying it silently.

This plan gives the in-flight paths the two data-safety guards the history path already has. Principle 1 (protect the
user's data) in `AGENTS.md`.

**❌ This is deliberately NOT a unification of the three mechanisms.** Routing the cancel path through the journal would
make it depend on the operation-log database having opened and no row having been dropped under backpressure, where
today it works from memory regardless. Trading a dependency-free safety net for a journal-dependent one, on a
data-safety path, is the wrong direction. There's a second reason worth writing down: for a directory merge the
in-memory ledger is strictly **more** capable than the journal, which marks merges `not_rollbackable`
(`transfer/move_op/same_fs.rs:138`) while `MoveTransaction` holds the exact rename list and can reverse it. The
in-memory ledgers stay; they just learn to verify.

All paths below are relative to `apps/desktop/src-tauri/src/file_system/write_operations/` unless stated otherwise.

## The decisions already made

- **A drifted file is skipped, not deleted.** David confirmed this knowing it's a visible behavior change: Rollback
  will sometimes leave files behind where today it always removes them. It matches the history path, and "something
  else touched this file, so I left it" is the right instinct here. Don't revisit it; do make the UI say what was left
  and why.
- **Snapshots are size-only, everywhere — including local.** See "The mtime trap" below. This is a change from the
  first draft of this plan and is the one call worth David's eye before you start (it's flagged in
  `docs/specs/index.md`).
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

## The mtime trap (read before milestone 1)

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

So: **size only, on every path.** It's what the volume path and the shipped history path already rely on, and
`verify_snapshot`'s own doc blesses a size-only snapshot. Also decide whether the recheck stats with `metadata` or
`symlink_metadata` and say why; the former on a dangling copied link yields `Unverifiable` and the same leftover.

The cost, stated honestly: a file replaced by a *different file of exactly the same size* verifies and gets deleted.
That's the same exposure the history path already ships.

## Milestone 1: the ledgers carry a snapshot

**Intent.** A bare path can't be verified. Each recorded entry needs the size the file had when Cmdr wrote it.

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
6. **Decide which way the bar may lie, and say so.** `transfer/volume/cleanup.rs:118` increments `paths_deleted` even
   when the delete failed. If a skip also increments, the bar reaches zero while files remain; if it doesn't, the bar
   strands short. Neither is acceptable silently — pick one and pair it with milestone 3's wording.

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

**On the bar: it counts forward.** ❌ Don't spend time re-deciding this. The history path faced the identical question
and answered it — `ReversalRunner::frame` (`write_operations/rollback.rs:135-155`) counts `files_done` up toward a
total, pinned by the E2E "a reversal reports honest forward progress". A decreasing bar earns its meaning only when it
always reaches zero, which stops being true here.

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

## Out of scope, deliberately

- **❌ Unifying the three mechanisms** onto the journal, for the reason at the top. If you find yourself reaching for
  the journal from the cancel path, stop.
- **Pre-finalize rollback eligibility** in the operation log (a header opens as `not_rollbackable` on purpose so a
  crash before finalize leaves it honestly unrollbackable). That only serves the unification above.
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
