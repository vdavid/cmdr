/**
 * Raising the "open Show in Finder in Cmdr?" offer.
 *
 * The offer is once-ever, and the nudge ledger is stamped the moment the toast
 * goes UP: a crash between raising it and answering it costs one offer, where
 * stamping on the answer would risk repeating the toast forever. The stamp also
 * starts the shared cooldown, so the Dock offer waits its turn.
 *
 * Whether to call this at all is `should-show-reveal-nudge.ts`; what the answers
 * do is `reveal-nudge-answer.ts`.
 */

import { addToast } from '$lib/ui/toast'
import { markNudgeOffered } from '$lib/nudges/nudge-store'
import { trackEvent } from '$lib/tauri-commands'
import { recordRevealNudgeAnswer } from './reveal-nudge-answer'
import RevealNudgeToastContent from './RevealNudgeToastContent.svelte'

/** Dedup id of the offer toast, so a second raise can never stack two. */
export const REVEAL_NUDGE_TOAST_ID = 'reveal-handler-nudge'

/**
 * Raises the offer and stamps the nudge ledger on the way up.
 *
 * Persistent on purpose: it's asked exactly once in the life of an install, and
 * a four-second toast would make that a coin flip on whether anyone read it.
 */
export function offerRevealHandler(): void {
  markNudgeOffered('reveal')
  void trackEvent('reveal_handler_offered')
  addToast(RevealNudgeToastContent, {
    level: 'info',
    dismissal: 'persistent',
    id: REVEAL_NUDGE_TOAST_ID,
    // Fires only on the frame's ×, never on the buttons' own `dismissToast`.
    onDismiss: () => {
      recordRevealNudgeAnswer('dismissed')
    },
  })
}
