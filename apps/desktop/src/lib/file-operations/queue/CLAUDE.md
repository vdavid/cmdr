# Operation queue window

The standalone macOS window listing every running, waiting, and couldn't-finish operation: per-row
pause/resume/cancel/rollback/dismiss, multi-select + "Cancel selected", global pause/resume. Opens from View > Operation
queue (⌥⌘Q) or the palette. Backend: `apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md`.

## Module map

- `queue-window.ts` opens the window (perms in `src-tauri/capabilities/queue.json`), `operations-store.svelte.ts` is the
  single reactive source it renders from, `QueueRow.svelte` a row, shell `routes/queue/+page.svelte`.
- Pure helpers: `failure-reason.ts` (a retained failure's title/explanation/suggestion), `queue-backlog.ts`
  (`hasOtherQueuedWork`, behind the progress dialog's Background/Queue label).

## Must-knows

- **It's a HARD window, not a modal** (a real `WebviewWindow` on `/queue`, sibling to Settings): the main window stays
  usable while operations run.
- **Progress is DEFINED with the copy dialog, not here.** Rows render `../TransferProgressReadout.svelte` (whose
  fixed-width columns set `queue-window.ts`'s `MIN_WIDTH`), and every ESTIMATE comes from the row's session via
  `bindOperationSession`, ❌ never the raw tick. The session keeps a parked row's speed off the screen while its time
  left stays. ❌ No smoother in the store: it holds membership and the latest tick, both stateless.
- **Two streams, never poll**: `operations-changed` is the THIN membership + status snapshot, `write-progress` drives
  the live bars, keyed by `operationId` and pruned to snapshot membership. ❌ Don't fatten it with per-tick data.
- **Rows cover copy/move/delete/trash AND the instant ops** (`rename` / `create_folder` / `create_file`), which emit no
  `write-progress`, so they're a spinner + label with no bars. Icon and label arms (`operation-icon.ts`) take the
  SNAKE_CASE wire values.
- **A row with `snapshot.reverses` is an UNDO: name it from `../reversal-wording.ts`, ❌ never `operationType`**
  (undoing a copy runs as a delete, so it would read "Deleting"). Its status cell keeps the lifecycle word.
- **The bar-is-moving truth is the SNAPSHOT `status`** (the backend's `LifecycleStatus`), and the status column names
  that LIFECYCLE except where nothing else says the op stopped or reversed: an in-flight rollback, and an unanswered
  clash (`session.awaitingAnswer`, ❌ never a raw `progress.activity`). DETAILS § "A row parked on a clash".
- **A running, PAUSED, or queued row can be `phase: 'scanning'`**: compact `ScanPhaseBody`, ❌ no dual bar (totals 0) or
  Rollback; Pause/Resume DO show. DETAILS § "A scanning row".
- **A failed row STAYS until someone dismisses it**: the page hides only `done` / `cancelled` (`isHiddenSettledStatus`,
  ❌ not `isTerminalStatus`); Dismiss / "Dismiss all" (or closing the error dialog that owns one) are the only ways out.
  ❌ No timer, no window close.
- **A failure's reason comes from the error pipeline, ❌ never new prose** (`failure-reason.ts`). Its `message` is
  MARKUP through `{@html}`; the pipeline's title is dropped in the row.
- **Show hands a row's operation back to the main window's progress dialog**, over `foreground-operation` carrying the
  id alone. ❌ Not a command on the operation, ❌ never offered on a `queued` row. DETAILS § Show.
- **A row commands its own operation through its session** (`../operation-session/CLAUDE.md`); the page keeps only what
  ISN'T per-operation (Pause all / Resume all / Cancel selected, dismissing one).
- **Cancel keeps partials; Rollback is the separate, opt-in undo.** `session.cancel()` → `cancel_operation`: no
  rollback, no confirm (why `capabilities/queue.json` DROPS `dialog:allow-ask` and `store:default`).
  `session.rollback()` shows ONLY where `supportsRollback` says so, ❌ never inferred from the type.
- **Window perms fail SILENTLY**: `await` every Tauri call in try/catch with a `log.warn`, and smoke-test with
  `pnpm dev` after a perm change. Being its own webview, the page inits its own i18n / theme / transparency / text size
  (`initWindowSettings()`, `lib/settings/CLAUDE.md`).
- **One opener, one store per webview**: `openQueueWindow`, plus the main window's own
  (`main-window-operations.svelte.ts`).

Architecture, the store's public API, retained failures, the vibrancy model, and decision detail: `DETAILS.md`. Read it
before any non-trivial work here: editing, planning, reorganizing, or advising.
