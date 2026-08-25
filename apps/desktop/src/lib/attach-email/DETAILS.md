# Attach my email: details

Depth behind `CLAUDE.md`. The two shapes the control takes, the settings it touches, why validation and persistence sit
where they do, and what the tests pin.

## The two shapes

`createAttachEmail()` reads `analytics.email` once at init (a plain cache read, so a mid-dialog settings change can't
move the control under the user), trims it, and that decides the shape:

- **An address on file** (`hasContactEmail`): the label is `common.attachEmail`, which names the address in parentheses.
  Ticking attaches it. No field, because the question is already answered.
- **Nothing on file**: the label is `common.attachEmailPrompt` ("Attach my email so we can reply"). Ticking reveals a
  `TextInput` (`type="email"`, named by `common.attachEmailInputLabel`, placeholder `common.attachEmailPlaceholder`)
  indented under the label, and the box focuses it so a keyboard user can answer immediately.

Only the second shape can write `analytics.email`. An address already on file is never edited from a report dialog; that
belongs to Settings > Updates & privacy and the onboarding beta step.

## Validation

`EMAIL_SHAPE` is `/^[^\s@]+@[^\s@]+$/`, character for character the api-server's `emailShapePattern`
(`apps/api-server/src/telemetry/feedback.ts`). A reply address only has to be routable, and Rust passes it through
untouched (`src-tauri/src/feedback.rs` `build_payload`), so the frontend is the last gate and it must not be stricter
than the server. Odd-but-real addresses (`a@b`, a quoted local part, non-ASCII) pass on purpose.

Three states, and what each does to the send:

- **Empty field, ticked**: nothing attaches, the send goes through. A user who ticked and changed their mind isn't a
  user who made a mistake, so there's nothing to correct.
- **Non-empty and it fits the shape**: the trimmed value attaches.
- **Non-empty and it can't be an address**: `blocksSend` goes true, the dialog disables Send, and
  `common.attachEmailInvalid` renders under the field with `aria-invalid` + `aria-describedby` pointing at it. Asking
  for a reply to nowhere is the one case worth stopping for.

The message tracks the field live rather than waiting for a blur. A disabled Send button with no visible reason reads as
a broken app, and `blocksSend` can't wait for a blur without letting a click through, so the two are kept in step.

## Persistence

`persist()` writes `updates.attachEmailToReports` (the sticky tick, shared by all three dialogs) and, when the user
typed an address that actually rode along, `analytics.email`. Every dialog calls it AFTER its send resolves
successfully:

- `FeedbackDialog.svelte`: inside the `result.kind === 'sent'` branch.
- `ErrorReportDialog.svelte`: right after `await sendErrorReport(…)` returns.
- `CrashReportDialog.svelte`: right after `await sendCrashReport(…)` returns (its `catch` swallows send failures).

Onboarding's email field persists per keystroke, which is right there: the user is on a page whose whole purpose is
giving an address. Here it would be wrong. A dialog abandoned mid-typing would leave `alex@ex` as the address every
future report replies to, and nothing in the UI would say so.

Each dialog also folds `blocksSend` into one `canSend` derived that gates the Send button, the keyboard send combo, and
`handleSend`'s own guard, so no path can slip past the block.

## Copy

All five strings live in `common.json` (`common.attachEmail`, `…Prompt`, `…InputLabel`, `…Placeholder`, `…Invalid`),
because three dialogs render them. `attach-email-i18n-parity.test.ts` freezes the English once, here rather than per
dialog. The prompt is the parenthetical-free twin of `common.attachEmail`; keep them parallel when either is edited.

## Testing

- `attach-email.test.ts`: the state machine against a mocked settings store, including the loose-validation corpus and
  every persistence rule (typing writes nothing; an invalid address files nothing; an address on file is left alone).
- `AttachEmailCheckbox.a11y.test.ts`: both shapes under axe, the field's accessible name, and the `aria-invalid` /
  `aria-describedby` wiring. Its `render()` awaits a tick before any click: Ark's checkbox machine starts on the mount's
  effects, and a synthetic click before that toggles the DOM input without ever reaching the binding.
- `FeedbackDialog.a11y.test.ts` carries the end-to-end collect flow (type, tick, type an address, send, assert the
  `analytics.email` write and that a failed send writes nothing); `error-reporter.a11y.test.ts` covers the reuse shape
  and the blocked send.
