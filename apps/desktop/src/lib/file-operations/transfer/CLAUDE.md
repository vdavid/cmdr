# Transfer (copy and move)

Frontend for copy (F5), move (F6), and compress (⌥F5): destination picker, dry-run conflict scan, dual-bar progress
dialog, error rendering. One set serves all via `operationType`; delete/trash reuse the progress dialog.

Backend: `apps/desktop/src-tauri/src/file_system/write_operations/transfer/CLAUDE.md` (copy/move semantics) and
`apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md` (state machine, ETA, settle contract).

## Module map

- `TransferDialog.svelte`: shell over `transfer-scan-state.svelte.ts` (deep scan preview),
  `transfer-conflict-check.svelte.ts` (cheap top-level check), and `transfer-dialog-logic.ts` (pure helpers).
- `TransferProgressDialog.svelte`: execution shell over `transfer-progress-state.svelte.ts` (the headless
  event/phase/cancel/pause/queue/conflict/scan-wait state machine), plus `TransferConflictDialog.svelte` and
  `transfer-stall.ts`.
- `TransferErrorDialog.svelte` + `FallbackErrorContent` render the typed `WriteOperationError`; plus
  `ArchivePasswordDialog`, `ScanPhaseBody`, `DirectionIndicator`, and the `transfer-*.ts` helpers.

## Must-knows

- **One transfer entry seam.** F5/F6, drag-and-drop, and paste all prepare through `pane/transfer-entry.ts`. The
  destination-guard copy is an E2E-asserted contract; don't reword it. `resolveSourceVolumeId` never returns a
  knowingly-wrong id. The paste path's MTP refusal stays SEPARATE and BEFORE the shared guard.
- **Batch IPC for selection lookups** (`get_paths_at_indices` / `get_files_at_indices`), never a per-index `getFileAt`
  loop: with 50k files that's 5-10 s vs ~1 ms.
- **Same-volume move disables Rollback and skips the deep scan preview** (source and dest the SAME non-default volume;
  the backend rename-merges server-side, zero-byte, no rollback). `DEFAULT_VOLUME_ID` is excluded, so local→local keeps
  both. Affordances disable with a tooltip; plain Cancel and the cheap conflict check stay live. ⚠️ A CROSS-volume move
  can't roll back either, and this dialog doesn't know yet (the backend says so via `supports_rollback`): DETAILS.
- **Speed and ETA are backend-owned, SHARED with the operation queue window** via `../progress-readout.ts` +
  `$lib/units`. ❌ No second instantaneous rate here; `ScanThroughput` is SCAN-phase only. So are the bars:
  `../TransferProgressReadout.svelte` draws both surfaces' labelled bars, amounts, percents, rates, and time left. Its
  fixed-width columns are why this dialog is 580 px wide; don't narrow it without them.
- **A stalled transfer drops the ETA and says why** (`transfer-stall.ts`). The BACKEND classifies; this side owns only
  the threshold. ❌ Never infer a stall from event timing: a wedge emits no events at all.
- **Rollback / Cancel disable during the settle window.** The dialog holds open `MIN_DISPLAY_MS = 400 ms` after
  `write-complete`; a click then hits an already-removed op and falsely flashes "Rolling back...". Gate on
  `disabled={isCancelling || operationSettled}`.
- **Cancel close waits for both `write-cancelled` AND `write-settled`** (a fast second F8 mid-teardown once wedged an
  MTP session) — ❌ but never as the ONLY exit: `progress.dismiss()` backs a Close button that leaves at once.
  `CANCEL_SETTLE_FALLBACK_MS` exceeds the backend's 15 s `CANCEL_DRAIN_DEADLINE`, so the automatic path can't report
  `0 files` before the real count lands.
- **`archive_needs_password` is intercepted UPSTREAM**, not by `TransferErrorDialog`: `handleTransferError`
  (`pane/dialog-state.svelte.ts`) shows `ArchivePasswordDialog`, re-dispatching on unlock.
- **Move refreshes BOTH panes** (source files gone); copy only the destination.
- **Flushing phase** (`phase: 'flushing'`) shows "Writing the last piece..." for the backend's closing `fdatasync`, a
  real multi-second pause on slow media; the bar mustn't sit frozen at 100%.
- **Confirm waits on the conflict check ONLY under `skip`** (MCP-only; the radios don't render while it runs): outside
  `Skip` the backend ignores `pre_known_conflicts`, so `conflicts: []` costs information, not safety. Awaiting paths
  disable the button and spin. ❌ Don't drop `handleCancel`'s `confirmed` guard: it's also `onclose`, and would free the
  preview under a pending dispatch.
- **`data-scan-state` on `.scan-stats`** is E2E's only race-free "counting done" signal; `DeleteDialog` mirrors it.
- **Compress swaps the conflict-policy UI for a dest-exists overwrite check**, and its auto-confirm (MCP) path must
  NEVER silently overwrite.
- **MTP move interleaves copy + delete per file** (the copy is done once the delete phase starts).
- **Pause/Resume and the "Paused" title follow the `operations-changed` snapshot status, never `is_running`.** Queue and
  the dialog-scoped F2 are FRONTEND-ONLY: set `backgrounded`, open the queue window, unmount via `onQueue` without
  cancelling — that flag makes `onDestroy` skip its safety-net cancel, and both release the foreground slot. That button
  reads "Background" with an empty queue, "Queue" otherwise (`../queue/queue-backlog.ts`); same action either way.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
