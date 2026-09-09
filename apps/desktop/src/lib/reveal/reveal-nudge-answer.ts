/**
 * The three ways out of the "Show in Finder" offer, and what each one records.
 *
 * Separate from `reveal-nudge.ts` so the toast body can reach the answers
 * without importing the module that mounts it (`import-cycles`). Every exit
 * lands on a `reveal_handler_answered`, and the toast frame's × reports
 * `dismissed` rather than `no`: "read it and declined" and "swept it away" are
 * different answers, and the split is the interesting half of the question.
 * Mirrors `$lib/dock/dock-pin-answer.ts`.
 */

import { addToast, dismissToast } from '$lib/ui/toast'
import { setRevealHandlerEnabled, trackEvent } from '$lib/tauri-commands'
import { tString } from '$lib/intl/messages.svelte'
import { getAppLogger } from '$lib/logging/logger'
import type { RevealHandlerState } from '$lib/ipc/bindings'

const log = getAppLogger('reveal')

/** How the offer ended. `dismissed` is the toast frame's ×, ❌ never an active refusal. */
export type RevealNudgeAnswer = 'yes' | 'no' | 'dismissed'

/** Records how the offer ended. The one place `reveal_handler_answered` is sent. */
export function recordRevealNudgeAnswer(answer: RevealNudgeAnswer): void {
  void trackEvent('reveal_handler_answered', { answer })
}

/** "No, thanks": retire the toast and never ask again. */
export function declineRevealNudge(toastId: string): void {
  dismissToast(toastId)
  recordRevealNudgeAnswer('no')
}

/**
 * "Yes, open them in Cmdr": take the `NSFileViewer` key, then report what the
 * OS was actually left holding.
 *
 * ❗ The answer records the PRESS, ❌ never the outcome: what's being measured is
 * whether people want this. The state that comes back is the OS's, ❌ never the
 * one the click asked for — another app can take the key between the offer being
 * drawn and the button being pressed, and saying "done" then would be a lie.
 * Same rule the Settings row follows (`settings/sections/RevealHandlerCard.svelte`).
 */
export async function acceptRevealNudge(toastId: string): Promise<void> {
  dismissToast(toastId)
  recordRevealNudgeAnswer('yes')

  const { state } = await setRevealHandlerEnabled(true)
  if (state.kind === 'registered') {
    addToast(tString('main.revealNudge.turnedOn'), { level: 'success' })
    return
  }

  log.warn('Cmdr did not get the reveal handler: {kind}', { kind: state.kind })
  void trackEvent('reveal_handler_not_taken', { reason: state.kind })
  addToast(refusalMessage(state), { level: 'warn' })
}

/**
 * What to tell the user when the key didn't end up ours.
 *
 * `heldByOtherApp` earns its own line because it names who won the race and is
 * the one case where nothing is broken. The other two collapse into one honest
 * "not this time" plus the place to try again: `notRegistered` (the write went
 * nowhere) and `unavailable` (a build that may not write the key at all) look
 * identical from here and lead to the same next step.
 */
function refusalMessage(state: RevealHandlerState): string {
  if (state.kind === 'heldByOtherApp') {
    return tString('main.revealNudge.heldByOtherApp', { app: state.displayName ?? state.bundleId })
  }
  return tString('main.revealNudge.notTurnedOn')
}
