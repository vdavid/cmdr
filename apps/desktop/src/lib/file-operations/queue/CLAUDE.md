# Operation queue window

The standalone macOS window listing every running and waiting operation, plus the ones that couldn't finish: per-row
pause/resume/cancel/rollback/dismiss, multi-select + "Cancel selected", global pause/resume. Opens from View > Operation
queue (⌥⌘Q) or the palette. Backend: `apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md`.

## Module map

- `queue-window.ts`: the opener (`openQueueWindow`), cloned from `lib/settings/settings-window.ts`. Runs on the MAIN
  window; the queue window's own perms live in `src-tauri/capabilities/queue.json`.
- `operations-store.svelte.ts`: `createOperationsStore()` — the single reactive source the window renders from, merging
  two streams. Public API + the progress-dialog-facing seams: `DETAILS.md`.
- `QueueRow.svelte`: one operation row. Line 1 is chrome (select, icon, source→dest, status, actions); line 2 the shared
  `../TransferProgressReadout.svelte`, compact, or a failure's reason. Shell: `routes/queue/+page.svelte`.
- `failure-reason.ts`: `failureReasonFor(snapshot)` — a retained failure's title/explanation/suggestion, from the
  existing `errors.write.*` pipeline. Shared with the main window's failure toast.
- `queue-backlog.ts`: `hasOtherQueuedWork(rows, selfId)` — the pure test behind the progress dialog's Background/Queue
  label.

## Must-knows

- **It's a HARD window, not a modal**: keeping the main window usable while operations run is the whole point. A real
  `WebviewWindow` on `/queue`, sibling to Settings.
- **Progress is DEFINED with the copy dialog, not here.** Rows render `../TransferProgressReadout.svelte`, and every
  ESTIMATE (rates, smoothed ETA, scan rates) comes from the row's session via `bindOperationSession`, NEVER the raw tick
  (which the dialog smoothed and this window didn't, once showing one operation as "8m 12s" here and "5m 46s" there; the
  session is also what keeps a paused row's speed and countdown off the screen). ❌ Never keep a smoother in the store:
  it holds membership and the latest tick, both stateless. The readout's fixed-width columns set `queue-window.ts`'s
  `MIN_WIDTH`; the two move together.
- **Two streams, never poll.** `operations-changed` is the THIN membership + status snapshot; `write-progress` drives
  the live bars/ETA, keyed by `operationId` and pruned to snapshot membership. ❌ Don't fatten `operations-changed`.
- **Rows cover copy/move/delete/trash AND the instant ops** (`rename` / `create_folder` / `create_file`), which emit no
  `write-progress`, so they're a spinner + label with no bars. Icon and label arms take the SNAKE_CASE wire values, with
  documented fallbacks (`operation-icon.ts`).
- **A paused op still reports `is_running: true`.** The bar-is-moving truth is the SNAPSHOT `status`, NEVER
  `is_running`.
- **A running OR queued row can be `phase: 'scanning'`**: compact `ScanPhaseBody`, ❌ no dual bar (totals are 0), no
  Pause, no Rollback. DETAILS § "A scanning row".
- **A failed row STAYS until someone dismisses it.** The backend retains failures out-of-band (`write_operations`
  DETAILS § "Retained failures"), the page hides only `done` / `cancelled` (`isHiddenSettledStatus`, ❌ not
  `isTerminalStatus`), and Dismiss / "Dismiss all" (plus closing the error dialog that owns one) are the only ways out:
  no timer, no window close, no next operation. A 40-minute copy that died at lunchtime must still be there.
- **A failure's reason comes from the error pipeline, never new prose** (`failure-reason.ts` →
  `../transfer/transfer-error-messages.ts`, `getMessage()` raw lookup, per-operation variant keys). Its `message` is
  MARKUP, so it renders through `{@html}` like the dialog's body. The pipeline's own title is dropped in the row: it
  would read "Couldn't copy" right beside "Couldn't finish".
- **Show hands a row's operation back to the main window's progress dialog**, over `foreground-operation` carrying only
  the id (the main window resolves it against its own snapshot, `../foreground-request.ts`). ❌ Not a command on the
  operation, and ❌ never offered on a `queued` row (nothing to show yet). The main window may refuse and says so there.
  DETAILS § Show.
- **A row commands its own operation through its session** (`../operation-session/CLAUDE.md`), so the same press means
  the same thing from every surface and no two of them can send it twice. `routes/queue/+page.svelte` keeps only what
  ISN'T a command on one operation: Pause all / Resume all / Cancel selected, and dismissing a retained failure. ❌
  Don't route a per-row control back through the page.
- **Cancel keeps partials; Rollback is the separate, opt-in undo.** `session.cancel()` maps to `cancel_operation`: no
  rollback, no confirm (which is why `capabilities/queue.json` DROPS `dialog:allow-ask` and `store:default`), and it
  drops a still-`queued` row before it spawns. `session.rollback()` calls `cancelWriteOperation(id, true)` and shows
  ONLY where `supportsRollback` says so, never inferred from the type.
- **Window perms fail SILENTLY.** Every Tauri call here is `await`ed in try/catch with a `log.warn`; smoke-test with
  `pnpm dev` after a perm change.
- **Each child window is its own webview**, so the page inits and tears down its own i18n / theme / transparency / text
  size. Use `initWindowSettings()` (not `initializeSettings`): it also seeds the reactive layer `<Size>` reads
  (`lib/settings/CLAUDE.md`).
- **One opener, one factory, one instance per webview.** The Queue button and the auto-queue surfacing both call
  `openQueueWindow`; the main window holds its own store instance (`main-window-operations.svelte.ts`). ❌ Don't fork a
  second one.

Architecture, the store's full public API, retained failures, the vibrancy model, and decision detail: `DETAILS.md`.
Read it before any non-trivial work here.
