# File operations

Umbrella over the transfer (copy/move), delete/trash, new-file, and new-folder dialogs: Shift+F4 (new file), F5 (copy),
F6 (move), F7 (new folder), F8 / Shift+F8 (trash / delete).

## Module map

- Subdirs with their own docs: `transfer/` (copy + move + the shared `TransferProgressDialog`), `delete/`, `mkdir/`,
  `mkfile/`, `operation-session/` (per-`operationId` event fan-out + the refcounted session registry), `queue/` (the
  standalone operation-queue window).
- Umbrella-level files: `TransferProgressReadout.svelte`, `scan-throughput.ts`, `foreground-operation.svelte.ts`,
  `foreground-request.ts`, `operation-conflict.svelte.ts`, `settled-operations.ts`, plus `mutation-error.ts` +
  `mutation-error-messages.ts` (the mutation-refusal path). What each one is: DETAILS § File map.

## Must-knows

- **Dialog copy lives in the i18n catalog, not the components**: `messages/en/fileOperations.json` via `t()` /
  `tString()` / `<Trans>`; hardcoding one fails `cmdr/no-raw-user-facing-string`. ⚠️ The transfer ERROR prose
  (`transfer-error-messages.ts`) is NOT ICU: it's the `errors.write.*` pipeline (`$lib/error-messages/CLAUDE.md`), keyed
  per operation type, and its en output is parity-pinned, so a copy edit lands in the catalog AND the test.
- **One dual-bar readout, two surfaces.** The progress dialog and the queue rows both render
  `TransferProgressReadout.svelte`; its fixed-width cells are by design, and are why the queue's `MIN_WIDTH` and the
  dialog's 580 px exist.
- **The foreground slot is released on EVERY route out of the dialog**, Queue and auto-queue included: that handoff is
  when ambient surfaces start speaking. Release with `clearForegroundOperation(id)`, ❌ never a bare
  `setForegroundOperationId(null)` (a late teardown silences the next dialog's operation).
- **A rename / mkdir / mkfile refusal stays TYPED to the surface.** Throw it with `throwMutationError` (a
  `TypedFailure`; ❌ `throwIpcError` flattens it to JSON), read it back with `asMutationError`, word it with
  `renderMutationError`. ❌ Never render `Unexpected.detail` or a `VolumeError` diagnostic as the message;
  `technicalDetail()` returns those separately. `timedOut` means the write may STILL LAND. DETAILS § "Mutation
  refusals".
- **An error dialog is a HANDOVER, not a release.** `handleTransferError` passes the id to `setForegroundFailureId`
  while the dialog still owns it; closing releases it and dismisses the failure. Skip it and the chip and toast announce
  what the user is already reading.
- **Rollback asks first, Cancel doesn't.** Every surface offering Rollback stacks `RollbackConfirmDialog` and calls
  nothing until the answer lands: rollback deletes everything written, and an OVERWRITTEN destination has no backup. ❌
  Never a native `ask`, ❌ never a file count in it. DETAILS § "Rollback asks first".
- **A conflict no dialog owns is answered on the MAIN window** (`operation-conflict.svelte.ts`): pause what's running,
  prompt, resume exactly the ids paused. ❌ Never `resumeAll()` (it restarts a USER pause); ❌ never decide ownership
  while `isForegroundClaimPending()` — defer, or you double-prompt or re-wedge it.
- **A clash answered ANYWHERE takes every surface's prompt down** (`write-conflict-resolved`): another window or an
  agent over MCP may have answered. Drop only the clash the event NAMES — the operation raises its next one the moment
  it takes an answer.
- **The answer names WHICH clash.** `session.resolveConflict(conflictId, ...)` with the id off the event on screen:
  anything but `resolved` means it settled without us, so take the prompt down and release the hold, ❌ never surface it
  as a failure. Only `null` (the call never landed) keeps it up. DETAILS § "Conflict prompts".
- **Journal rows become readable at `write-settled`, not at the terminal event** (the buffered tail flushes in the later
  finalize barrier). Reading on complete silently hands back an EMPTY page. Wait through `whenOperationSettled(id)`.
- **`ScanThroughput` is SCAN-phase only** (the backend `EtaEstimator` owns every write phase), returns nulls until two
  samples land, and needs `reset()` between scans.

Backend counterpart: `apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md`.

Mutation refusals, archive edits, the readout, the foreground slots, conflict prompts, and scan throughput:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
