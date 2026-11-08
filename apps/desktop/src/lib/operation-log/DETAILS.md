# Operation log (frontend) details

Depth behind `CLAUDE.md`. The durable journal, its schema, and the rollback engine live on the backend
(`crates/cmdr-operation-log/DETAILS.md`); this doc covers only the dialog over it.

## File map

- `operation-log-trigger.svelte.ts`: the reactive state and the open/close seam, modeled on the What's-new trigger
  (`$state` needs a `.svelte.ts` file, and `routes/(main)/+page.svelte` mounts the dialog against
  `operationLogState.open`). Reads the newest 50 on open, appends 50 per "Load more", and exposes
  `markOperationRollingBack` and `refreshOperation` (one header, re-read in place). A failed append keeps `hasMore`, so
  Load more stays up as the retry rather than making the list read as the whole history.
- `OperationLogDialog.svelte`: the shell. Holds the list cursor (by `opId`, so the list can refresh under it), one tree
  source, the action bar (`ModalDialog`'s footer: "N of M selected", `RollbackControls`, Close, the rollback button),
  the pending rollback question, and per-operation refusals.
- `OperationList.svelte`: the left pane, a one-column `ColumnList` `grid` whose cursor is the selection. Each row
  carries `data-op-id` (the E2E hook), the summary, who and when, a `StatusGlyph` when the operation didn't run to its
  end, and the rollback state in its color. "Load more" sits under it.
- `OperationDetail.svelte`: the right pane's header (`#op-head-<id>`, badges, the route line, the standing notice or
  refusal as `#op-reason-<id>` / `.op-refusal`) over `OperationTree.svelte`.
- `OperationTree.svelte`: a `ColumnList` `treegrid` (name with indent, disclosure, checkbox; size; status), the header's
  select-all box, the cursor row's story under the tree, ← / → through `#lib/ui/tree-cursor.ts`, and the blank states
  (loading, backfilling, indexes building, a running operation, gone).
- `operation-tree-source.svelte.ts`: § "The tree source" below.
- `operation-log-labels.ts`: pure, no I/O. Summary formatting (ICU plural over `kind` + a count: the planned
  `itemCount`, or for a canceled or stopped operation the `itemsDone` it got to, wrapped in "then canceled (N planned)",
  since the planned count there claimed a whole copy that never happened), the enum label mappings, the action bar
  (`rollbackAction`, `rollbackActionLabel`, `selectedCountText`), the standing notices (`rowStandingNotice`,
  `partiallyRolledBackSummary`), and `rollbackRefusalNotice`. `rollbackConfirmVariant` is NOT here: it lives in
  `#lib/file-operations/reversal-wording.ts` with the type it returns, so `queue/` and `#lib/status-corner/` (which name
  the running reversal off the same variant) can reach it without depending on this module, which depends on that one.
- `operation-tree-labels.ts`: pure. A row's status (kind-aware: "Removed", "Moved back", "Renamed back", "Tags
  restored", "Skipped", "Didn't finish", a folder's "3 rolled back, 2 skipped"), its story (`treeRowTips`: skip reason,
  "comes back with its folder", why a folder won't unfold, the other end), the route line, and the blank notices.
- `operation-tree-test-fixtures.ts`: rows and windows in the wire shape, for the component and source tests.
- `RollbackControls.svelte`: the Pause / Resume and Cancel a row carries while its reversal runs. One per rolling-back
  row, binding a session to the ROW'S `inverseOpId`, and reporting the reversal's end (`onEnded`) so the row re-reads.
- `rollback-refusal.ts`: `RollbackRefusalFailure` / `throwRollbackRefusal` / `asRollbackRefusal`, the three-line
  `TypedFailure` family (`#lib/ipc/typed-failure.ts` is the pattern).
- `recording.ts`: `isOperationLogRecording()`, the one check every right-after-the-operation Undo makes before offering
  itself (the trash toast and Edit > Undo move to trash, Undo rename, Ask Cmdr's rename undo). With
  `operationLog.enabled` off the backend journals nothing new, so those undos could only be refused. The dialog reads
  the reactive `getOperationLogRecording()` instead and, while off, puts a `recording-off` notice above the list (in
  place of the empty state, whose "copy something and it'll show up here" would be untrue). Backend side:
  `crates/cmdr-operation-log/DETAILS.md` § "Turning recording off".
- `operation-log-shortcut.test.ts` pins the ⌥⌘L route; `operation-log-trigger.test.ts` owns the paging assertions, so
  the component tests don't repeat them.

## Layout

~900 × 640 by default, resizable (`fillBody`): the list takes 300 px on the left, the detail the rest, each with one
scroller (the list's and the tree's own `ColumnList` viewports). Under 640 px the list stacks above the detail, measured
with `useInlineSize` (Cmdr's WebKit floor predates container queries; `.stylelintrc.mjs` bans them). The Tab order
follows the DOM: the list, Load more, the tree, the action bar.

Deviations from the mockup, each for a reason:

- **The cursor row's story is a line under the tree**, not a popover under the row: an absolute popover inside a
  windowed row clips at the viewport and covers the rows below. The same words are each row's `aria-describedby`.
- **A small fresh tree opens its top folders by itself** (up to eight, when the whole tree fits one window), so the
  mockup's open folders read at a glance; anything bigger opens shut.
- **The tree's ↑ ↓ and ← / → work; Space, Insert, ⌘A, ⌘↩, and leaving the list by → are the keyboard pass's**, as is the
  `keyHint` on the rollback button.

## The tree source

`createOperationTreeSource()` is one view's state over `get_operation_log_tree`, modelled on the transfer tree's
(`#lib/file-operations/transfer-tree/transfer-tree-view.svelte.ts`): ❌ no tree model, only what the backend can't know.

- **One request in flight** (`createCoalesced`), built when SENT, so everything that changed meanwhile rides along.
- **Generation-tagged.** Every edit (a check, an expand, select-all, a refresh) and every operation switch bumps
  `generation`; an answer asked at an older one is dropped whole. The pressed checkbox shows its new state at once
  (`checkboxOf`, `allCheckbox`) and clears when the latest answer lands. Why: Space, Space fast otherwise lets the first
  answer's normalized selection wipe the second toggle.
- **The selection is the backend's.** A press appends one toggle to the last normalized selection; the answer's
  `selection` replaces it. One normalized back to `{ everything, [] }` reads as untouched (`null`), so the press sends
  nothing. A pick past the 2,000-toggle cap comes back `togglesCapped` with the newest dropped, which keeps the previous
  selection, and the tree says "pick a folder instead".
- **The cursor is a `TreeRowKey`.** The answer's `cursorIndex` puts it back on its row after an expand above it, or on
  its nearest open ancestor after a collapse; a cursor that moved while the answer was out stays where the person put
  it.
- **The operation settles first.** `show(op)` waits `SETTLE_MS` (150 ms) of rest before reading the tree, so holding ↓
  through 50 operations reads one tree and starts at most one backfill. The header renders from the list row at once. A
  load quicker than 200 ms shows no spinner.
- **An old operation backfills.** An `unbuilt(notBuilt)` or `building` answer starts (or joins)
  `start_operation_log_tree_backfill` once per settled operation; `operation-log-tree-backfill` events carry progress (a
  progress bar only past 10 s, a spinner and a line before) and the end (`built` reads the tree). Leaving the operation
  or closing the dialog cancels it. `indexesBuilding` (once, after an upgrade) asks again every second: the backend
  emits nothing when the indexes land. `operationRunning` / `rollingBack` just say so.
- **Read-only** (`readOnly`: a not-rollbackable operation, or one rolling back) takes no toggles; neither does a row
  inside a folder unit (`contents`), whose checkbox mirrors its folder, disabled.

## The rollback flow

1. The action bar offers a press on exactly the two states the backend's own gate admits (`rollbackAction` over
   `rowRollbackAction`): `rollbackable` earns "Roll back selected (N)" ("Roll back" at 0 of 0, or while the tree can't
   be read), `partiallyRolledBack` earns "Roll back the rest" until the person narrows the checks, then "Roll back
   selected (N)". The other three get nothing. At N = 0 with M > 0 the button is disabled; the engine's
   `nothingSelected` refusal is the backstop. At 0 of 0 with rows still waiting (a copied folder held back, now empty)
   the whole-operation press sweeps them: ❌ never a dead end.
2. Pressing it captures the operation, the action, and the selection to send (`null` unless narrowed) and stacks
   `RollbackConfirmDialog` with the variant `rollbackConfirmVariant(op.kind)` picks, `finishing` on a partly reversed
   operation, and `subset` when the selection leaves some out ("Roll back the selected items?").
3. `rollbackAsked` re-resolves the operation out of `operationLogState.entries` on every read and yields `null` once it
   stops offering a rollback. A reversal started elsewhere while the question is up therefore takes the question down,
   the way `QueueRow` withdraws its own.
4. Confirming calls `rollbackOperation(opId, 'user', selection)` and, on the `Ok`, flips the row to `rollingBack`,
   records the dispatch's `inverseOpId`, and re-reads the tree (now read-only). The `dispatching` set guards the window
   between the two so a double press can't double-dispatch.
5. A rejection lands in `refusals`, keyed by opId, and takes the standing notice's slot as a `role="status"` line. The
   button stays, because every refusal here is a race the person can respond to.
6. While the operation reads `rollingBack` and names an `inverseOpId`, the action bar carries `RollbackControls.svelte`.
   When the reversal ends, the dialog re-reads the header and the tree, and drops the picks: they described a state
   that's gone, and "Roll back the rest" is the natural next press.

### Decision: straight to the queue, no foreground dialog

**Why.** A rollback launched from history opens no progress dialog. The status corner already surfaces running
operations and opens the full queue on click, and someone reading their history isn't in the posture of watching a
transfer. Handing off keeps the history dialog a history dialog.

**What replaces the progress feedback.** The row's own badge, which changes under the cursor that clicked. That's honest
rather than optimistic: `dispatch_rollback` gates and writes `rolling_back` to the journal synchronously, before it
returns, so an `Ok` means the state is already durable. Re-reading the row to learn what we already know would cost a
round trip and a spinner for nothing.

**How the end arrives.** Not from the dispatch: the row's controls follow the reversal's live session, and when it ends
the dialog re-reads that one row, so `rolledBack` versus `partiallyRolledBack` shows up in place ("Caching and
staleness" below).

### Decision: the row commands the reversal, and the id for it is journal truth

A rollback over a slow mount can run for a long time, and the dialog that started it is where a person looks to stop it.
The engine has always supported both — it polls its stop flag through `StopMeans::IntentLeavesRunning` and parks on its
pause gate once per item — so this is buttons, not machinery.

**What they command.** The REVERSAL, which is its own managed operation, never the finished operation the row names.
Pausing a row's own `op_id` would be a no-op at best; on the wrong id it would park something else entirely.

**Where the id comes from.** `OperationRow.inverseOpId`, filled by every journal read (the backend's own
`crates/cmdr-operation-log/DETAILS.md` holds why every reader fills it) and by the dispatch's own answer in step 4. ❌
Not two sources: both carry the same journal fact, which is why a reversal started in another window, by an agent over
MCP, or before this dialog opened gets the same buttons as one started under the user's cursor.

**Through the session, so they're the queue window's buttons.** `bindOperationSession` on the inverse id, then
`togglePause()` / `cancel()`. The in-flight guards live on the shared session, so a press here disables the button there
and the other way round. ❌ Never call `pause_operation` / `cancel_operation` from a view.

**What decides whether they show.** The session's SNAPSHOT status, not the journal row: `running` or `paused` earns
Pause/Resume, and those plus `queued` earn Cancel. That's also what makes a stale `rolling_back` row safe — the dialog
reads the journal once, on open, so a reversal that has since ended leaves a row still badged "Rolling back", and the
absent live status is what stops it offering a press with nothing to press.

**And what says it has ended, for an operation that never says so itself.** Three readings, because a reversal ends more
quietly than a transfer. `settled` catches the terminal events; a snapshot saying `done` catches the status. Neither
ever lands here: the reversal emits `write-progress` and no terminal event at all, and its last word is dropping out of
the registry. That's `session.leftRegistry` (`../file-operations/operation-session/DETAILS.md` § "Leaving the
registry"), and it is the reading that actually fires in practice. ❌ Drop it and the buttons stay lit over a finished
reversal, where Cancel disables itself forever on a press nothing receives and Pause stays live beside it.

**They keep their labels whole.** The set is ONE flex item of the action bar (`.rollback-controls`) and it doesn't
shrink, or a narrow dialog would squeeze three buttons below "Resume", let alone "Szüneteltetés". `.btn-inner` is
`white-space: nowrap` on top of that, so a label that still doesn't fit keeps its width rather than breaking mid-word.

**A paused reversal must not read as finished.** The badge stays "Rolling back", because that's what the journal says
and it's true. The controls add the queue window's own status word, "Paused", off the same lifecycle status, so the two
surfaces can't word one park differently.

**Cancel lands on wording that already exists.** Stopping midway leaves the operation `partiallyRolledBack`, which the
detail already badges and already explains (`partiallyRolledBackSummary`, from the tree's counts), and already offers
"Roll back the rest" for (next section). ❌ No second sentence here saying it differently. The detail picks that state
up when the reversal leaves the registry, through the re-read in "Caching and staleness".

### Decision: a partly-reversed operation offers "Roll back the rest", and the engine is what makes that safe

A reversal a user cancels midway (or narrows to a subset) leaves the operation `partiallyRolledBack` with the untouched
items still sitting at the destination. Offering nothing there is a dead end reached by an ordinary action, so the bar
offers to roll back the rest. The reason that's safe is in the ENGINE, not in this dialog:

- **The op-level gate already allows it.** `check_rollbackable` (`crates/cmdr-operation-log/src/rollback.rs`) refuses
  `rollingBack`, `rolledBack`, and `notRollbackable`, and passes `rollbackable` and `partiallyRolledBack` alike. The
  `rolling_back` write happens synchronously before the dispatch returns, so two passes can never overlap.
- **Every per-item inverse is recheck-then-act, and re-entrant by construction.** A second pass streams every
  `rollback_unit` row of the ORIGINAL operation that still awaits reversal: the ones the first pass never reached, and
  the ones it held back (`RollbackUnit::awaits_reversal`), and rechecks each against its recorded snapshot. Items the
  first pass reversed are marked `rolled_back` and aren't re-read; one whose mark a crash lost reads as `NotFound` ⇒
  `SkipReason::AlreadyGone`, which `counts_as_reversed`, so it's credited without acting. Nothing is acted on twice, a
  run whose leftovers all clear lands `rolledBack`, and a pass that finds nothing to retry stays partial.
- **The two data-safety guards still stand on the second pass.** Drift and unverifiable snapshots skip, and a restore
  never overwrites a different entry. So a file the user created at a reversed item's old path after the cancel is
  skipped, not clobbered.
- **A selective pass is the same re-entry, narrowed.** `rollbackOperation` takes the item tree's normalized selection
  (`null` for everything); the engine normalizes it again and only REMOVES rows from the stream, so every row it takes
  gets the same recheck-then-act, and the rows it left out still await reversal, keeping the op `partiallyRolledBack`
  for the next pass. A selection it can't run is refused typed (`selection`: nothing selected, the summary not built,
  too many picks) before anything is set. Engine side: `crates/cmdr-operation-log/DETAILS.md` § "Streaming + ordering".

That's why the copy promises a pass rather than a completion: the same state also covers a reversal that RAN to the end
and skipped items it couldn't verify, and those skip again. `partiallyRolledBackNotice` (the fallback while the tree
can't count) says "Rolling back the rest takes another pass and skips anything Cmdr still isn't sure about" for exactly
that reason, and the counted one says how many were skipped and how many "haven't been rolled back yet". That last
phrase covers both an item someone left unchecked and one a cancelled pass never reached: the journal records them the
same way (a `done` row still awaiting reversal), so one honest phrase beats a guess. ❌ Don't reword either into a
promise that the rest will come back.

### Decision: the confirmation is worded by the inverse, not by the operation

`RollbackConfirmDialog` takes a `RollbackConfirmVariant` (`#lib/file-operations/reversal-wording.ts`), and
`rollbackConfirmVariant` maps `OpKind` to it with an exhaustive switch that mirrors the backend's `inverse_kind` arm for
arm. The SAME variant names the reversal while it runs, on the queue row, the corner chip, and the progress dialog:
`#lib/file-operations/DETAILS.md` § "The running reversal is named from the SAME variant".

The arms:

- copy / createFolder / createFile / archiveEdit → `undoByDeleting`
- move / trash → `undoByMovingBack`
- rename → `undoByRenamingBack`
- tag → `undoByRestoringTags` (each file gets its prior Finder tags back; nothing moves or is deleted)
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

- `OperationLogDialog.test.ts` drives the dialog against a MOCKED `get_operation_log_tree`: the list picks the detail,
  the tree's checkboxes and statuses render from an answer, a folder uncheck sends one toggle, the bar's words and its
  disabled state at 0 of M, that nothing is dispatched on a "no", that a subset sends the normalized toggles and says
  "selected", and the refusal notice. `operation-tree-source.svelte.test.ts` owns the request rules (generations, the
  settling cursor, backfill, the cap); `operation-log-labels.test.ts` and `operation-tree-labels.test.ts` own the
  exhaustive maps, so the component tests don't re-enumerate every state. `OperationLogDialog.a11y.test.ts` audits the
  dialog and each pane on its own.
- `RollbackControls.svelte.test.ts` mounts the dialog over a REAL session registry, fed the way the backend feeds one,
  and pins which control shows for which lifecycle status, that each press names the INVERSE operation, that a second
  press is refused while the first is out, and that a paused reversal says so. Two ordering traps it documents in place:
  a session claimed while the registry is empty seeds as `gone` and stays settled (so a test emits the snapshot BEFORE
  mounting), and the controls go away on the terminal EVENT, never on the operation leaving the snapshot.
- `apps/desktop/test/e2e-playwright/operation-log-rollback.spec.ts` drives the same flow against the REAL engine and
  real files, which is what pins the things a mock can't: that a real move is journaled as a move (so the question
  really does word itself as a restore), that a wire value like `alreadyRolledBack` is one the backend actually emits,
  and that a reversal launched from this dialog reaches the disk. It also covers the engine's three live controls end to
  end: forward progress, a pause that resumes where it left off, and a cancel inside one large file. Its cancel-midway
  spec then rolls back the rest, which is the only place the re-entry argument above is checked against a real journal
  and real files rather than reasoned about. ❗ Its helpers still address the old accordion (`li.op`, `.op-row`); they
  move to `[data-op-id]` and the action bar with the subset specs.

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
state has to choose between explaining itself and staying silent rather than falling silent by default. Three states
speak: `notRollbackable` (its reason, per above), `rollbackable` (what rolling back does to the selected items, by the
same reversal the confirmation names, so the checkboxes read as "these go" or "these come back"), and
`partiallyRolledBack` (counted from the tree when it can be). The action bar's button points its `aria-describedby` at
the detail's heading and, when there is one, that line.

**An interrupted operation says so too** (`coverageNotice`, exhaustive over `SearchCoverageReason`): one a crash cut off
ends at the next launch with `top_level_only(interrupted)` coverage, so the detail adds that Cmdr may not have recorded
everything and a rollback reaches only what it did. The other coverage reasons are name-search gaps a rollback doesn't
depend on, and stay quiet.

**The DETAIL is where those sentences actually land.** The button renders only on a state the gate admits, so a
`notRollbackable` operation never offers a press to refuse: routed only through the refusal path, every reason above
would be unreachable, and the user would face a bare "Can't roll back" badge. So the detail renders
`notRollbackableNotice` from the `notRollbackableReason` the row already carries over the wire, as `#op-reason-<id>`
under the header. A NULL reason (an operation still running, which opens `not_rollbackable` until finalize decides)
renders NOTHING: the badge stands on its own, and a dangling label would be worse than silence. The refusal notice still
wins the slot when a press earned one, since a race the user just lost outranks a standing explanation.

Two copy constraints that outlive any wording pass:

- **The merge line must not read as a mistake.** Merging a folder into a same-named one is what the user asked for and
  the right outcome; only the undo is unavailable. Copy that apologizes for the merge teaches people to fear a normal
  move.
- **Only `volumeUnavailable` ends in an action.** Every not-rollbackable reason is permanent, so none of them may hint
  at a retry, a setting, or a recovery. Inventing hope there costs more trust than the refusal does.

## Caching and staleness

The tree is read per window and never cached past the operation it shows: picking another operation starts fresh. The
operation HEADERS drift (a rollback anywhere changes `rollbackState`). Two things keep them honest while the dialog is
open, and nothing polls:

- **The local flip in step 4**, when this dialog starts a reversal.
- **A re-read when the shown operation's reversal ends.** `RollbackControls` fires `onEnded` once, off the same session
  readings that take its buttons away (`settled`, `leftRegistry`, or a `done` status; a session seeded as gone counts,
  since the journal read on open is then behind). The dialog calls `refreshOperation`, which reads the header alone
  (`getOperationLogDetail(id, 0, 0)`) and swaps it in, and re-reads the tree. That's safe on the signal because
  `execute_rollback` writes the final `rollback_state` over the writer's synchronous reply channel before `on_settled`
  drops the reversal from the registry.

What still drifts until it's picked: a reversal of an operation the cursor isn't on (its controls mount, seed as gone,
and re-read the moment it's picked) and one started ELSEWHERE while the dialog is open. A subscription to every header
change would close both; it was judged not worth a feed on a rarely open dialog.

**Rollbacks are never listed.** A rollback is not itself undoable (`crates/cmdr-operation-log/DETAILS.md`), so the
backend leaves inverse operations out of the list and search; the original's `inverseOpId` still names its running
reversal for `RollbackControls`. There are no links between an operation and its rollback.

## Where the copy lives

The dialog's own strings are `operationLog.*` (`#lib/intl/messages/en/operationLog.json`). The confirmation's strings
are `fileOperations.rollbackConfirm.*`, because the dialog belongs to `#lib/file-operations/` and its copy travels with
it (`#lib/file-operations/DETAILS.md` § "Rollback asks first").
