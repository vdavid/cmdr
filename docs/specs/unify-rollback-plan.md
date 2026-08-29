# Rollback: fix what's broken, give the journal-driven one a button, then decide about unifying

**The ask.** The operation-log rollback has no button in the history dialog, reports no progress, can't pause, and can't
be canceled inside a large file. Fix that. Then, if it's worth it, collapse the duplicate rollback implementations.

**Two findings reshaped this plan.**

1. **A cluster of live bugs sits under the shipped rollback**, the worst of which relocates files the operation never
   touched, on both the local and the volume path. They have to be fixed before a button puts this engine in front of
   users, and they're worth fixing whatever happens to the rest of this plan. That's M1.
2. **The obvious reuse is the wrong reuse.** The first draft routed the rollback through the shared transfer driver.
   Traced properly, the driver stats the destination and runs conflict resolution _before_ it calls the per-item
   closure, which fights rollback's pinned never-overwrite policy and buys an extra round trip per item on SMB. The
   right reuse is one level down (the volume move, for cross-volume restores). That removes a whole milestone of churn
   across shared, heavily-pinned code.

## The three mechanisms

1. **Local in-flight** (`write_operations/state.rs` `CopyTransaction`, `transfer/move_op.rs` `MoveTransaction`,
   `transfer/copy/rollback.rs` `rollback_with_progress`). In-memory ledger, backwards-running progress bar, local disk
   only, no recheck before deleting. `CopyTransaction`'s `Drop` impl is a panic-cleanup safety net.
2. **Volume in-flight** (`transfer/volume/cleanup.rs` `volume_rollback_with_progress`). Volume-aware, emits progress,
   honors cancel-the-rollback, prunes created dirs only when empty, and gates recursive deletes behind a `TreeRemoval`
   capability type so a wrong "is this a directory?" belief can't reach a recursive delete. Also no recheck. Well built.
3. **Journal-driven** (`operation_log/rollback.rs` + `rollback/order.rs`, `rollback/skips.rs`, glue in
   `write_operations/rollback.rs`). Replays the durable journal to compute an inverse. Rechecks every item against its
   recorded snapshot and skips on drift. Volume-correct. Reachable from Ask Cmdr's rename undo and over MCP. **No
   progress, no pause, no mid-file cancel, no button.**

They agree on meaning, so both being called "Rollback" in the UI is correct and stays.

Two footnotes for later milestones: `archive_edit/move_out.rs` runs its extract phase through the volume copy, so it
**inherits mechanism 2** and is a caller any removal has to account for (pinned by
`move_out_tests.rs::move_out_rollback_deletes_nothing_from_the_archive`). And `overwrite::safe_overwrite_dir` is a
genuine per-item aside-and-restore that `move_resolved_into_place` depends on; it's small, it _is_ a rollback, and it
survives all of this untouched.

Scope corrections worth carrying: `supports_rollback` is not simply "copy and move". The local starter sets it that way,
but volume starters set their own, and **cross-volume and same-volume move have no in-flight rollback at all**. Trash
undo is reachable over MCP, not from the frontend; Ask Cmdr's batch rename is the only frontend caller of
`undo_operations`.

---

## M1: The live journaling bugs

**Intent.** The engine we're about to expose is already wrong in three places, and the worst of them moves files the
user never asked to touch. Nothing else in this plan matters until these are fixed. All three affect **shipped**
behavior reachable today from Ask Cmdr's rename undo and MCP.

### Bug 1 (worst): a directory _merge_ move finalizes as rollbackable, and its rollback relocates untouched files. Both on the local path and the volume path.

`transfer/move_op.rs:273` takes the merge branch when source and dest are both directories. The journal writes **one**
directory row whose dest is the **pre-existing** destination directory (`:335-343`). Directories are existence-only
checked on restore (`operation_log/rollback.rs:672-679`), and if the merge emptied the source it was removed
(`move_op.rs:531-537`), so the restore target is clear and the rename fires.

Net effect: rolling back `move A/ → B/` where `B/A` already existed renames the merged `B/A`, **including files that
only ever lived in the destination**, to `A/`, and `B/A` disappears. That is exactly what the rollback engine's own
module docs promise never happens.

**The disqualifying condition is "took the merge branch", not "overwrote something".** This matters, because the
tempting fix is wrong. Setting `item_overwrote` for merge children (which the local path never does, since it's only
assigned at `:309` inside the file-conflict branch) catches merges where a child _replaced_ a dest file, and leaves the
bug fully live otherwise: pre-existing `B/A` holding a dest-only file `x`, source `A/` holding only `y`, merge moves `y`
in, nothing is overwritten, source is removed, and rollback still relocates `x`.

**The same bug is shipped on the volume path**, which the earlier draft of this plan wrongly cited as the model to copy.
`volume/move_same.rs:629-639` journals one row with dest = the pre-existing directory and `overwrote` from the
merge-overwrote flag and the overwritten-sources set, and `rename_merge.rs:205` removes the emptied source. Identical
failure. Fix both together.

**Design decision to make before writing code:** either treat entering the merge branch as disqualifying (the operation
is not rollbackable at directory granularity), or journal per-child rows instead of one directory row so the inverse can
reverse exactly what moved and nothing else. Per-child rows are the better end state and cost more; the disqualifying
flag is honest and cheap. Recommendation: **per-child rows**, because "we merged into your folder so we can't undo it"
is a poor answer for a common operation, and D-granularity is what makes the current row wrong in the first place.

Note that `operation_log/DETAILS.md:262` asserts overwrite detection is something only the volume paths need; that
sentence is wrong on two counts and gets corrected here.

### Bug 2: a same-FS move under rename-aside journals the wrong destination, in two places

The landed path goes into the in-memory ledger (`move_op.rs:103`), but the journal records the pre-conflict `dest_path`
at **`:339`** (the rollback unit) **and again at `:354`**, where the search-leaf rebase points the whole subtree at that
same wrong path. Fixing only the first leaves every search row for a rename-aside move pointing at the pre-existing
file's location, so history and name search stay wrong.

### Bug 3: a cross-FS move journals staging paths as destinations

`move_with_staging` copies into `.cmdr-staging-<op_id>/` through `copy_single_item`, which journals the staging path as
the destination; phase 3 renames staging to final and never corrects the rows. Two consequences, and the plan's first
draft only saw the first: the inverse restores from a path that no longer exists, reads as already-gone, and is
**counted as reversed** (a phantom success); and every cross-FS move's history and name-search rows permanently record
paths under a staging directory that no longer exists.

**Fix approach, and the trap in it.** Don't rewrite rows after the fact: the writer exposes no API to amend an item's
destination, and dest directories are interned with folded names, so amending needs a new writer message plus a store
path. Don't buffer the leaf list to journal after the rename either; that breaks the streaming-memory property the
engine relies on. So: **pass `copy_single_item` a journal-as path distinct from the write path** (a clean
`Option<&Path>`, two call sites and two journal sites inside), computing it with the `staging_dir → destination`
arithmetic the remap closure at `move_op.rs:862-869` already does.

**But that arithmetic alone gives the wrong answer, and would trade a phantom success for bug 1's failure mode.** Phase
2 journals; phase 3 resolves conflicts. `staging_dir → destination` is the true final path only when phase 3 hits no
conflict. Under a rename-aside the file lands at `name (2)`; under Skip the staged copy is deleted; inside a phase-3
merge, children get their own rename-aside or skip. Recording the naive final path therefore names a location holding a
**stranger's** file, and since a move's inverse is `RestoreMove`, a size-and-mtime match (very plausible for a
duplicate) would move the pre-existing destination file back to the source.

So the fix has to fold phase-3 resolution in. **Simplest honest answer: any phase-3 conflict resolution marks the
operation not-rollbackable.** Related and also unlisted until now: phase 3 never sets `overwrote` at all, so a cross-FS
move that overwrites at the final destination journals `overwrote = false` and finalizes rollbackable. Same fix closes
both.

**Watch for.** Retention, reconcile, and name search all assume rows land as they land. The fix must preserve that;
changing _what path_ is recorded is fine, changing _when_ is not.

**Tests.** Test-first, real red→green:

- **A merge move whose rollback would relocate a dest-only file.** Two cases, because one fix catches only the first: a
  merge that replaced a dest file, **and a merge that overwrote nothing** (pre-existing `B/A` with a dest-only `x`,
  source `A/` with only `y`). The second is the one that stays broken under the tempting fix. Both on the local path and
  the same-volume path.
- A move hitting a rename-aside conflict rolls back the moved file and leaves the pre-existing one alone, and its search
  rows point at the landed path.
- A cross-FS move rolls back and actually restores the file, rather than reporting a phantom success; its journal rows
  name final paths, not staging paths; and one whose phase 3 resolved a conflict is not offered as rollbackable.

**Docs.** `operation_log/DETAILS.md` (the record point for volume transfers, and the incorrect § about overwrite
detection being volume-only).

**Checks.** `pnpm check rust`, `pnpm check`.

---

## M2: Give the journal-driven rollback progress, pause, and mid-file cancel

**Intent.** Make the existing engine loop observable and interruptible. **This is a contained change to the engine's own
~40-line item loop, not a rearchitecture**, and deliberately does not touch the shared transfer driver.

**Why not the transfer driver.** Its async pipeline is fixed: compute a destination under one shared root, stat it via a
metadata fetcher, run driver-owned conflict resolution, then call the per-item closure
(`transfer_driver/async_driver.rs:167-190`). Rollback has no shared destination root; two of its three inverse actions
are deletes with no destination to stat; that stat is a wasted round trip per item on SMB; and driver-owned conflict
resolution fights the pinned "never overwrite, always skip" policy, which cannot be enforced from inside the closure
because the driver has already decided by then. Adapting the driver would mean changing ~24 call sites across production
and tests on a hot component whose whole purpose is three pinned data-safety properties, in exchange for scaffolding
that doesn't fit. Not worth it.

**The structural move this milestone actually needs: split planner from executor.** The cross-volume machinery worth
reusing is not reachable from `operation_log`. `move_volumes_with_progress` is exported `pub(crate)` **only under
`#[cfg(test)]`**, and the per-file primitives (`strategy::stream_pipe_file`, `strategy::resolve_staging`) are
`pub(super)` inside `volume/`, whose module doc explicitly says everything else there is an implementation detail and
that callers should add a re-export rather than widen a submodule. So "just call the volume move" would swap the
transfer driver's boundary problem for an identical one.

The resolution the codebase's own layering points at: **the rollback engine stays a planner in `operation_log`** (read
the journal page, verify the snapshot, decide the inverse action per item) **and the acting moves to
`write_operations::rollback`**, which is already where the manager glue lives and already imports from both sides. The
engine hands out decided actions; the executor performs them with the volume primitives it can legitimately reach. That
also keeps `operation_log` free of file-moving code, which fits what that subsystem is for.

Do this split first; everything below hangs off it. Three things to get right, each of which an agent would otherwise
discover the hard way:

- **Inject the executor, don't call across.** Keep the loop in the planner and hand it the executor as an injected
  closure, mirroring the `spawn` hook `rollback_operation` already takes (`operation_log/rollback.rs:761`) precisely to
  keep `operation_log` from depending on `write_operations`. Moving the loop out instead relocates all of
  `rollback/tests.rs` and `undo_tests.rs`, since their rig is `pub(super)` inside `operation_log::rollback`.
- **Interleave per item, and pause _before_ verifying.** Don't verify a whole 512-item page and then act on it: that
  widens the verify-to-act window, and a pause landing between "verified unchanged" and "delete" would let a
  ten-minute-stale verification authorize a destructive act.
- **The sink is injected at the edge.** `write_operations/mod.rs:82-84` pins (grep-enforced) that the pipeline never
  constructs a sink, so it has to arrive through `dispatch_rollback`, which ripples to its MCP caller and to
  `undo_operations`.

Reachability is fine: `volume/mod.rs` already re-exports `strategy::pull_path_to_local` at exactly the
`pub(in crate::file_system::write_operations)` visibility an executor needs, added exactly the way that module's docs
instruct, and everything `stream_pipe_file` wants is already in hand where the glue builds the operation state.

**Work.**

1. **Split the engine.** Planner stays in `operation_log::rollback` (paging, snapshot verification, inverse-action
   decisions, journal bookkeeping). Executor moves to `write_operations::rollback`, taking a decided action per item and
   returning an outcome. Keep the planner's existing per-page streaming so the split doesn't materialize the item list.
2. **Widen the executor's inputs** to include an event sink and the operation state. `execute_rollback` currently takes
   only a cancel predicate (`operation_log/rollback.rs:320-327`), so the pause gate is genuinely out of scope rather
   than "present but unasked".
3. Emit progress per item. Totals come from the journal when the inverse is planned, so there is no scanning phase and
   the bar is meaningful from the first frame.
4. **Replace `cross_volume_restore`** (whose `noop` callback at `rollback.rs:735` reports no bytes and never answers
   "stop") with the staged cross-volume primitives, now reachable from the executor's new home. This is what buys
   mid-file cancel, byte progress, staging, retry, and stall detection. If a per-file staged move primitive doesn't
   exist at the right granularity (`move_volumes_with_progress` is an operation-level driver, not a per-file one),
   extract one and re-export it through the `volume/` facade as its own docs instruct.
5. Add `pause_gate.wait_while_paused_async` at the item boundary, **and a cancel check in the deferred-directory phase**
   (`rollback.rs:406-413`), which today polls cancellation not at all.
6. **Fix the progress interval.** The inverse is built with `Duration::from_millis(0)`
   (`write_operations/rollback.rs:168`), so a throttle would never engage and a large rollback would emit an event per
   item. Use the interval a normal transfer uses.
7. Set the queue row's summary text, currently `OperationSummaryText::default()`, so the row isn't nameless.

**Keep.** The per-page streaming (`ROLLBACK_PAGE = 512`, documented so a million-item operation never materializes its
list), the newest-first reversal order, the typed skip reasons, and the paged flush.

**Tests.** Test-first, real red→green:

- Cancel inside a single large cross-volume file leaves no partial at the destination and nothing lost at the source.
- A paused rollback stops advancing and resumes where it left off.

Per `docs/testing.md`, drive cancel and pause **through the public interface**, never by mutating the intent atomic
directly; `write_operations/` is a named hot spot and direct state-machine mutation in tests is a listed anti-pattern.

Written after: progress events are monotonic and reach their totals; the summary text appears on the queue row.

**On the existing suites**: the shared rig (`operation_log/rollback/test_support.rs:136`) calls `execute_rollback`
directly with no sink and no state, so widening the signature forces the rig to change and `rollback/tests.rs` and
`undo_tests.rs` recompile against it. Every _assertion_ must survive unedited; they pin the skip-on-drift contract.

**Docs.** `operation_log/CLAUDE.md` (the rollback must-know) and `DETAILS.md` (the rollback contract).

**Checks.** `pnpm check rust`, `pnpm check`, and `pnpm check --include-slow` to close the milestone, since it touches
cross-volume transfer.

---

## M3: A button in the history dialog

**Intent.** Ship what was missing, on an engine that by now is correct (M1), observable, and cancelable (M2).

**Work.**

1. A thin IPC command in front of the existing fire-and-forget `dispatch_rollback`. The awaiting `undo_operations` stays
   for Ask Cmdr, which needs a tally to report.
2. The frontend wrapper goes in the existing `src/lib/tauri-commands/operation-log.ts` alongside `undoOperations`, with
   its case added to the existing `operation-log.test.ts`.
3. The per-row button in `OperationLogDialog.svelte`, enabled off the rollback-state badge the dialog already renders.

**No new phase is needed.** `WriteOperationPhase::RollingBack` already exists and the transfer dialog already words it.
The gap was only that the engine emitted nothing.

**Design decisions, with the why.**

- **The bar counts forward here.** The backwards bar earns its meaning on the transfer dialog because it drains a bar
  already on screen and full. A rollback from history opens a _fresh_ bar, where "full" would mean "nothing done yet",
  inverted from every other bar in the app, with rate and ETA hanging off a shrinking number. So: a forward bar marked
  as a reversal (distinct treatment, undo icon, wording like "Putting 1,240 files back"). Confirmed with David.
- **`RollbackConfirmDialog` cannot be reused verbatim.** Its body says rollback "deletes every destination the operation
  has written", which is right for undoing a copy and wrong for undoing a move, where the inverse is a restore. The
  history dialog needs copy that matches the operation being undone. This is human-facing copy, so it's David's call.
- **Volume directory rollback is still partial until M4.** Rolling back an SMB folder copy from history reverses the top
  level and skips every inner leaf as unverifiable. Safe, honest, confusing. See question 1.

**Tests.** Component tests: the button shows only for a rollbackable row, routes through the confirm dialog, fires with
the right id. Per `docs/testing.md`, a **new user-visible flow owes one E2E happy-path spec**, and a **destructive
`#[tauri::command]` owes an IPC contract test** in `lib/ipc/`; both apply here and the first draft skipped both. Merge
new a11y cases into the existing `OperationLogDialog.a11y.test.ts` rather than adding a file.

**Docs and generated surface.** A new `CLAUDE.md` + `DETAILS.md` pair for `src/lib/operation-log/` (it has neither),
**linked into the doc graph** or `docs-reachable` fails, which is an error rather than a warning. Update
`src/lib/file-operations/CLAUDE.md` + `DETAILS.md`, and `docs/specs/index.md`. Correct `mcp/DETAILS.md:183`, which says
there's no interactive rollback confirmation yet and is invalidated by this milestone. The new IPC command is `specta`
surface, so regenerate bindings and expect `desktop-bindings-fresh` to gate it. New user-facing strings go through the
i18n message files with `@key` descriptions across every locale, and the copy is a draft for David.

**Checks.** `pnpm check desktop`, then `pnpm check`.

---

## M4: Make the journal a faithful ledger

**Intent.** Close the gaps that make the journal an incomplete record of what an operation did. **This stands on its own
merits** whether or not unification ever happens: it closes the documented snapshot-completeness limit that today makes
every cross-volume directory rollback a partial.

**The four gaps, correctly sized.**

1. **Created directories are journaled only on the success path** (`transfer/copy/mod.rs:575`; `volume/copy.rs:1032`
   gates on no-error-and-not-cancelled). A canceled operation has zero directory rows. Small fix. Same shape, also
   unlisted until now: a **cross-FS local move journals no directory rows at all**, since `record_created_dirs` is
   reached only from the copy path, so its rollback leaves empty destination trees behind even on success.
2. **An interrupted operation's completed work is invisible.** Volume journaling happens per top-level source _at
   completion_, so an interrupted **directory** source contributes zero rows for all of its already-fully-copied
   children; they exist only in the in-memory ledger. Plus `volume/copy.rs:1055-1071` folds partially-written
   destinations into the rollback set, and those never had a row. **This is the real work of the milestone**, and it is
   bigger than "a partial file".
3. **Volume inner leaves have no snapshot** (`journal.rs:220-243` passes `None, None`), so they skip as unverifiable.
   **This is cheap, contrary to the first draft.** Record **size only**: the byte count is already in hand, since
   `copy_leaf` returns it and takes a size hint from the `list_directory` the walker already performed. Threading it
   through the created-paths record into `record_volume_transfer_source` costs **zero** extra round trips, and top-level
   volume rows already verify on size alone. **Do not record mtime**: the volume write path doesn't preserve it (which
   is exactly why the existing top-level row records `None`), so capturing the source's mtime would flip every leaf from
   "unverifiable" to "drifted", which reads to the user as "you changed this file". Strictly worse. No benchmark needed.
   Verified: `verify_snapshot` (`operation_log/rollback.rs:184-215`) is flat over the row (any recorded field must
   match, at least one must verify) with no inner-versus-top-level branch, so a size-only leaf verifies and reverses.
   The one cost worth knowing: the created-paths record grows a size alongside each path, which ripples mechanically
   through the concurrent copy's path collection.
4. **Pre-finalize eligibility.** An operation's header opens as `not_rollbackable` on purpose
   (`operation_log/writer.rs:373-378`) so a crash before finalize leaves it honestly unrollbackable, and eligibility is
   computed at finalize. Any in-flight rollback needs an answer here that doesn't weaken that property. Genuine design
   question.

**Tests.** Test-first for each, since each is a data-safety claim: a canceled copy's created directories are recorded;
an interrupted volume directory copy's completed children are recorded; a volume directory copy's inner leaves verify
and reverse rather than skip.

**Docs.** Rewrite `operation_log/DETAILS.md` § "Known snapshot-completeness limit"; it should shrink or disappear.

**Checks.** `pnpm check rust`, `pnpm check`, `pnpm check --include-slow`.

---

## M5: Collapse the three into one (optional, decide after M4)

**Intent.** With the journal trustworthy, retire the two in-memory rollbacks so there's one mechanism, and so the
in-flight path gains the snapshot recheck it lacks today.

**The decision is not about cost on SMB.** M4.3 turned out cheap, so the first draft's stated gate ("stop if faithful
capture is too expensive") doesn't apply. What the decision should turn on is the three real blockers below.

**Blocker 1, and it belongs in M2's interface, not here.** `is_cancelled` is `intent != Running`
(`operation_intent.rs:53-55`), with a pinned test asserting `RollingBack` reads as cancelled. Today's journal rollback
survives only because it runs as a _separate_ managed operation with a fresh `Running` state. Run the engine against the
original operation, whose intent is `RollingBack`, and the loop bails on its first iteration while the pause gate
returns instantly: **nothing is reversed and the app reports a successful rollback.** Use a fresh state instead and the
`RollingBack → Stopped` transition ("stop undoing, keep the rest") no longer reaches the rollback. The cancel predicate
must distinguish "the original operation is rolling back" from "stop the rollback". **Design that into M2's signature**
so M5 doesn't reopen it.

**Blocker 2.** `MoveTransaction` is not a pure ledger; it's a required `&mut` parameter of `move_resolved_into_place`
and the merge walker, with two throwaway instances on the staging path existing only to satisfy the signature. Removing
it is a refactor across three call paths.

**Blocker 3.** `volume_rollback_with_progress` has a caller beyond the copy and move paths: `archive_edit/move_out.rs`
inherits it through the volume copy, pinned by an existing test.

**Also.** `move_with_staging` creates a `CopyTransaction` and **never commits it**, so on every success path the `Drop`
impl logs "dropped without commit" and runs rollback. It's harmless today only because phase 3 already renamed every
staging path away, so each removal fails silently. Worth fixing on its own, and it means M5's panic-cleanup replacement
has to reckon with a net that currently fires on success too.

**Work, if it goes ahead.** Flush the journal as a barrier on cancel-with-rollback, then run the journal-driven rollback
against the partially-complete operation. Delete `transfer/copy/rollback.rs`, `CopyTransaction`'s rollback path,
`MoveTransaction`, and `volume_rollback_with_progress`. Replace the panic-cleanup safety net before deleting it: the
settle path does fire on panic, but a panicked copy that overwrote anything computes as not-rollbackable and would get
no cleanup where `Drop` cleans today. Keep the backwards bar by inverting in the frontend (the engine reports forward,
the dialog subtracts from its cancel-point values); after M4 the journal has real sizes, so that bar becomes exact
rather than linearly interpolated.

**Unresolved.** Whether archive and zip-edit operations join the unified engine. Compress is journal-rollbackable but
has no in-flight path and spawns outside the normal starter, and routing "delete one file inside an archive on an SMB
volume" through a transfer-shaped engine is a poor fit. Decide in or out before starting.

**Tests.** Test-first: a destination modified after the copy wrote it is **not** deleted by rollback (the data-safety
win; fails today). Panic cleanup: write it before deleting the `Drop` impl, confirm green with the old net, confirm
still green on the new path (green-then-green is correct here, because the behavior must not change). Plus: a canceled
copy leaves no empty directory tree, and no truncated partial survives.

**Docs.** `write_operations/CLAUDE.md`, `transfer/CLAUDE.md`, `transfer/volume/CLAUDE.md` + `DETAILS.md` (the cleanup
helpers and § "Overwrite isn't reversible"), the frontend `file-operations/transfer/` docs, `docs/architecture.md`. Any
`❌` line that becomes unrepresentable gets deleted rather than reworded; `invariant-density` is exempt from allowlist
consent, so adjust it freely.

**Checks.** `pnpm check rust`, `pnpm check`, `pnpm check --include-slow`.

---

## What this leaves alone, on purpose

- **No redo.** The inverse is marked not-itself-rollbackable. Undoing an undo re-applies what the person just chose to
  undo, and nobody asked.
- **Overwrite stays irreversible.** No per-file backup of a replaced original is kept, because keeping one for a whole
  operation can fill the user's disk on a large overwrite. That decision lives next to the code. It's also exactly why a
  confirmation sits in front of Rollback.
- **The awaiting `undo_operations` stays**, because Ask Cmdr's rename undo needs a tally.

## Sequencing

Strictly sequential; nothing here is safe to parallelize. M1 is independently valuable and should land regardless of
what happens to the rest, since it fixes shipped data-safety bugs. M2 and M3 deliver the requested feature. M4 stands
alone. M5 is a decision to make after M4, not a commitment now.

## Questions for David

1. **M3 before or after M4?** Shipping the button at M3 means a history rollback of an SMB _folder_ copy reverses the
   top level and skips every inner file, honestly reported but confusing. Options: (a) ship M3 with wording that
   explains the partial, (b) ship M3 with the button disabled for that case, (c) hold M3 until M4 closes the gap.
   Recommendation: (a), since the same limitation already applies to the MCP and Ask Cmdr paths shipping today.
2. **Commit to M5 now, or decide after M4?** Recommendation: decide after M4. The blockers are the cancel-predicate
   design, the `MoveTransaction` refactor, and the archive caller, none of which get cheaper by committing early.
3. **Copy** for the reversal phase, the queue row ("Putting 1,240 files back"?), and a confirmation that words undoing a
   _move_ correctly rather than reusing the delete-flavored one. Human-facing, so yours per principle 4.
4. **Foreground dialog or straight to the queue** for a rollback launched from history? Recommendation: straight to the
   queue, since the user is in a history dialog rather than watching a transfer.
