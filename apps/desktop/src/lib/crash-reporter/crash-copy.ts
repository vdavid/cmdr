/**
 * Picks the crash dialog's opening sentence from what the report actually knows.
 *
 * A crash file records `appFate` (`crash_reporter/survival.rs`) because the app doesn't
 * always die of the panic it reported: since the lock-poison policy, a panic on a
 * background thread leaves the app running, and the user then quits it themselves. One
 * fixed "Cmdr quit unexpectedly last time" was false for those, and vague copy that
 * covered every case would be true but useless. So each fate gets its own true sentence.
 */

import type { AppFate } from '$lib/ipc/bindings'
import type { MessageKey } from '$lib/intl/keys.gen'

/**
 * The body key for one pending crash report.
 *
 * Anything other than a settled fate resolves to the `unknown` sentence, which stays true
 * whether the app died or not. That's the safe direction: the alternative would tell
 * someone their app crashed on the strength of a field that wasn't there.
 */
export function crashDialogBodyKey(report: { appFate?: AppFate | null }): MessageKey {
  switch (report.appFate) {
    case 'ended':
      return 'crashReporter.dialog.body.ended'
    case 'keptRunning':
      return 'crashReporter.dialog.body.keptRunning'
    default:
      return 'crashReporter.dialog.body.unknown'
  }
}
