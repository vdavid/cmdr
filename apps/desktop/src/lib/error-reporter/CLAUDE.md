# Error reporter (frontend)

Flow A (user-initiated "Send error report" UI): preview the redacted log bundle, optionally add a note, ship it to the
api server. Flow B (opt-in auto-send on user-visible errors): a listener turns the backend's `error-report-auto-sent`
event into a confirmation toast, whose button reopens the SAME report for a note.

## File map

- `error-report-flow.svelte.ts`: reactive store + entry points: `openErrorReportDialog({ from, initialNote? })` and
  `openErrorReportFor(incident)` (compose), `openErrorReportDialogForAutoSentReport()` (amend), and
  `hostErrorReportDialog()` (the main window's layout). `mode` lives in the store, not in a second argument.
- `report-incident.ts` + `ReportOffer.svelte`: the typed `ReportIncident` (surface, operation, error kind) that words
  the pre-filled note, and the one action every failure and slowness surface renders for it.
- `ErrorReportDialog.svelte`: one dialog, both modes. `ErrorReportToastContent.svelte`: the post-send toast for both
  outcomes (`kind` picks the sentence).
- `BundleSavedToastContent.svelte`: the "saved to disk" toast (path + Reveal in Finder).
- `auto-send-toast.svelte.ts` + `AutoSendToastContent.svelte`: Flow B listener and toast. Both toasts render
  `SentReportToastBody.svelte` (optional title, sentence + id badge, right-aligned actions).
- `breadcrumbs.ts`: fire-and-forget `recordBreadcrumb(event)` over the generated closed `BreadcrumbEvent` union; its
  command payload narrows to `CommandId`. Privacy boundary: `apps/desktop/src-tauri/src/error_reporter/DETAILS.md`.

## Must-knows

- **One incident, one id.** The preview's id IS the report's id: the dialog passes it back to `sendErrorReport`, so
  badge, Copy button, and post-send toast all name the same report. ❌ Never let a path mint a second one.
- ❌ **The auto-sent toast must never reach the compose path.** Its button opens amend mode, which calls
  `amendErrorReport` (adds to the stashed report, no upload) instead of `sendErrorReport`. Amendments accumulate, so
  disable the submit while the call is in flight, and branch on `canAmend`, ❌ never on an error message.
- **No stash, or `canAmend: false` → an honest dead end**, not a fallback send: the dialog says the report can't take a
  note and offers only Close. Same for a throwing lookup.
- ❌ **Nothing reactive may be read in the preview effect's synchronous phase**, or a keystroke rebuilds a multi-MB
  bundle and re-mints the id under the cursor. `displayedManifest` overlays the live note and email instead.
- **Char counting uses `Array.from(userNote).length`** (code points), matching the Rust validator's `.chars().count()`;
  `userNote.length` would let emoji-heavy notes pass the FE cap then fail server-side. Caps in `DETAILS.md`.
- **The reply-to email rides every USER-INITIATED send, amend included**: typing an address into the amend dialog IS the
  explicit per-report consent the Flow-B-never-email rule is about. `apps/desktop/src/lib/attach-email/CLAUDE.md` owns
  the rules; auto-send itself still ships `email: None` structurally (`error_reporter/DETAILS.md` § Flow-B-never-email).
- **Saving to disk is compose-only**: it writes the zip under the id the send would use, and amend mode has no local
  bundle. It shows in dev, and as the ONLY action when the organization turned reports off (no Send, no attach-email).
  `DETAILS.md` § When the organization turned reports off.
- **Offer a report via `ReportOffer` + a typed `ReportIncident`**, ❌ never a hand-rolled button or a note built from a
  message. A component error toast MUST pass `report`. ❌ Never log at error level to make something reportable (that
  auto-sends). `DETAILS.md` § Report offers.
- **Only the main window renders the dialog** (`(main)/+layout.svelte`, AFTER the page, so it opens over the dialogs
  it's offered from). Other windows ask over `open-error-report`. The one exception is `PageCrashPanel.svelte` (a
  crashed window, shown by `+error.svelte` and the root `WindowBoundary`): that layout is gone, so the panel hosts the
  dialog and its toasts itself.

The two modes, the managed-off dialog, the Flow B toast, and the caps: `DETAILS.md`. Read it before any non-trivial work
here: editing, planning, reorganizing, or advising.
