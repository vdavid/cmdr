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

/**
 * Whether this report is about a crash Cmdr actually went down with.
 *
 * The dialog title and the sent toast both name the artifact, and "crash report" is false
 * for a survived panic and unprovable for a report that carries no fate. They split two
 * ways rather than three, because the two non-crash cases want the identical wording.
 */
function isCrash(report: { appFate?: AppFate | null }): boolean {
  return report.appFate === 'ended'
}

/** Dialog title. `.ended` keeps the specific "Send crash report?"; the rest go neutral. */
export function crashDialogTitleKey(report: { appFate?: AppFate | null }): MessageKey {
  return isCrash(report) ? 'crashReporter.dialog.title.crash' : 'crashReporter.dialog.title.report'
}

/** The after-auto-send toast, split the same way and for the same reason as the title. */
export function crashSentToastKey(report: { appFate?: AppFate | null }): MessageKey {
  return isCrash(report) ? 'crashReporter.sentToast.message.crash' : 'crashReporter.sentToast.message.report'
}
