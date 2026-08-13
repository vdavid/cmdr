# File operations

Umbrella over the transfer (copy/move), delete/trash, new-file, and new-folder dialogs, triggered by Shift+F4 (new
file), F5 (copy), F6 (move), F7 (new folder), and F8 / Shift+F8 (trash / delete).

## Module map

- Subdirs with their own docs: `transfer/` (copy + move + the shared `TransferProgressDialog`), `delete/`, `mkdir/`,
  `mkfile/`, `operation-session/` (per-`operationId` event fan-out + the refcounted session registry), `queue/` (the
  standalone operation-queue window).
- Umbrella-level files: `TransferProgressReadout.svelte`, `scan-throughput.ts`, `foreground-operation.svelte.ts`,
  `foreground-request.ts`, `operation-conflict.svelte.ts`. What each one is: DETAILS § File map.

## Must-knows

- **Dialog copy lives in the i18n catalog, not in the components**: `messages/en/fileOperations.json` via `t()` /
  `tString()` / `<Trans>`, and hardcoding one fails `cmdr/no-raw-user-facing-string`. ⚠️ The transfer ERROR prose
  (`transfer-error-messages.ts`) is NOT ICU: it belongs to the `errors.write.*` pipeline
  (`$lib/error-messages/CLAUDE.md`), keyed per operation type. en output is parity-pinned, so a copy edit lands in the
  catalog AND the test together.
- **One dual-bar readout, two surfaces.** The progress dialog and the queue rows both render
  `TransferProgressReadout.svelte`; its cells are fixed-width by design, which is why the queue window's `MIN_WIDTH` and
  the dialog's 580 px exist.
- **The foreground slot is released on EVERY route out of the dialog**, Queue and auto-queue included: that handoff is
  when ambient surfaces start speaking about the operation. Release with `clearForegroundOperation(id)`, ❌ never an
  unconditional `setForegroundOperationId(null)` (a late teardown silences the next dialog's operation).
- **An error dialog is a HANDOVER, not a release.** `handleTransferError` passes the id to `setForegroundFailureId`
  while the dialog still owns it; closing releases it and dismisses the failure. Skip it and the chip and the toast
  announce what the user is already reading.
- **Rollback asks first, Cancel doesn't.** Every surface offering Rollback stacks `RollbackConfirmDialog` over itself
  and calls nothing until the answer comes back: rollback deletes everything the operation wrote, and a destination it
  OVERWROTE has no backup. ❌ Never a native `ask` (the queue window has no such capability, and E2E can't drive one),
  ❌ never a file count in it (the counter includes skips). DETAILS § "Rollback asks first".
- **A conflict for an operation no dialog owns is answered on the MAIN window** (`operation-conflict.svelte.ts`): pause
  what's running, prompt, resume exactly the ids paused. ❌ Never `resumeAll()` (it restarts a pause the USER made); ❌
  never decide ownership while `isForegroundClaimPending()` — defer, or you double-prompt or re-wedge the operation.
- **The backend arbitrates a clash and the answer names WHICH clash.** Answer through
  `session.resolveConflict(conflictId, ...)` with the id off the event on screen: anything but `resolved` means the
  question is settled without us, so take that prompt down and release the hold, ❌ never surface it as a failure. Only
  `null` (the call never landed) keeps it up, and a clash that arrived DURING the answer stays up.
- **`ScanThroughput` is SCAN-phase only** (the backend `EtaEstimator` owns every write phase), returns nulls until two
  samples land, and must be `reset()` between scans.

Backend counterpart: `apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md`.

Archive edits, the readout, the foreground slots, conflict prompts, and scan throughput: `DETAILS.md`. Read it before
any non-trivial work here: editing, planning, reorganizing, or advising.
