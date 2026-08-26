/**
 * The "Attach my email" opt-in shared by the crash-report, error-report, and feedback
 * dialogs.
 *
 * All three read the same beta contact email (`analytics.email`) and write back the same
 * sticky `updates.attachEmailToReports` choice, so ticking the box in one carries to the
 * next. With no address on file the control collects one instead of reusing one, and a
 * successful send files it under `analytics.email` for next time. Pair this with
 * `AttachEmailCheckbox.svelte`, which renders the box and the field off the same state.
 *
 * Only user-initiated sends may carry the email. The error reporter's Flow B (auto-send)
 * never goes near this: it ships `email: None` structurally, backend-side. See
 * `src-tauri/src/error_reporter/CLAUDE.md`.
 */

import { getSetting, onSpecificSettingChange, setSetting } from '$lib/settings'

/**
 * A reply address only has to be routable, so this is the loosest shape that can't be a
 * typo for something else: one `@`, something on each side, no whitespace. It mirrors
 * `emailShapePattern` in `apps/api-server/src/telemetry/feedback.ts` character for
 * character, so the frontend never refuses an address the server would have taken.
 */
const EMAIL_SHAPE = /^[^\s@]+@[^\s@]+$/

export interface AttachEmail {
  /** The contact email on file, trimmed and live. Empty when the user hasn't set one. */
  readonly contactEmail: string
  /** Whether an address is already on file, so the control reuses it instead of asking. */
  readonly hasContactEmail: boolean
  /** The live checkbox state. Bindable. */
  attach: boolean
  /** What the user typed into the revealed field. Bindable; unused when one is on file. */
  typedEmail: string
  /** Whether the typed address is non-empty and can't be an address. Drives the message. */
  readonly typedEmailInvalid: boolean
  /** Whether the dialog must refuse to send: the user asked for a reply to nowhere. */
  readonly blocksSend: boolean
  /** The email to ship with the report, or `undefined` when it shouldn't ride along. */
  readonly emailToAttach: string | undefined
  /** Write the sticky choice (and any newly typed address) back. Call on a SUCCESSFUL send. */
  persist(): void
}

/**
 * Build the attach-email state for one dialog instance. Call it during component init.
 *
 * `analytics.email` is followed LIVE. The label's "change" link opens Settings, which is a
 * WINDOW, not a modal, so the dialog stays up while the user edits the address behind it;
 * a one-shot read would leave the control naming an address that no longer exists. The
 * `$effect` puts teardown where the state was created, so the component that owns the
 * dialog owns the listener too and no caller can forget a `dispose()`.
 *
 * A live change keeps both the tick and the typed draft. The tick means "I want a reply",
 * which the address moving doesn't falsify, and it can't quietly come to mean a different
 * address: the control always SHOWS what will ride along (the label names the address on
 * file, the field shows what was typed) and `emailToAttach` reads the CURRENT shape. So
 * clearing the address to empty lands on a ticked box over a revealed empty field, which
 * attaches nothing and asks for an address instead of assuming one.
 *
 * `updates.attachEmailToReports` stays a one-shot read: it seeds the tick, and from then
 * on the tick belongs to the user in front of the dialog.
 */
export function createAttachEmail(): AttachEmail {
  // Trimmed so a stray-space value doesn't count as "on file".
  let contactEmail = $state(getSetting('analytics.email').trim())
  // Sticky default from the last choice (the Advanced toggle, or a prior report). Never
  // pre-ticked on first use: the registry default is false.
  let attach = $state(getSetting('updates.attachEmailToReports'))
  let typedEmail = $state('')

  // Returning the unsubscribe as the effect's cleanup: the listener lives exactly as long
  // as the component that called this, including the second dialog in a two-dialog stack.
  $effect(() =>
    onSpecificSettingChange('analytics.email', (value) => {
      contactEmail = value.trim()
    }),
  )

  // Plain functions, not `$derived`: this state is built during a component's init and
  // read from that component's template, so the deriveds would be created unowned here
  // and stop propagating (a stale `$derived` left the revealed field unrendered). The
  // computations are a trim and a regex test, so recomputing per read costs nothing.

  /** Whether an address is on file right now, which is what picks the control's shape. */
  const hasContactEmail = () => contactEmail.length > 0
  /** The typed address as it would be sent, or `''` when the user left the field alone. */
  const trimmedTyped = () => typedEmail.trim()
  /** Whether the typed field is the one deciding the address (nothing on file, box ticked). */
  const collecting = () => attach && !hasContactEmail()
  /** Text that can't be an address, so the user asked for a reply to nowhere. */
  const typedEmailInvalid = () => collecting() && trimmedTyped().length > 0 && !EMAIL_SHAPE.test(trimmedTyped())
  const emailToAttach = () => {
    if (!attach) return undefined
    if (hasContactEmail()) return contactEmail
    const typed = trimmedTyped()
    return typed.length > 0 && EMAIL_SHAPE.test(typed) ? typed : undefined
  }

  return {
    get contactEmail() {
      return contactEmail
    },
    get hasContactEmail() {
      return hasContactEmail()
    },
    get attach() {
      return attach
    },
    set attach(value: boolean) {
      attach = value
    },
    get typedEmail() {
      return typedEmail
    },
    set typedEmail(value: string) {
      typedEmail = value
    },
    get typedEmailInvalid() {
      return typedEmailInvalid()
    },
    get blocksSend() {
      // An empty field is a change of mind, not a mistake: the report goes without an
      // address. Only text that can't be one is worth stopping the send for.
      return typedEmailInvalid()
    },
    get emailToAttach() {
      return emailToAttach()
    },
    persist() {
      setSetting('updates.attachEmailToReports', attach)
      // Only an address that actually rode along on a successful send earns a place in
      // settings; a half-typed one would quietly become the user's reply channel.
      const sent = emailToAttach()
      if (collecting() && sent) {
        setSetting('analytics.email', sent)
      }
    },
  }
}
