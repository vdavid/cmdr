# Feedback (frontend) details

Entry points, mounting, and behavior. The must-knows are in `CLAUDE.md`.

## Entry points

- **Help menu, "Send feedback…"** (macOS and Linux), routed through the `feedback.send` command.
- **Command palette, "Send feedback"**.

Both dispatch `feedback.send`, whose handler (`app-dialog-handlers.ts`) calls `openFeedbackDialog()`. The dialog mounts
in `(main)/+layout.svelte` next to `ErrorReportDialog` (same pattern), which keeps focus / Escape handling consistent.

## Behavior

- On success: warm toast, text cleared, dialog closes.
- On failure: inline retry message, dialog stays open, the user's text survives.

## Shared with the error reporter

The text caps and the typed-result pattern mirror the error reporter (see its `error-report-flow`); the attach-email
control is literally the same one, documented in `apps/desktop/src/lib/attach-email/DETAILS.md`.
`feedback-flow.svelte.ts` is the feedback analog of `error-report-flow`.
