# Operation log (frontend) details

Depth behind `CLAUDE.md`. The durable journal, its schema, and the rollback engine live on the backend
(`apps/desktop/src-tauri/src/operation_log/DETAILS.md`); this doc covers only the dialog over it.

## File map

- `operation-log-trigger.svelte.ts`: the reactive state and the open/close seam, modeled on the What's-new trigger
  (`$state` needs a `.svelte.ts` file, and `routes/(main)/+page.svelte` mounts the dialog against
  `operationLogState.open`). Reads the newest 50 on open, appends 50 per "Load more", and exposes
  `markOperationRollingBack`.
- `OperationLogDialog.svelte`: the dialog. Lazily fetches an operation's item rows on first expand and caches them for
  the dialog's lifetime, holds the pending rollback question, and renders per-row refusal notices.
- `operation-log-labels.ts`: pure, no I/O. Summary formatting (ICU plural over `kind` + `itemCount`), the four enum
  label mappings, and `rollbackRefusalNotice`. `rollbackConfirmVariant` is NOT here: it lives in
  `$lib/file-operations/reversal-wording.ts` with the type it returns, so `queue/` and `$lib/status-corner/` (which name
  the running reversal off the same variant) can reach it without depending on this module, which depends on that one.
- `rollback-refusal.ts`: `RollbackRefusalFailure` / `throwRollbackRefusal` / `asRollbackRefusal`, the three-line
  `TypedFailure` family (`$lib/ipc/typed-failure.ts` is the pattern).
- `operation-log-shortcut.test.ts` pins the ⌥⌘L route; `operation-log-trigger.test.ts` owns the paging assertions, so
  the component tests don't repeat them.

## The rollback flow

1. A row offers a button on exactly the two states the backend's own gate admits, and `rowRollbackAction` mirrors that
   gate: `rollbackable` earns "Roll back", `partiallyRolledBack` earns "Finish rolling back". The other three
   (`notRollbackable`, `rollingBack`, `rolledBack`) get nothing, because a press could only ever come back as a refusal.
2. Pressing it sets `rollbackAskedId` and stacks `RollbackConfirmDialog` with the variant
   `rollbackConfirmVariant(op.kind)` picks, plus `finishing` when the action is `finish`. Nothing has happened yet.
3. `rollbackAsked` re-resolves the row out of `operationLogState.entries` on every read and yields `null` once it stops
   offering an action. A reversal started elsewhere while the question is up therefore takes the question down, the way
   `QueueRow` withdraws its own: there'd be nothing left for an answer to act on.
4. Confirming calls `rollbackOperation(opId)` and, on the `Ok`, flips the row to `rollingBack`. The `dispatching` set
   guards the window between the two so a double press can't double-dispatch.
5. A rejection lands in `refusals`, keyed by opId, and renders as a `role="status"` line under the row. The row keeps
   its button, because every refusal here is a race the user can respond to.

### Decision: straight to the queue, no foreground dialog

**Why.** A rollback launched from history opens no progress dialog. The status corner already surfaces running
operations and opens the full queue on click, and someone reading their history isn't in the posture of watching a
transfer. Handing off keeps the history dialog a history dialog.

**What replaces the progress feedback.** The row's own badge, which changes under the cursor that clicked. That's honest
rather than optimistic: `dispatch_rollback` gates and writes `rolling_back` to the journal synchronously, before it
returns, so an `Ok` means the state is already durable. Re-reading the row to learn what we already know would cost a
round trip and a spinner for nothing.

**The consequence to keep in mind.** The dialog never learns how the reversal ENDED. Reopening the log (or reading the
queue) is how a user sees `rolledBack` versus `partiallyRolledBack`. If that ever needs to be live, subscribe to the
operation's settle event rather than polling.

### Decision: a partly-reversed operation offers to finish, and the engine is what makes that safe

A reversal a user cancels midway leaves the operation `partiallyRolledBack` with the untouched items still sitting at
the destination. Offering nothing there is a dead end reached by an ordinary action (cancelling), so the row offers to
finish. The reason that's safe is in the ENGINE, not in this dialog:

- **The op-level gate already allows it.** `check_rollbackable` (`src-tauri/src/operation_log/rollback.rs`) refuses
  `rollingBack`, `rolledBack`, and `notRollbackable`, and passes `rollbackable` and `partiallyRolledBack` alike. The
  `rolling_back` write happens synchronously before the dispatch returns, so two passes can never overlap.
- **Every per-item inverse is recheck-then-act, and re-entrant by construction.** A second pass re-streams every
  `rollback_unit` row of the ORIGINAL operation and rechecks each against its recorded snapshot. An item the first pass
  removed reads as `NotFound` ⇒ `SkipReason::AlreadyGone`, which `counts_as_reversed`, so it's credited without any
  filesystem call. An item the first pass restored is gone from the dest for the same reason. Nothing is acted on twice,
  and a run whose leftovers all clear lands `rolledBack`.
- **The two data-safety guards still stand on the second pass.** Drift and unverifiable snapshots skip, and a restore
  never overwrites a different entry. So a file the user created at a reversed item's old path after the cancel is
  skipped, not clobbered.

That's why the copy promises a pass rather than a completion: the same state also covers a reversal that RAN to the end
and skipped items it couldn't verify, and those skip again. `partiallyRolledBackNotice` says "Finishing takes another
pass and skips anything Cmdr still isn't sure about" for exactly that reason. ❌ Don't reword it into a promise that the
rest will come back.

### Decision: the confirmation is worded by the inverse, not by the operation

`RollbackConfirmDialog` takes a `RollbackConfirmVariant` (`$lib/file-operations/reversal-wording.ts`), and
`rollbackConfirmVariant` maps `OpKind` to it with an exhaustive switch that mirrors the backend's `inverse_kind` arm for
arm. The SAME variant names the reversal while it runs, on the queue row, the corner chip, and the progress dialog:
`$lib/file-operations/DETAILS.md` § "The running reversal is named from the SAME variant".

The arms:

- copy / createFolder / createFile / archiveEdit → `undoByDeleting`
- move / trash → `undoByMovingBack`
- rename → `undoByRenamingBack`
- delete → `undoByDeleting`, unreachable (a permanent delete is gated as not-rollbackable, so no button appears on one).
  It's mapped anyway so a NEW `OpKind` surfaces as a compile error here rather than as a confidently wrong sentence in
  front of a user.

**Why not one body for all of them.** The single body this dialog shipped with says rollback "deletes every file the
operation has written". That's true of a running copy and false of undoing a move, whose inverse restores and deletes
nothing. One wording has to over-warn on half the cases or under-warn on the other half.

**Why the second sentence is vague on purpose.** Every undo body ends with "Cmdr skips anything it isn't sure about, so
a few may…". Two different mechanisms produce a partial reversal: an item that drifted from its recorded snapshot, and
an item whose recorded snapshot the backend can't check against what sits there now. Naming either one would leave the
other unexplained, and "isn't sure about" is true of both. The clause also sets the expectation that a rollback may come
out partial, which is the honest read today.

## Where the tests live

- `OperationLogDialog.test.ts` drives the rollback flow against a MOCKED backend: which rows offer the button and with
  which words, that nothing is dispatched on a "no", the kind-aware wording, the standing notices, and the refusal
  notice. `operation-log-labels.test.ts` owns the two exhaustive maps behind the rows (`rowRollbackAction`,
  `rowStandingNotice`), so the component tests don't re-enumerate every state.
- `apps/desktop/test/e2e-playwright/operation-log-rollback.spec.ts` drives the same flow against the REAL engine and
  real files, which is what pins the things a mock can't: that a real move is journaled as a move (so the question
  really does word itself as a restore), that a wire value like `alreadyRolledBack` is one the backend actually emits,
  and that a reversal launched from this dialog reaches the disk. It also covers the engine's three live controls end to
  end: forward progress, a pause that resumes where it left off, and a cancel inside one large file. Its cancel-midway
  spec then FINISHES the reversal from the row, which is the only place the re-entry argument above is checked against a
  real journal and real files rather than reasoned about.

## Refusal notices

`rollbackRefusalNotice` maps each `RollbackRefusal` variant to its own line, plus a reason-free fallback for `null` (the
press that never reached the backend). One sentence each rather than one generic line, because the next move differs:
"already rolling back" points at the queue, "a volume is missing" asks the user to reconnect, and "already rolled back"
asks for nothing.

`notRollbackable` goes one level deeper: `notRollbackableNotice` words each `NotRollbackableReason` separately, because
the reasons aren't one situation. A directory merge and a resolved name clash LOST the information a reversal would
need; an overwrite and a permanent delete kept no bytes to restore; a zip-inner edit is a gap Cmdr hasn't closed yet; an
incomplete journal is Cmdr declining to guess. A single "this can't be rolled back" left the user unable to tell which,
and unable to tell whether they'd done something wrong.

`rowStandingNotice` is the one place that decides which states speak: it's exhaustive over `RollbackState`, so a new
state has to choose between explaining itself and staying silent rather than falling silent by default. Two states
speak: `notRollbackable` (its reason, per above) and `partiallyRolledBack` (one fixed sentence, since the journal
records no per-state detail the UI could word). A row's action button points its `aria-describedby` at both its own head
and, when there is one, that line.

**The ROW is where those sentences actually land.** The Roll back button renders only on a `rollbackable` row, so a
`notRollbackable` one never offers a press to refuse: routed only through the refusal path, every reason above would be
unreachable, and the user would face a bare "Can't roll back" badge. So the row renders `notRollbackableNotice` from the
`notRollbackableReason` it already carries over the wire, as a quiet `.op-reason` line under the row, and the row's own
button points at it with `aria-describedby` so a screen reader hears the badge and the why together. A NULL reason (an
operation still running, which opens `not_rollbackable` until finalize decides) renders NOTHING: the badge stands on its
own, and a dangling label would be worse than silence. The refusal notice still wins the slot when a press earned one,
since a race the user just lost outranks a standing explanation.

Two copy constraints that outlive any wording pass:

- **The merge line must not read as a mistake.** Merging a folder into a same-named one is what the user asked for and
  the right outcome; only the undo is unavailable. Copy that apologizes for the merge teaches people to fear a normal
  move.
- **Only `volumeUnavailable` ends in an action.** Every not-rollbackable reason is permanent, so none of them may hint
  at a retry, a setting, or a recovery. Inventing hope there costs more trust than the refusal does.

## Caching and staleness

Item rows are fetched once per operation on first expand and kept for the dialog's lifetime; they describe a finished
operation, so they don't drift. The operation HEADERS do drift (a rollback anywhere changes `rollbackState`), and
nothing refreshes them while the dialog is open beyond the local flip in step 4. Reopening the dialog re-reads page one.
That's the alpha's accepted limit; a live feed would subscribe rather than poll.

## Where the copy lives

The dialog's own strings are `operationLog.*` (`$lib/intl/messages/en/operationLog.json`). The confirmation's strings
are `fileOperations.rollbackConfirm.*`, because the dialog belongs to `$lib/file-operations/` and its copy travels with
it (`$lib/file-operations/DETAILS.md` § "Rollback asks first").
