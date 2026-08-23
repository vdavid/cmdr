# Crash reporter (frontend)

The next-launch "here's what happened last time" surface: a dialog when the user hasn't opted in (or the app looks stuck
in a crash loop), a toast when they have. Capture, sanitizing, and upload are all backend
(`src-tauri/src/crash_reporter/`); this side only decides which surface to show, which sentence is true of the report,
and collects the user's choices.

## File map

- `CrashReportDialog.svelte`: the decide-and-send dialog (report ID, expandable JSON, "Always send", attach-email,
  Dismiss / Send report).
- `CrashReportToastContent.svelte`: the after-auto-send toast (one line + "Change in Settings > Updates").
- `crash-copy.ts`: maps the report's `appFate` to the body key whose sentence is true of it.
- `crash-reporter-i18n-parity.test.ts`: freezes the en copy for both. The `.a11y.test.ts` pair covers roles and labels.

The flow lives in `routes/(main)/+layout.svelte` (`checkForPendingCrashReport`), not here: it calls
`checkPendingCrashReport` after settings load, then auto-sends + toasts, or mounts the dialog.

## Must-knows

- **A crash loop overrides the opt-in.** Auto-send needs `updates.crashReports` AND `!report.possibleCrashLoop`;
  otherwise the dialog shows. A crashing app must never silently fire a report per launch. Don't simplify that condition
  to the setting alone.
- **The dialog owns the send, the layout owns the auto-send.** Both call `sendCrashReport`, and only the dialog path can
  attach an email or flip `updates.crashReports` on. Adding a send path means deciding both again.
- **Attach-email comes from `$lib/attach-email`** (`createAttachEmail()` + `<AttachEmailCheckbox>`), shared with the
  error-report and feedback dialogs; `persist()` writes the sticky `updates.attachEmailToReports` back on send. Don't
  hand-roll the checkbox or add a crash-specific copy of the label.
- **Dismiss must reach the backend.** `dismissCrashReport` deletes the crash file; closing the dialog any other way
  (Escape, the × button) routes through `handleDismiss` for exactly that reason. Skip it and the same report re-offers
  itself on every launch.
- **Not every crash report describes a crash the app died of**, so the body sentence comes from `crashDialogBodyKey`,
  never a fixed key. Anything unsettled (`unconfirmed`, a missing `appFate` from an older build) falls to the `unknown`
  sentence, which is true either way. Don't flip that default: it would tell a user their app crashed on the strength of
  a field that wasn't there. The fate itself is decided backend-side; `src-tauri/src/crash_reporter/DETAILS.md`.
- **The report JSON is shown verbatim and is safe to show.** The backend already redacted and capped it before it
  reached disk. Don't add fields to the displayed payload here or re-sanitize it: `src-tauri/src/crash_reporter/` is the
  single place that decides what a crash report contains.

Flows, the dialog's states, and the dialog-gallery fixture: `DETAILS.md`.
