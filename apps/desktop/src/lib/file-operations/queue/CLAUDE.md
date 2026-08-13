# Operation queue window

The standalone macOS window listing every running and waiting operation, plus the ones that couldn't finish: per-row
pause/resume/cancel/rollback/dismiss, multi-select + "Cancel selected", global pause/resume. Opens from View > Operation
queue (⌥⌘Q) or the palette. Backend: `apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md`.

## Module map

- `queue-window.ts` opens the window (perms in `src-tauri/capabilities/queue.json`), `operations-store.svelte.ts` is the
  single reactive source it renders from, `QueueRow.svelte` is one row, shell `routes/queue/+page.svelte`.
- Pure helpers: `failure-reason.ts` (a retained failure's title/explanation/suggestion) and `queue-backlog.ts`
  (`hasOtherQueuedWork`, behind the progress dialog's Background/Queue label).

## Must-knows

- **It's a HARD window, not a modal** (a real `WebviewWindow` on `/queue`, sibling to Settings): keeping the main window
  usable while operations run is the whole point.
- **Progress is DEFINED with the copy dialog, not here.** Rows render `../TransferProgressReadout.svelte` (whose
  fixed-width columns set `queue-window.ts`'s `MIN_WIDTH`), and every ESTIMATE comes from the row's session via
  `bindOperationSession`, ❌ never the raw tick. The session is also what keeps a parked row's speed off the screen
  while its time left stays. ❌ Never keep a smoother in the store: it holds membership and the
  latest tick, both stateless.
- **Two streams, never poll**: `operations-changed` is the THIN membership + status snapshot, `write-progress` drives
  the live bars, keyed by `operationId` and pruned to snapshot membership. ❌ Don't fatten `operations-changed`.
- **Rows cover copy/move/delete/trash AND the instant ops** (`rename` / `create_folder` / `create_file`), which emit no
  `write-progress`, so they're a spinner + label with no bars. Icon and label arms (`operation-icon.ts`) take the
  SNAKE_CASE wire values.
- **A paused op still reports `is_running: true`** — the bar-is-moving truth is the SNAPSHOT `status`.
- **A running OR queued row can be `phase: 'scanning'`**: compact `ScanPhaseBody`, ❌ no dual bar (totals are 0), no
  Pause, no Rollback. DETAILS § "A scanning row".
- **A failed row STAYS until someone dismisses it**: the page hides only `done` / `cancelled` (`isHiddenSettledStatus`,
  ❌ not `isTerminalStatus`), and Dismiss / "Dismiss all" (or closing the error dialog that owns one) are the only ways
  out. No timer, no window close, no next operation.
- **A failure's reason comes from the error pipeline, ❌ never new prose** (`failure-reason.ts`). Its `message` is
  MARKUP, rendered through `{@html}`; the pipeline's own title is dropped in the row.
- **Show hands a row's operation back to the main window's progress dialog**, over `foreground-operation` carrying only
  the id. ❌ Not a command on the operation, and ❌ never offered on a `queued` row. DETAILS § Show.
- **A row commands its own operation through its session** (`../operation-session/CLAUDE.md`); the page keeps only what
  ISN'T a per-operation command (Pause all / Resume all / Cancel selected, dismissing a failure).
- **Cancel keeps partials; Rollback is the separate, opt-in undo.** `session.cancel()` → `cancel_operation`: no
  rollback, no confirm (which is why `capabilities/queue.json` DROPS `dialog:allow-ask` and `store:default`).
  `session.rollback()` shows ONLY where `supportsRollback` says so, ❌ never inferred from the type.
- **Window perms fail SILENTLY**: `await` every Tauri call in try/catch with a `log.warn`, and smoke-test with
  `pnpm dev` after a perm change. Being its own webview, the page also inits its own i18n / theme / transparency / text
  size (`initWindowSettings()`, `lib/settings/CLAUDE.md`).
- **One opener, one store instance per webview**: `openQueueWindow`, plus the main window's own store
  (`main-window-operations.svelte.ts`).

Architecture, the store's full public API, retained failures, the vibrancy model, and decision detail: `DETAILS.md`.
Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
