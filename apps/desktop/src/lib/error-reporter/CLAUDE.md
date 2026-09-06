# Error reporter (frontend)

Flow A (user-initiated "Send error report" UI): preview the redacted log bundle, optionally add a note, ship it to the
api server. Flow B (opt-in auto-send on user-visible errors): a listener turns the backend's `error-report-auto-sent`
event into a confirmation toast, whose button reopens the SAME report for a note.

## File map

- `error-report-flow.svelte.ts`: reactive store + two entry points, `openErrorReportDialog(initialNote?)` (compose) and
  `openErrorReportDialogForAutoSentReport()` (amend). `mode` lives in the store, not in a second argument.
- `ErrorReportDialog.svelte`: one dialog, both modes. `ErrorReportToastContent.svelte`: the post-send toast for both
  outcomes (`kind` picks the sentence).
- `BundleSavedToastContent.svelte`: dev-only "Save bundle to disk" toast (path + Reveal in Finder).
- `auto-send-toast.svelte.ts` + `AutoSendToastContent.svelte`: Flow B listener and toast. Both toasts render
  `SentReportToastBody.svelte` (optional title, sentence + id badge, right-aligned actions).
- `breadcrumbs.ts`: fire-and-forget `recordBreadcrumb(kind, message, ctx?)`; wire from FE handlers to add triage
  context. Backend semantics in `error_reporter/CLAUDE.md` § Breadcrumbs.

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
- **"Save bundle to disk" is dev-only AND compose-only**: it writes the zip under the id the send would use, and amend
  mode hides it (no local bundle exists there).

Compose entry points: the Help menu's "Send error report…" (`command-dispatch.ts`'s `help.sendErrorReport`) and the
inline link on plain-text error toasts (`ToastItem.svelte`), which pre-fills the note. The dialog mounts in
`(main)/+layout.svelte`.

Full details (the two modes side by side, Flow B toast lifecycle, note-capture timing, the caps, `<script module>`
`$state` notes): `DETAILS.md`.
