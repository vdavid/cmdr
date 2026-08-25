# Attach my email (shared opt-in)

The "Attach my email … so we can reply" control the crash-report, error-report, and feedback dialogs all render, so a
report can carry a reply address. Shapes, copy keys, decisions, and testing: `DETAILS.md`.

## Module map

- `attach-email.svelte.ts`: `createAttachEmail()` builds the `AttachEmail` state (the tick, the typed address, its
  validity, `persist()`). Call it during a dialog's init.
- `AttachEmailCheckbox.svelte`: the checkbox, plus the email field a tick reveals when nothing is on file.

## Must-knows

- **The control ALWAYS renders.** With an `analytics.email` on file the label names the address; without one, ticking
  reveals a field to type one in. Don't reintroduce a hide-when-empty branch: these dialogs are the only place a user
  who skipped the onboarding beta step can leave a reply address.
- **Never pre-ticked off an address.** The tick comes from `updates.attachEmailToReports` (registry default `false`) and
  from nothing else. The field appearing only after an explicit tick is what keeps the ask honest.
- **Call `persist()` only on a SUCCESSFUL send**, which is why every dialog calls it after its `await`, never before. It
  writes the sticky tick plus, when the user typed one, the new `analytics.email`; run it earlier and a half-typed
  address quietly becomes their reply channel.
- **Validation is deliberately loose**, mirroring the server's `emailShapePattern`. `blocksSend` is what a dialog
  disables Send on; an empty field never blocks, it just sends without an address.
- ❌ **Only user-initiated sends may carry an email.** The error reporter's Flow B (auto-send) ships `email: None`
  structurally, backend-side; see `src-tauri/src/error_reporter/CLAUDE.md`.
