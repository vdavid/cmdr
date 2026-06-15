# Feedback (frontend)

The open-beta "Send feedback" dialog: a zero-friction channel for testers to send free-text feedback from inside the
app. The text lands on the api-server (`POST /feedback`), which stores it in D1 and pings the maintainer's Discord. No
log bundle rides along; that's the error reporter's job.

## Files

- **`feedback-flow.svelte.ts`**: Reactive store + `openFeedbackDialog()` entry point (mirrors `error-report-flow`)
- **`FeedbackDialog.svelte`**: The dialog: textarea, attach-email checkbox, GitHub / book-a-call links, send/cancel
- **`FeedbackDialog.a11y.test.ts`**: Tier 3 a11y + behavior tests (send paths, caps, email gating, link routing)

## Entry points

- **Help menu → "Send feedback…"** (both macOS and Linux), routed through the `feedback.send` command.
- **Command palette → "Send feedback"**.

Both dispatch `feedback.send`, whose handler (`app-dialog-handlers.ts`) calls `openFeedbackDialog()`. The dialog mounts
in `(main)/+layout.svelte` next to `ErrorReportDialog` (same pattern).

## Conventions shared with the error reporter

- **Text caps**: soft counter at 50 000, hard cap at 100 000, counted in Unicode code points (`Array.from(text).length`)
  so the frontend, the Rust validator (`.chars().count()`), and the server (`Array.from(text).length`) all agree.
  Emoji-heavy text must not bypass the cap.
- **Attach-email checkbox**: shows only when `analytics.email` is set; initialized from `updates.attachEmailToReports`
  (never pre-ticked on first use) and written back on send, so the choice is sticky and shared with the error and crash
  report dialogs.
- **Typed results, no string matching**: `sendFeedback` (in `tauri-commands/feedback.ts`) returns `SendFeedbackResult`
  (`sent` / `invalid` / `softFailure`); the dialog branches on `kind`.

## Behavior notes

- On success: warm toast, text cleared, dialog closes. On failure: inline retry message, dialog stays open, the user's
  text survives.
- The "browse and vote on GitHub" / "book a call" links live in `$lib/beta-links.ts` (shared with other open-beta
  surfaces) and route through `openExternalUrl` (opener plugin; raw `<a>` navigation is blocked in Tauri).

Backend counterpart: `apps/desktop/src-tauri/src/feedback.rs` (validation + payload + send) and `commands/feedback.rs`
(thin IPC wrapper). Server: `apps/api-server/src/feedback.ts`.

Full details: [DETAILS.md](DETAILS.md).
