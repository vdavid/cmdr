# Error reporter (frontend)

Flow A (user-initiated "Send error report" UI): preview the redacted log bundle, optionally add a note, ship it to the
api server. Flow B (opt-in auto-send on user-visible errors): a listener turns the backend's `error-report-auto-sent`
event into a confirmation toast.

## File map

- `error-report-flow.svelte.ts`: reactive store + `openErrorReportDialog(initialNote?)` entry point (Flow A).
- `ErrorReportDialog.svelte`: preview-and-send dialog (Flow A).
- `ErrorReportToastContent.svelte`: Flow A post-send toast (server-issued ID + Copy).
- `BundleSavedToastContent.svelte`: dev-only "Save bundle to disk" toast (path + Reveal in Finder).
- `auto-send-toast.svelte.ts` + `AutoSendToastContent.svelte`: Flow B listener and toast.
- `breadcrumbs.ts`: fire-and-forget `recordBreadcrumb(kind, message, ctx?)` over the `record_breadcrumb` IPC; wire from
  FE handlers to add triage context. Backend semantics in `error_reporter/CLAUDE.md` § Breadcrumbs.

## Must-knows

- **Two stateless commands, not a stateful prepare-then-send pair.** `prepareErrorReportPreview` renders the preview (no
  network); `sendErrorReport` ships. Don't cache bundle bytes on the Rust side across IPC round-trips: re-building is
  cheap and the preview matches the upload byte-for-byte modulo the timestamp. Backend rationale in
  `apps/desktop/src-tauri/src/error_reporter/CLAUDE.md`.
- **Display the preview ID in the dialog, but show the post-send toast with the server's response ID.** The server may
  regenerate the ID on a HEAD collision, so `sendErrorReport`'s return value is canonical. The dialog calls
  `setLastSentReportId(id)` (exported from `ErrorReportToastContent.svelte`'s `<script module>`) before `addToast`.
- **Char counting uses `Array.from(userNote).length`** (code points) to match the Rust validator's `.chars().count()`.
  `userNote.length` (UTF-16 units) would let emoji-heavy notes pass the FE cap then fail server-side. Hard limit 100 000
  chars (Send disabled, both layers enforce); soft warning at 50 000; server also caps total payload at 10 MB.
- **The reply-to email rides ONLY user-initiated sends.** Both Flow A and the crash dialog show "Attach my email" but
  only when `analytics.email` is set; it threads into the send payload and writes `updates.attachEmailToReports` back
  (sticky). Flow B (auto-send) never attaches it (see `error_reporter/DETAILS.md` § Flow-B-never-email).
- **The dev-only "Save bundle to disk" button** calls `saveErrorReportToDisk`, gated on `import.meta.env.DEV`; the Tauri
  command is `cfg!(debug_assertions)`-only, so calling it in production returns an error.

Entry points: Help menu → "Send error report…" (via `command-dispatch.ts`'s `help.sendErrorReport`), and an inline "Send
error report…" link on plain-text error toasts (`ToastItem.svelte`) that pre-fills the note. Both call
`openErrorReportDialog(initialNote?)`; the dialog mounts in `(main)/+layout.svelte`.

Full details (Flow B toast contents and lifecycle, note-capture timing, `<script module>` `$state` notes):
[DETAILS.md](DETAILS.md).
