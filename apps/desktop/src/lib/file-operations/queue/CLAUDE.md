# Transfer queue window

The standalone macOS window listing every running and waiting copy, move, delete, and trash operation, with per-row
pause/resume/cancel, multi-select + "Cancel selected", and global pause/resume. Backend: the operation manager in
`apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md`.

## Module map

- `queue-window.ts`: the opener (`openQueueWindow`), cloned from `lib/settings/settings-window.ts`. Runs on the MAIN
  window; the queue window's own perms live in `src-tauri/capabilities/queue.json`.
- `operations-store.svelte.ts`: `createOperationsStore()` — the single reactive source the window renders from, merging
  two streams. Public API + the progress-dialog-facing seams: `DETAILS.md`.
- `QueueRow.svelte`: one operation row. Line 1 is chrome (select, icon, source→dest, status, actions); line 2 the shared
  `../TransferProgressReadout.svelte`, compact. Shell: `routes/queue/+page.svelte`.

## Must-knows

- **It's a HARD window, not a modal.** The whole point is to keep working in the main window while transfers run; a
  modal would block that. So it's a real `WebviewWindow` on the `/queue` route, sibling to Settings / Shortcuts.
- **Progress is DEFINED with the copy dialog, not here.** Speed and ETA are the backend's; the ETA goes through
  `createEtaSmoother()` in `../progress-readout.ts` and rows pass `row.etaSecondsDisplay`, NEVER `progress.etaSeconds`
  (raw here while the dialog smoothed it once showed one operation as "8m 12s" in one window and "5m 46s" in the other).
  The bars, amounts, percents, rates, and time left are `../TransferProgressReadout.svelte`, whose fixed-width columns
  set `queue-window.ts`'s `MIN_WIDTH`: narrow past that and the bars vanish, so the two move together.
- **Two streams, never poll** (`subscribe, don't poll`). `operations-changed` is the THIN membership + lifecycle-status
  snapshot (the row set + each row's status); the existing per-file `write-progress` stream drives the live bars/ETA.
  The store keys progress by `operationId` and prunes it to current snapshot membership, so a finished op's bar can't
  linger. Don't fatten `operations-changed` with progress.
- **Rows cover copy/move/delete/trash AND the instant ops `rename` / `create_folder` / `create_file`.** Instant ops emit
  NO `write-progress`, so their rows are a spinner + label with no bars (`progress` stays null), usually flashing by
  before you can read them. `QueueRow`'s icon + `queue.row.label` arms use the SNAKE_CASE wire values (`create_folder`,
  not `createFolder`), or they fall silently to the `trash-2` / "Working" fallbacks (pure `operation-icon.ts`).
- **A paused op still reports `is_running: true`** from the backend status query (it stays in the write-op-state map).
  The bar-is-moving truth is the SNAPSHOT `status` (`'running'` vs `'paused'`), NEVER `is_running`. Rows read
  `snapshot.status`.
- **Cancel keeps partials; Rollback is the separate, opt-in undo.** Cancel (per-row and "Cancel selected") maps to
  `cancel_operation(s)`: no rollback, no confirm, which is why `capabilities/queue.json` DROPS `dialog:allow-ask` and
  `store:default`. Per-row **Rollback** calls `cancelWriteOperation(id, true)` and shows ONLY where the snapshot's
  `supportsRollback` says so — never inferred from the operation type.
- **Window perms fail SILENTLY.** Every Tauri call in `queue-window.ts` and `+page.svelte` is `await`ed in try/catch
  with a `log.warn`: a missing grant must surface as a log line, not a dead window. Smoke-test with `pnpm dev` after a
  perm change.
- **Each child window is its own webview** with its own i18n / theme / reduce-transparency runtime, so the page inits
  and tears down its own (`initWindowSettings`, language sync, `initAccentColor` / `initReduceTransparency` /
  `initTextSize`), mirroring Settings / Shortcuts. `initWindowSettings()` (not `initializeSettings`) also seeds the
  reactive layer `<Size>` reads, so sizes follow the user's binary/SI choice; see `lib/settings/CLAUDE.md`.
- **The opener is the shared reuse point.** The progress dialog's Queue button and the auto-queue surfacing (starting an
  op on a busy lane) both call `openQueueWindow` and read the same store; don't fork a second opener or a second store.

Architecture, the store's full public API, the vibrancy/reduce-transparency model, and decision detail: `DETAILS.md`.
Read it before any non-trivial work here.
