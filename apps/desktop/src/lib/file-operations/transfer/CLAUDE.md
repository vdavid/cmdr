# Transfer (copy and move)

Frontend for copy (F5), move (F6), and compress (⌥F5): destination picker, dry-run conflict scan, dual-bar progress
dialog, error rendering. One set serves all via `operationType`; delete/trash reuse the progress dialog. Backend
counterpart: `apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md` and its `transfer/` subdir.

## Module map

- `TransferDialog.svelte` is the setup shell (over `transfer-scan-state.svelte.ts`, `transfer-conflict-check.svelte.ts`,
  `transfer-dialog-logic.ts`); `TransferProgressDialog.svelte` is the execution shell (over
  `transfer-progress-state.svelte.ts`, plus `TransferConflictDialog.svelte`, `transfer-stall.ts`).
- `transfer-dispatch.ts` is birth: which backend command a confirmed copy/move/compress/delete/trash routes to. The rest
  (error, password, scan-phase, direction components and the `transfer-*.ts` helpers): DETAILS § File map.

## Must-knows

- **The dialog is a VIEW of its operation, ❌ never its owner.** Phase, counts, rates, smoothed ETA, clash, outcome, and
  every command come from its session (`../operation-session/CLAUDE.md`), shared with the queue rows and the corner
  chip. The view keeps only UI: `MIN_DISPLAY_MS`, dismissal, the settle-slow label, the cancel-settle fallback, the
  Queue handoff. ❌ Never a second smoother, listener, or event buffer.
- **A close is a DETACH, ❌ never a cancel.** `ModalDialog`'s `onclose` goes to `detach()`, handing a still-running
  operation to the queue window; only the Cancel button cancels, and unmounting stops nothing. With no session yet (the
  sub-frame after the id lands) it leaves the operation ALONE, exactly as `handleCancel` does: guessing would report a
  cancel over a live transfer. ❌ While a clash is up there's no `onclose` at all: every way out of a clash decides
  something about the user's files.
- **Queue and the dialog-scoped F2 are FRONTEND-ONLY** (set `backgrounded`, open the queue window, unmount via
  `onQueue`). ❌ `backgrounded` and `destroyed` stay plain `let`s: teardown reads them during reactive-scope disposal,
  where a rune returns a stale value, which is how a just-queued transfer once got cancelled.
- **One transfer entry seam**: F5/F6, drag-and-drop, and paste all prepare through `pane/transfer-entry.ts`. The
  destination-guard copy is E2E-asserted, and the paste path's MTP refusal stays SEPARATE and BEFORE the shared guard.
- **Batch IPC for selection lookups** (`get_paths_at_indices` / `get_files_at_indices`), ❌ never a per-index
  `getFileAt` loop: 50k files is 5-10 s vs ~1 ms.
- **Speed, ETA, and bars are backend-owned and SHARED with the queue window** (`../TransferProgressReadout.svelte`): ❌
  no second instantaneous rate here, and its fixed-width columns are why the dialog is 580 px wide.
- **A stall drops the ETA and says why** (`transfer-stall.ts`): the BACKEND classifies, this side owns the threshold. ❌
  Never infer a stall from event timing — a wedge emits no events at all. The notice is a warning-toned `SectionCard`
  above the buttons; ❌ don't hand-pick a yellow, the tone token owns both themes. `DETAILS.md` § "The stalled-transfer
  notice".
- **"Couldn't find out" is its own state in BOTH pre-confirm checks, ❌ never silence and ❌ never an empty answer.**
  Rendering nothing is exactly what a clean destination renders, and this feeds an overwrite decision. `DETAILS.md` §
  "When the dialog can't find out".
- **Rollback / Cancel disable during the settle window** (`disabled={isCancelling || operationSettled}`), and a cancel
  close waits for both `write-cancelled` AND `write-settled` — ❌ but never as the ONLY exit: `progress.dismiss()` backs
  a Close button that leaves at once.
- **The progress dialog does NOT wait for the scan; the BACKEND does.** It dispatches on mount, so a still-counting
  transfer has an `operationId`, a queue row, and Background from frame one. ❌ Never cancel the preview on teardown —
  the operation owns it. Confirm ALWAYS awaits `scan.scanStarted` (a null `previewId` means a concurrent re-walk plus an
  orphaned preview). DETAILS § Scan.
- **Compress swaps the conflict-policy UI for a dest-exists overwrite check**; its auto-confirm (MCP) path must NEVER
  silently overwrite.

Rollback's limits on a move, `handleCancel`'s `confirmed` guard, `archive_needs_password` interception, the
`data-scan-state` and `data-conflict-state` E2E markers, pane refresh after a move, flows, the phase catalog
(`flushing`, MTP's interleaved move), decisions, and gotchas: `DETAILS.md`. Read it before any non-trivial work here:
editing, planning, reorganizing, or advising.
