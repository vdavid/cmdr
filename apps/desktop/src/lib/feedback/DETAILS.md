# Feedback (frontend) details

Entry points, mounting, and behavior. The must-knows are in `CLAUDE.md`.

## Entry points

- **Help menu, "Send feedback…"** (macOS and Linux), routed through the `feedback.send` command.
- **Command palette, "Send feedback"**.

Both dispatch `feedback.send`, whose handler (`app-dialog-handlers.ts`) calls `openFeedbackDialog()`. The dialog mounts
in `(main)/+layout.svelte` next to `ErrorReportDialog` (same pattern), which keeps focus / Escape handling consistent.

## Behavior

- On success: warm toast, text cleared, dialog closes.
- On success without the logs the user asked for: the same thanks plus a line saying the logs didn't come along, and the
  dialog closes all the same (the feedback is already in).
- On failure: inline retry message, dialog stays open, the user's text survives.

## Including logs

The "Include the past five hours of logs (redacted)" box passes `includeLogs` to `send_feedback`. The backend
(`src-tauri/src/feedback.rs`) mints an `ERR-XXXXX`, sends the feedback naming it, and only then uploads the error
reporter's own Flow A bundle under that id, so a failed send uploads nothing and the dialog's retry is clean. There's no
preview of these logs (the error report dialog has one): the box is the consent, as with the crash toast's "Attach
logs". The server files the two as one card and one digest entry (`apps/api-server/DETAILS.md` § The reports repo).

## Shared with the error reporter

The text caps and the typed-result pattern mirror the error reporter (see its `error-report-flow`); the attach-email
control is literally the same one, documented in `apps/desktop/src/lib/attach-email/DETAILS.md`.
`feedback-flow.svelte.ts` is the feedback analog of `error-report-flow`.
