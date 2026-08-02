/**
 * The "Attach my email" opt-in shared by the crash-report, error-report, and feedback
 * dialogs.
 *
 * All three read the same beta contact email (`analytics.email`) and write back the same
 * sticky `updates.attachEmailToReports` choice, so ticking the box in one carries to the
 * next. Pair this with `AttachEmailCheckbox.svelte`, which renders (and hides) itself off
 * the same state.
 *
 * Only user-initiated sends may carry the email. The error reporter's Flow B (auto-send)
 * never goes near this: it ships `email: None` structurally, backend-side. See
 * `src-tauri/src/error_reporter/CLAUDE.md`.
 */

import { getSetting, setSetting } from '$lib/settings'

export interface AttachEmail {
  /** The contact email on file, trimmed. Empty when the user hasn't set one. */
  readonly contactEmail: string
  /** Whether to offer the checkbox at all. False when no email is on file. */
  readonly available: boolean
  /** The live checkbox state. Bindable. */
  attach: boolean
  /** The email to ship with the report, or `undefined` when it shouldn't ride along. */
  readonly emailToAttach: string | undefined
  /** Write the sticky choice back to settings. Call right before sending. */
  persist(): void
}

/**
 * Build the attach-email state for one dialog instance. Call it during component init;
 * both settings are read once (`getSetting` is a plain cache read, not a reactive
 * source), so a mid-dialog settings change doesn't move the checkbox underneath the user.
 */
export function createAttachEmail(): AttachEmail {
  // Trimmed so a stray-space value doesn't count as "on file".
  const contactEmail = getSetting('analytics.email').trim()
  // Sticky default from the last choice (the Advanced toggle, or a prior report). Never
  // pre-ticked on first use: the registry default is false.
  let attach = $state(getSetting('updates.attachEmailToReports'))

  return {
    get contactEmail() {
      return contactEmail
    },
    get available() {
      return contactEmail.length > 0
    },
    get attach() {
      return attach
    },
    set attach(value: boolean) {
      attach = value
    },
    get emailToAttach() {
      return attach && contactEmail ? contactEmail : undefined
    },
    persist() {
      // Only when an email is on file: without one the checkbox never rendered, so
      // `attach` is whatever the setting already said and writing it back is noise.
      if (contactEmail) {
        setSetting('updates.attachEmailToReports', attach)
      }
    },
  }
}
