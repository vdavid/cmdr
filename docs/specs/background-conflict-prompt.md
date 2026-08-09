# The conflict prompt a backgrounded operation can't reach

**Status**: built. **Owner**: David. **Date**: 2026-08-09.

## The bug

`TransferDialog`'s upfront conflict check is one destination listing, top level only. Folders always merge, so a file
clash _inside_ a merged folder can't be found before the operation starts; it surfaces mid-operation. With the "Ask for
each" policy (`ConflictResolution::Stop`) the backend emits `write-conflict` and parks the operation on a oneshot,
waiting for `resolve_write_conflict`.

The only listener for `write-conflict` in the app is `transfer/transfer-progress-state.svelte.ts`, and it tears its
listeners down when the progress dialog hands the operation to the queue. So a backgrounded operation that hits a deep
clash waits for an answer nobody can give. The stall notice stays silent for the "a conflict prompt is open" state on
the reasoning that the dialog's own title says so, and when backgrounded there is no dialog. The operation wedges
invisibly, and the queue window shows it as `running` with a frozen bar.

Reachable today through the progress dialog's Queue button (F2); it predates the operation-queue work.

## The shape

A main-window host that owns conflict prompts for operations no foreground dialog is showing.

1. `lib/file-operations/operation-conflict.svelte.ts`, a headless controller started by `routes/(main)/+page.svelte`
   next to the failure watch. It listens to `write-conflict` for the life of the window.
2. On a conflict nobody in the foreground owns: **pause**, then prompt. The prompt is `OperationConflictDialog.svelte`,
   a `ModalDialog` whose body is the existing `TransferConflictDialog.svelte` — the same component, the same
   `resolveWriteConflict(operationId, resolution, applyToAll)` call, no second conflict UI.
3. Answering resolves the clash and resumes exactly what the prompt paused.

The prompt belongs to the OPERATION, not to a window. The controller never asks which windows are open, never talks to
the queue window, and would work unchanged if the queue became a popover inside the main window.

## Decisions, and what they rule out

### Who owns a conflict: one function, one seam

`conflictOwner(operationId, foreground)` in `operation-conflict-rules.ts` is the whole ownership test, pure and tested
per branch. It returns `here` / `foreground` / `unknown`; today `here` means "not the foreground dialog's operation".
The upcoming Foreground work (adopting a running operation back into the progress dialog) changes who holds the
foreground slot, not this function.

**The claim race is real, and it isn't fixed by a timer.** A conflict can arrive before the start command's response
gives `transfer-progress-state` its `operationId` — that's exactly why the dialog buffers events in `pendingEvents`. In
that window the foreground slot is empty, so a naive ownership test would prompt for an operation the modal is about to
own, and the user would get two prompts for one clash. So `foreground-operation.svelte.ts` grew a third piece of state:
a claim counter, incremented before the start command is dispatched and decremented once the id lands (or the dispatch
is abandoned). While a claim is pending the controller **defers** rather than deciding; the claim settling re-runs the
pass. Deterministic, no delay, and the deferred conflict is answered milliseconds later.

Rejected: a settle delay like the chip's `CHIP_SETTLE_MS`. The chip can afford to be late by design (it suppresses a
sub-second flash). A conflict prompt that guesses wrong either double-prompts or wedges, and neither is a thing to leave
to a timer.

A counter, not a boolean: a dialog's dispatch can still be in flight when the next dialog begins its own claim (Escape
during dispatch), and a boolean would clear the second claim with the first one's teardown.

### Pause: everything running, remembered by id

David's call is to pause everything for now, with "pause only this operation and let the parallel and next-in-line ones
continue" as the direction of travel. The seam is `operationsToPauseFor(conflictOperationId, rows)`: today it returns
every `running` id, and the later change returns the conflicting one (plus whatever shares its lane). Nothing else in
the controller knows how wide the pause is.

**The ids are remembered and resumed one by one, never `resumeAll()`.** `pauseAll()` / `resumeAll()` would resume an
operation the USER paused by hand before the conflict, which is a quiet way to override a person's explicit decision.
The controller pauses exactly the operations that were running, and resumes exactly those.

**The conflicting operation is paused too**, even though the backend already has it parked on the oneshot. It costs
nothing (the pause gate is a flag read at the next between-files boundary, and the operation is not at one — it is
blocked on the conflict) and it buys honesty: the queue window and the corner chip both read `paused`, so every surface
agrees that nothing is moving. The alternative left one row reading "Running" with a bar that never moves.

**Resolve first, resume second.** If the resolve IPC throws, the prompt stays up and the queue stays paused, which is
the correct state for an unanswered question. The resolved operation may park at its next boundary for the handful of
milliseconds before the resume lands; parking between files is what pause is built for, and cancellation still wins over
both.

### Several conflicts at once: a FIFO of one

Operations on disjoint lanes run in parallel and can each hit a clash. Within ONE operation the backend already
serializes prompts (`conflict_dispatch_lock` plus a single `conflict_resolution_tx` slot), so the queue only ever holds
one entry per operation.

The controller shows **one prompt at a time, in arrival order**, and resumes only once the last one is answered.
Stacking modals was rejected: the buttons in `TransferConflictDialog` say "Skip" and "Overwrite all", and two sets of
them on screen at once give no way to tell which operation each acts on. The context line above the body names the
operation for exactly that reason.

A second event for an operation already queued replaces its entry in place. It shouldn't happen (the backend serializes
per operation), but if it ever did, the newer event is the truthful one: `resolve_write_conflict` is keyed by operation
id alone, so an answer always lands on whatever clash that operation is currently parked on.

### The operation going away mid-prompt

Three ways out, all covered:

- **Cancel in the prompt**: `cancelWriteOperation(id, rollback)`, same call the progress dialog makes. The backend drops
  the oneshot sender, which unblocks the parked operation with `Cancelled`. The entry is dropped immediately rather than
  waiting for the row to disappear.
- **Cancel from the queue window, or the operation ending some other way**: an effect over the store's rows drops any
  queued prompt whose operation is no longer live. Nothing is left waiting on a promise nobody will keep.
- **Either one emptying the queue**: the remembered operations resume. A conflict that ends by cancellation must not
  leave the rest of the queue paused.

**The main window closing is not a case.** Closing it quits the app (`lib.rs`'s `CloseRequested` handler calls
`app_handle().exit(0)`), so there is no state where operations run without a main window to host the prompt. That is
also why the prompt needs no fallback host in the queue window.

### Raising the window

The realistic path into this bug ends with the queue window in front and the main window behind it. A prompt nobody can
see is the same wedge with more code, so opening one raises the main window (`getCurrentWindow().setFocus()`, the
self-focus pattern — cross-window `setFocus()` doesn't reliably raise on macOS). Skipped under `isE2eRun()`, like every
other focus call in the app.

Stealing focus is justified here and nowhere near the failure toast: a settled failure asks for no decision, while a
conflict prompt is holding a person's file operation still until they answer.

## Copy

Two new keys in `fileOperations.json`, plus one description edit.

- `fileOperations.operationConflict.context`: names which operation is asking ("Copying to Backup").
- `fileOperations.operationConflict.pausedNote`: shown only when the prompt actually paused something else, so it can't
  claim a hold that isn't there.

The dialog title stays `fileOperations.transferProgress.titleConflict` ("File already exists"): same question, same
words, and no new string for nine translators. Its `@key.description` widens to cover both hosts.

## What this deliberately doesn't do

- **No change to the foreground path.** A conflict for the operation the progress dialog owns is handled exactly as it
  is today, in the dialog's own body.
- **No second conflict component**, no forked resolve path, no new IPC, no new backend event.
- **No queue-window UI.** The queue may become a popover inside the main window later, so nothing here depends on it.
- **The upfront check stays top-level.** Making it recursive is a different (and much more expensive) change: it would
  walk the whole destination tree before every copy. Deep clashes are meant to surface mid-operation; the bug was that
  nobody was listening.
