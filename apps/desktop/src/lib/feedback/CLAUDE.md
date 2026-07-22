# Feedback (frontend)

The open-beta "Send feedback" dialog: free-text feedback sent to the api-server (`POST /feedback`), which stores it in
D1 and pings Discord. No log bundle rides along (that's the error reporter's job). Entry points, mount location, and
behavior detail: `DETAILS.md`.

## Module map

- `feedback-flow.svelte.ts`: reactive `feedbackFlow` store + `openFeedbackDialog()` / `closeFeedbackDialog()`.
- `FeedbackDialog.svelte`: the dialog (textarea, attach-email checkbox, GitHub / book-a-call links, send / cancel).

## Must-knows

- **Text caps must stay byte-agreed across three layers.** Soft warn at 50 000, hard cap at 100 000, counted in Unicode
  code points: frontend `Array.from(text).length`, Rust `.chars().count()`, server `Array.from(text).length`. They must
  agree, or emoji-heavy text bypasses the cap on one layer.
- **The attach-email checkbox is shared and sticky.** It shows only when `analytics.email` is set, initializes from
  `updates.attachEmailToReports` (never pre-ticked on first use), and writes back on send, so the choice is shared with
  the error and crash report dialogs. Don't give feedback its own setting key.
- **Branch on the typed `SendFeedbackResult.kind`** (`sent` / `invalid` / `softFailure`), never on message substrings
  (`no-string-matching` rule). `sendFeedback` in `tauri-commands/feedback.ts` returns it.
- **External links go through `openExternalUrl`** (opener plugin), never a raw `<a>` navigation, which Tauri blocks.
  Link URLs live in `$lib/beta-links.ts`, shared with other open-beta surfaces.

Backend: `src-tauri/src/feedback.rs` (validation + payload + send), `commands/feedback.rs` (thin IPC). Server:
`apps/api-server/src/feedback.ts`.
