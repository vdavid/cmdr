/**
 * The three ways out of the Finder extension offer, and what each one records.
 * Separate from `finder-extension-nudge.ts` so the toast body can reach the answers
 * without importing the module that mounts it (`import-cycles`). Mirrors
 * `reveal-nudge-answer.ts`.
 */

import { addToast, dismissToast } from '#lib/ui/toast/index.js'
import { setFinderExtensionEnabled, trackEvent } from '#lib/tauri-commands/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { getAppLogger } from '#lib/logging/logger.js'

const log = getAppLogger('reveal')

/** How the offer ended. `dismissed` is the toast frame's ×, ❌ never an active refusal. */
export type FinderExtensionNudgeAnswer = 'yes' | 'no' | 'dismissed'

/** Records how the offer ended. The one place `finder_extension_answered` is sent. */
export function recordFinderExtensionNudgeAnswer(answer: FinderExtensionNudgeAnswer): void {
  void trackEvent('finder_extension_answered', { answer })
}

/** "No, thanks": retire the toast and never ask again. */
export function declineFinderExtensionNudge(toastId: string): void {
  dismissToast(toastId)
  recordFinderExtensionNudgeAnswer('no')
}

/**
 * "Yes, add it": switch the extension on, then report what macOS was left with.
 *
 * ❗ The answer records the PRESS, ❌ never the outcome, and the toast words the state
 * that came back, ❌ never the one the click asked for.
 */
export async function acceptFinderExtensionNudge(toastId: string): Promise<void> {
  dismissToast(toastId)
  recordFinderExtensionNudgeAnswer('yes')

  const status = await setFinderExtensionEnabled(true)
  if (status?.enabled === true) {
    addToast(tString('main.finderExtensionNudge.turnedOn'), { level: 'success' })
    return
  }

  // `unavailable` is no answer at all: the command didn't exist or didn't reply.
  const reason = status === null ? 'unavailable' : (status.blockedBy ?? 'stayedOff')
  log.warn('The Finder extension did not switch on: {reason}', { reason })
  void trackEvent('finder_extension_not_enabled', { reason })
  addToast(tString('main.finderExtensionNudge.notTurnedOn'), { level: 'warn' })
}
