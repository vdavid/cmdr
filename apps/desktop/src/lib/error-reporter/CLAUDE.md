# Error reporter (frontend)

Flow A — user-initiated "Send error report" UI. Lets the user preview the redacted log bundle, optionally add a note,
and ship it to the api server. Phase 5 will add the auto-send flow on top of the same Tauri commands.

## Files

| File                             | Purpose                                                                |
| -------------------------------- | ---------------------------------------------------------------------- |
| `error-report-flow.svelte.ts`    | Reactive store + `openErrorReportDialog(initialNote?)` entry point     |
| `ErrorReportDialog.svelte`       | Preview-and-send dialog: preview, note textarea, manifest, send/cancel |
| `ErrorReportToastContent.svelte` | Post-send confirmation toast — shows the server-issued ID + Copy       |

## Entry points

- **Help menu → "Send error report…"** routes through `command-dispatch.ts`'s `help.sendErrorReport` case.
- **Inline button on error toasts** — `ToastItem.svelte` adds a "Send error report…" link to error-level toasts that
  carry a plain-text message. The toast text is pre-filled into the dialog's note textarea so the user starts from real
  context.

Both call `openErrorReportDialog(initialNote?)`, which flips the store flag the layout watches. The dialog mounts inside
`(main)/+layout.svelte` — same pattern as `CrashReportDialog`.

## Two-command split (matches backend)

The dialog calls `prepareErrorReportPreview` to render the preview (no network) and `sendErrorReport` to ship the
bundle. Two commands instead of one stateful "prepare-then-send" pair, because:

- Caching MB of zip bytes across IPC round-trips is wasteful — re-building is cheap.
- Holding bundle state on the Rust side risks leaks if the user dismisses without sending.
- The inputs (log file contents + user note) are deterministic enough that the preview matches the actual upload
  byte-for-byte modulo the timestamp.

See `apps/desktop/src-tauri/src/error_reporter/CLAUDE.md` for the backend rationale.

## ID handling

`prepareErrorReportPreview` returns a locally-generated `ERR-XXXXX` ID. **Display the preview ID in the dialog**, but
**only display the post-send toast with the server's response ID** — the server may regenerate the ID on a HEAD
collision. The `sendErrorReport` return value is the canonical one to show the user.

Bridge: `ErrorReportToastContent.svelte` exports `setLastSentReportId(id)` from a `<script module>` block. The dialog
calls it right before `addToast(component, ...)` so the toast can render the ID without the toast system needing to
forward props. Same pattern as `MtpConnectedToastContent`.

## User note caps

- Soft warning at 50 000 chars (counter appears, no other change).
- Hard limit at 100 000 chars (red border, "Send" disabled).
- Backend command also enforces 100 000 chars — both layers in case the textarea control is bypassed (paste, etc.).
- Server enforces a separate 10 MB total payload cap, which is mostly hit by logs, not the note.

## Dev affordance

In dev (`import.meta.env.DEV`), the dialog shows an extra "Save bundle to disk (debug)" button that calls
`saveErrorReportToDisk` and toasts the resulting path. The Tauri command is gated on `cfg!(debug_assertions)`; in
production it isn't registered, so calling the wrapper would return an error.

## Gotchas

- The dialog re-runs `prepareErrorReportPreview` on each note keystroke (debounced 250 ms). This is wasteful but cheap —
  the heavy work is reading + redacting log lines, which the OS is happy to keep in page cache.
- `errorReportFlow.initialNote` is captured when the dialog mounts via
  `let userNote = $state(errorReportFlow.initialNote)`. Subsequent edits to the textarea are local to the component —
  closing and reopening the dialog reads from the store again.
- `<script module>` blocks in Svelte 5 _do_ support `$state`. The compiler warns if you put module-level state in a
  regular `<script>` block by mistake.
