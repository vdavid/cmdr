# Error reporter (frontend) — details

Read before structural changes. `CLAUDE.md` holds the must-knows; this is the depth.

## Flow B: auto-send toast

When `updates.errorReports` is on, the Rust auto-dispatcher fires `error-report-auto-sent` (payload: server-issued
report ID) after a successful upload. `auto-send-toast.svelte.ts`, initialized from the main window layout's `onMount`,
listens and shows `addToast(AutoSendToastContent, ...)`:

- **Title**: "Error report sent". **Body**: reference ID badge.
- **Actions**: "View" reuses the Flow A preview dialog so the user can inspect what was shipped (the dialog re-builds
  the bundle locally, deterministic modulo timestamp). "Change settings" opens the Settings window to flip the opt-in.
- **Auto-dismiss after 10 s** (longer than the default 4 s): auto-sent reports are surprising, so the user needs more
  time to notice and act.

The listener is initialized in `(main)/+layout.svelte` next to the Flow A dialog mount and torn down in the matching
`onDestroy`. Idempotent: repeated `init` calls are no-ops.

## ID-bridging pattern

`error-report-toast-state.svelte.ts` holds the report ID in a module-level `$state` with `setLastSentReportId(id)` /
`getLastSentReportId()`. The dialog sets it right before `addToast(component, ...)` so the toast renders the ID without
the toast system forwarding props; the toast reads it via the getter. The state lives in a `.svelte.ts` module rather
than the toast's `<script module>` so its exports are typed across imports (a `.svelte` module export is seen as `any`).
Same pattern in `bundle-saved-toast-state`, `auto-send-toast-state`, and mtp's `mtp-connected-toast-state`.

## Note-capture timing and gotchas

- The dialog calls `prepareErrorReportPreview` exactly once on mount. The user note doesn't influence log content; it
  only lands in the manifest. The displayed manifest is rebuilt locally with the live note value, and `sendErrorReport`
  (and `saveErrorReportToDisk`) ship the current note when invoked. Rebuilding the multi-MB zip per keystroke would be
  wasteful for no behavioral gain.
- `errorReportFlow.initialNote` is captured on mount via `let userNote = $state(errorReportFlow.initialNote)`. Later
  textarea edits are local to the component; closing and reopening reads from the store again.
- `<script module>` blocks in Svelte 5 do support `$state`. The compiler warns if you put module-level state in a
  regular `<script>` block by mistake.
