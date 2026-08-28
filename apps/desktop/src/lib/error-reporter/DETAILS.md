# Error reporter (frontend) — details

Read this before any non-trivial work here: editing, planning, reorganizing, or advising. `CLAUDE.md` holds the
must-knows; this is the depth.

## The dialog's two modes

`ErrorReportDialog.svelte` reads `errorReportFlow.mode` ONCE at init (like `initialNote`) and branches on a plain
`isAmend` const, so a close mid-flight can't swap the mode under an in-flight submit.

What differs, compose → amend:

- Opened by: `openErrorReportDialog(initialNote?)` → `openErrorReportDialogForAutoSentReport()`.
- Preview source: `prepareErrorReportPreview()`, which builds a bundle → `getAutoSentReportPreview()`, which reads the
  backend stash, so the manifest and sample lines are the ones that actually shipped.
- Submit: `sendErrorReport(note, email, preview.id)` → `amendErrorReport(note, email)` (no id: there's only ever one
  stashed report).
- Submit enabled when: the note is under the cap and the email is valid → that, PLUS a note or an email to carry, since
  the server turns down an amendment with neither.
- "Save bundle to disk (debug)": shown in dev → hidden, there's no local bundle.
- Post-send toast: `kind: 'sent'` → `kind: 'amended'`.
- Copy: everything else, the reference-ID badge and its Copy button included, comes from the same keys or the same
  shape; only the `errorReporter.amend.*` family differs.

The mode lives in the store rather than in a second positional argument to `openErrorReportDialog`, which has ten call
sites and would be exactly the confusable-parameter shape `cmdr/no-confusable-callback-params` exists to discourage.
`closeErrorReportDialog()` resets it to `compose`, so a leftover `amend` can't leak into the next Help-menu open.

### Why amend exists

One incident used to produce three ids: Flow B auto-sent `ERR-J9BKB` and said so in a toast; the toast's button opened
the compose dialog, which built and displayed a THIRD bundle (`ERR-ZVWQ2`); pressing Send uploaded a SECOND report
(`ERR-AYVM4`). The id the user copied was the one that never existed. Hence the two invariants the tests guard: the
amend path never calls `sendErrorReport`, and the id on screen is the id that shipped.

### The dead end, and why it's a dead end

`getAutoSentReportPreview()` returns `null` when nothing was auto-sent this run (the stash dies with the process), and
`canAmend: false` when the server handed back no amend key. Either one, plus a throwing lookup, lands on
`errorReporter.amend.unavailable`: a sentence pointing at the Help menu, and a Close button. ❌ No fallback send, no
retry loop. Branch on `canAmend`, never on a message (`cmdr/no-error-string-match`).

An amend can land more than once for the same report; amendments accumulate server-side and `canAmend` stays true. The
button is therefore disabled DURING the call, not after it.

## Flow B: auto-send toast

When `updates.errorReports` is on, the Rust auto-dispatcher fires `error-report-auto-sent` (payload: server-issued
report ID) after a successful upload. `auto-send-toast.svelte.ts`, initialized from the main window layout's `onMount`,
listens and shows `addToast(AutoSendToastContent, ...)`:

- **Title**: "Error report sent". **Body**: reference ID badge.
- **Actions**: "View or add notes to the report" opens the dialog in amend mode; "Change settings" opens the Settings
  window to flip the opt-in.
- **Auto-dismiss after 10 s** (longer than the default 4 s): auto-sent reports are surprising, so the user needs more
  time to notice and act.

The listener is initialized in `(main)/+layout.svelte` next to the dialog mount and torn down in the matching
`onDestroy`. Idempotent: repeated `init` calls are no-ops.

## ID-bridging pattern

`error-report-toast-state.svelte.ts` holds `{ id, kind }` in a module-level `$state` with `setLastSentReport(...)` /
`getLastSentReportId()` / `getLastSentReportKind()`. The dialog sets it right before `addToast(component, ...)` so the
toast renders both without the toast system forwarding props. One setter taking an object keeps the pair from drifting:
an amended report showing the "Error report sent" sentence would be the same class of lie the amend mode fixes. The
state lives in a `.svelte.ts` module rather than the toast's `<script module>` so its exports are typed across imports
(a `.svelte` module export is seen as `any`). Same pattern in `bundle-saved-toast-state`, `auto-send-toast-state`, and
mtp's `mtp-connected-toast-state`.

## Note-capture timing and gotchas

- The preview loads exactly once per mount, in an `$effect` whose synchronous phase reads NOTHING reactive. The email
  used to be an argument to `prepareErrorReportPreview`, which made `attachEmail.emailToAttach` (a getter over `$state`)
  a tracked dependency: ticking the box or typing one character re-ran the megabyte-scale bundle build and minted a
  fresh report id. `displayedManifest` overlays the live note and email onto the cached manifest instead, and the submit
  ships the current values, so the preview stays accurate with one build.
- `errorReportFlow.initialNote` is captured on mount via `let userNote = $state(errorReportFlow.initialNote)`. Later
  textarea edits are local to the component; closing and reopening reads from the store again.
- Note caps: hard limit 100 000 code points (submit disabled, FE and Rust both enforce), soft counter at 50 000, and the
  server caps the whole payload at 10 MB.
- `<script module>` blocks in Svelte 5 do support `$state`. The compiler warns if you put module-level state in a
  regular `<script>` block by mistake.

## The gallery row

`dialog-gallery` has an `amend` state for this dialog. It seeds the real store and hits the real backend, so on a
machine that hasn't auto-sent anything this run it honestly renders the dead end rather than a staged preview; the row's
`note` says so and how to stage the real thing.
