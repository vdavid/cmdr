# Feedback (frontend)

The open-beta "Send feedback" dialog: free-text feedback sent to the api-server (`POST /feedback`), which stores it in
D1 and files it on the reports board. An opt-in box sends the past five hours of logs along, as an error report the
server shows on the same card. Entry points, mount location, and behavior detail: `DETAILS.md`.

## Module map

- `feedback-flow.svelte.ts`: reactive `feedbackFlow` store + `openFeedbackDialog()` / `closeFeedbackDialog()`.
- `FeedbackDialog.svelte`: the dialog (textarea, attach-email and include-logs checkboxes, GitHub / book-a-call links,
  send / cancel).

## Must-knows

- **Text caps must stay byte-agreed across three layers.** Soft warn at 50 000, hard cap at 100 000, counted in Unicode
  code points: frontend `Array.from(text).length`, Rust `.chars().count()`, server `Array.from(text).length`. They must
  agree, or emoji-heavy text bypasses the cap on one layer.
- **The attach-email checkbox comes from `#lib/attach-email`**, shared with the error- and crash-report dialogs.
  `attachEmail.blocksSend` is folded into this dialog's `canSend`, and `persist()` runs in the `sent` branch, never
  before the send. Don't hand-roll the control or give feedback its own setting key or label; everything else about it
  lives in `apps/desktop/src/lib/attach-email/CLAUDE.md`.
- **Branch on the typed `SendFeedbackResult.kind`** (`sent` / `sentWithoutLogs` / `invalid` / `softFailure`), never on
  message substrings. `sendFeedback` in `tauri-commands/feedback.ts` returns it. `sentWithoutLogs` closes the dialog
  like `sent`: the feedback is in, so a retry would only send it twice.
- **The include-logs box starts unticked every time and hides under `reportsDisabled`** (the managed policy), since the
  logs travel as an error report. ❌ Don't make it sticky or give it a setting: it's a per-message choice.
- **External links go through `openExternalUrl`** (opener plugin), never a raw `<a>` navigation, which Tauri blocks.
  Link URLs live in `#lib/beta-links.ts`, shared with other open-beta surfaces.

Backend: `src-tauri/src/feedback.rs` (validation + payload + send), `commands/feedback.rs` (thin IPC). Server:
`apps/api-server/src/telemetry/feedback.ts`.
