# Error reporter (frontend)

Flow A — user-initiated "Send error report" UI. Lets the user preview the redacted log bundle, optionally add a note,
and ship it to the api server. Flow B — opt-in auto-send on user-visible errors — is wired here too: a tiny listener
turns the backend's `error-report-auto-sent` event into a confirmation toast.

## Files

| File                             | Purpose                                                                                                                                                                                                                                                                      |
| -------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `error-report-flow.svelte.ts`    | Reactive store + `openErrorReportDialog(initialNote?)` entry point (Flow A)                                                                                                                                                                                                  |
| `ErrorReportDialog.svelte`       | Preview-and-send dialog: preview, note textarea, manifest, send/cancel (Flow A)                                                                                                                                                                                              |
| `ErrorReportToastContent.svelte` | Flow A post-send confirmation toast — shows the server-issued ID + Copy                                                                                                                                                                                                      |
| `auto-send-toast.svelte.ts`      | Flow B listener: subscribes to `error-report-auto-sent`, renders the auto-send toast                                                                                                                                                                                         |
| `AutoSendToastContent.svelte`    | Flow B toast UI — title, reference ID, "View" + "Change settings" links, 10 s timeout                                                                                                                                                                                        |
| `breadcrumbs.ts`                 | Thin `recordBreadcrumb(kind, message, ctx?)` wrapper around the `record_breadcrumb` IPC. Fire-and-forget; failures swallowed. Wire from FE event handlers to add triage context to error report bundles. See `error_reporter/CLAUDE.md` § Breadcrumbs for backend semantics. |

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

## Flow B — auto-send toast

When the `updates.errorReports` setting is on, the Rust auto-dispatcher fires `error-report-auto-sent` (payload:
server-issued report ID) after a successful upload. `auto-send-toast.svelte.ts` listens for that event from the main
window layout's `onMount` and shows a toast via `addToast(AutoSendToastContent, ...)`:

- **Title**: "Error report sent"
- **Body**: Reference ID badge.
- **Actions**: "View" reuses the Flow A preview dialog so the user can inspect what was shipped (the dialog re-builds
  the bundle locally — deterministic modulo the timestamp). "Change settings" opens the Settings window so they can flip
  the opt-in flag.
- **Auto-dismiss after 10 s**, longer than the default 4 s — auto-sent reports are surprising, so the user needs more
  time to notice and act.

The listener is initialized in `(main)/+layout.svelte` next to the Flow A dialog mount, and torn down in the matching
`onDestroy`. Idempotent — repeated `init` calls are no-ops.

## Dev affordance

In dev (`import.meta.env.DEV`), the dialog shows an extra "Save bundle to disk (debug)" button that calls
`saveErrorReportToDisk` and toasts the resulting path. The Tauri command is gated on `cfg!(debug_assertions)`; in
production it isn't registered, so calling the wrapper would return an error.

## Gotchas

- The dialog calls `prepareErrorReportPreview` exactly once when it mounts. The user note doesn't influence log content;
  it only lands in the manifest. The displayed manifest is rebuilt locally with the live note value, and
  `sendErrorReport` (and `saveErrorReportToDisk`) ship the current note when invoked. Rebuilding the multi-MB zip per
  keystroke would have been wasteful for no behavioural gain.
- The note counter and Send-disabling use `Array.from(userNote).length` so they match the Rust validator's
  `.chars().count()` (Unicode code points). `userNote.length` (UTF-16 code units) would let emoji-heavy notes bypass the
  cap on the frontend and then fail server-side.
- `errorReportFlow.initialNote` is captured when the dialog mounts via
  `let userNote = $state(errorReportFlow.initialNote)`. Subsequent edits to the textarea are local to the component —
  closing and reopening the dialog reads from the store again.
- `<script module>` blocks in Svelte 5 _do_ support `$state`. The compiler warns if you put module-level state in a
  regular `<script>` block by mistake.
