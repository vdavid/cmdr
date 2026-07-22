# Transfer queue window

The standalone macOS window that lists every running and waiting copy, move, delete, and trash operation, with per-row
pause/resume/cancel, multi-select + "Cancel selected", and global pause/resume. Backend counterpart: the operation
manager in `apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md`.

## Module map

- `queue-window.ts`: the opener (`openQueueWindow`), cloned from `lib/settings/settings-window.ts`. Runs on the MAIN
  window; the queue window's own perms live in `src-tauri/capabilities/queue.json`.
- `operations-store.svelte.ts`: `createOperationsStore()` — the single reactive source the window renders from. Merges
  two streams. Public API + the progress-dialog-facing seams in `DETAILS.md`.
- `QueueRow.svelte`: one operation row (type icon, source→dest, live bar + ETA, status, pause/resume + cancel, select
  checkbox). Route shell: `routes/queue/+page.svelte`.

## Must-knows

- **It's a HARD window, not a modal.** The whole point is to keep working in the main window while transfers run; a
  modal would block that. So it's a real `WebviewWindow` on the `/queue` route, sibling to Settings / Shortcuts.
- **Two streams, never poll** (`subscribe, don't poll`). `operations-changed` is the THIN membership + lifecycle-status
  snapshot (the row set + each row's status); the existing per-file `write-progress` stream drives the live bars/ETA.
  The store keys progress by `operationId` and prunes it to current snapshot membership, so a finished op's bar can't
  linger. Don't fatten `operations-changed` with progress.
- **Rows cover copy/move/delete/trash AND the instant ops `rename` / `create_folder` / `create_file`.** Instant ops emit
  NO `write-progress`, so their rows render with a spinner + label and no bar (`progress` stays null), usually flashing
  by before you can read them. `QueueRow`'s icon + `queue.row.label` arms use the SNAKE_CASE wire values
  (`create_folder`, not `createFolder`), or they silently fall to the `trash-2` / "Working" fallbacks; the icon mapping
  is the pure `operation-icon.ts` (unit-tested).
- **A paused op still reports `is_running: true`** from the backend status query (it stays in the write-op-state map).
  The bar-is-moving truth is the SNAPSHOT `status` (`'running'` vs `'paused'`), NEVER `is_running`. Rows read
  `snapshot.status`.
- **Cancel keeps partials, always (rollback = false).** Per-row Cancel and "Cancel selected" both map to
  `cancel_operation(s)` with no rollback and no confirm prompt: a queued op is dropped before it spawns; a
  running/paused op stops keeping copied files. That's why `capabilities/queue.json` DROPS `dialog:allow-ask` (no
  prompt) and `store:default` (no persistence in v1).
- **Window perms fail SILENTLY.** Every Tauri call in `queue-window.ts` and `+page.svelte` is `await`ed in try/catch
  with a `log.warn`. A missing grant must surface as a log line, not a dead window. Smoke-test with `pnpm dev` after any
  perm change.
- **Each child window is its own webview** with its own i18n / theme / reduce-transparency runtime, so the page inits
  them itself (`initializeSettings`, language sync, `initAccentColor` / `initReduceTransparency` / `initTextSize`) and
  cleans them up on destroy. Mirrors Settings / Shortcuts.
- **The opener is the shared reuse point.** The progress dialog's Queue button and the auto-queue surfacing (starting an
  op on a busy lane) both call `openQueueWindow` and read the same store; don't fork a second opener or a second store.

Architecture, the store's full public API, the vibrancy/reduce-transparency model, and decision detail: `DETAILS.md`.
Read it before any non-trivial work here.
