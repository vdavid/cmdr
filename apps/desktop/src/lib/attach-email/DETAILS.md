# Attach my email: details

Depth behind `CLAUDE.md`. The two shapes the control takes, how it follows the address live, the settings it touches,
why validation and persistence sit where they do, and what the tests pin.

## The two shapes

`createAttachEmail()` holds `analytics.email`, trimmed, and that decides the shape:

- **An address on file** (`hasContactEmail`): the label is `common.attachEmail`, which names the address in parentheses
  and offers an inline `<change>` link. Ticking attaches it. No field, because the question is already answered.
- **Nothing on file**: the label is `common.attachEmailPrompt` ("Attach my email address so you can follow up"). Ticking
  reveals a `TextInput` (`type="email"`, named by `common.attachEmailInputLabel`, placeholder
  `common.attachEmailPlaceholder`) indented under the label, and the box focuses it so a keyboard user can answer
  immediately. A field revealed by a live change takes no focus: the user is in the Settings window at that moment.

Only the second shape can write `analytics.email`. An address already on file is never edited from a report dialog; the
`<change>` link hands that job to Settings > Updates & privacy, which also owns it for the onboarding beta step.

## Following the address live

`createAttachEmail()` subscribes with `onSpecificSettingChange('analytics.email', …)` from an `$effect`, whose cleanup
is the unsubscribe. That puts the listener's life in the hands of the component that built the state, so a dialog that
closes (including the second in a crash-over-error stack) takes its listener with it and no caller can forget a
`dispose()`. The one-shot read it replaced can't work any more: the label's "change" link opens Settings as a WINDOW, so
the user edits the address with the dialog still on screen behind it.

**Decision: a live change keeps both the tick and the typed draft.** The tick means "I want a reply", which the address
moving doesn't falsify, and it can't come to mean a different address behind the user's back, because the control always
SHOWS what will ride along: the label names the address on file, the field shows what was typed, and `emailToAttach`
reads whichever shape is current. So the two directions land like this:

- **Empty → set**: the field disappears, the label names the new address, and that address is what sends. A draft left
  in the field is kept, not sent, and comes back if the address is cleared again.
- **Set → empty**: the collect field appears under a box that is still ticked, and `emailToAttach` goes `undefined`.
  Nothing attaches until the user types something. This is the case worth being careful about, and it's why the tick is
  safe to keep: an empty field attaches nothing, and a non-empty one is visible in front of the user.

`updates.attachEmailToReports` stays a one-shot read. It seeds the tick; after that the tick belongs to the user in
front of the dialog, and a sticky-choice write from another dialog moving it would be exactly the surprise the live
address avoids.

## The "change" link

`common.attachEmail` carries a `<change>` tag, so the label renders through `<Trans>` (`$lib/intl`) rather than
`tString()`, with `{emailAddress}` as a plain param. The tag and the param are named apart on purpose:
`i18n-tag-param-collision` is an error precisely because `<Trans>` merges tag snippets into the interpolation params.

The link is a `LinkButton` that calls
`openSettingsWindow('attach-email', ['Updates & privacy'], settingAnchorId('analytics.email'))`. Two details in there:

- **`settings-window` is imported lazily**, the way `ShortcutChip` does it: this control renders in the crash-report
  dialog, and a static import would pull the settings window's Tauri surface into all three dialogs at eval time.
- **A click on the link must not tick the box.** Ark's `Checkbox.Root` IS the `<label>` the link renders inside, and by
  the time a handler on the link itself runs the box has already toggled. The guard therefore sits on the wrapper as an
  `onclickcapture` that cancels the click on the way down. HTML does say a label ignores clicks on interactive content
  inside it, but a canceled click can't activate a label anywhere, which is the version worth relying on.

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
dialog: the raw source (tag and placeholder included) plus the sentence as `<Trans>` composes it. The prompt is the
link-free, parenthetical-free twin of `common.attachEmail`; keep them parallel when either is edited.

## Testing

- `attach-email.svelte.test.ts`: the state machine against a mocked settings store, including the loose-validation
  corpus, every persistence rule (typing writes nothing; an invalid address files nothing; an address on file is left
  alone), and the live switch in both directions. It builds the state inside an `$effect.root` because the `$effect`
  needs an owner; closing that root is how it checks the listener really goes away.
- `AttachEmailCheckbox.a11y.test.ts`: both shapes under axe, the field's accessible name, the `aria-invalid` /
  `aria-describedby` wiring, the link (named, tab-reachable, deep-linking with the right surface/section/anchor, and
  leaving the tick alone), and the live switch in both directions plus its teardown. It mounts
  `attach-email-fixture.svelte` rather than calling `createAttachEmail()` itself, so the state's `$effect` has a real
  component to belong to. Its `render()` awaits a tick before any click: Ark's checkbox machine starts on the mount's
  effects, and a synthetic click before that toggles the DOM input without ever reaching the binding.
- `FeedbackDialog.a11y.test.ts` carries the end-to-end collect flow (type, tick, type an address, send, assert the
  `analytics.email` write and that a failed send writes nothing); `error-reporter.a11y.test.ts` covers the reuse shape
  and the blocked send.
