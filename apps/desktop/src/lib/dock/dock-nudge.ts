/**
 * Raising the "keep Cmdr in your Dock?" offer.
 *
 * The offer is once-ever, and the flag is spent the moment the toast goes UP:
 * a crash between raising it and answering it costs one offer, where spending
 * the flag on the answer would risk repeating the toast forever.
 *
 * Whether to call this at all is `should-show-dock-nudge.ts`; what the answers
 * do is `dock-pin-answer.ts`.
 */

import { addToast } from '$lib/ui/toast'
import { setSetting } from '$lib/settings'
import { trackEvent } from '$lib/tauri-commands'
import { recordDockPinAnswer } from './dock-pin-answer'
import DockPinNudgeToastContent from './DockPinNudgeToastContent.svelte'

/** Dedup id of the offer toast, so a second raise can never stack two. */
export const DOCK_PIN_NUDGE_TOAST_ID = 'dock-pin-nudge'

/**
 * Raises the offer and spends `behavior.dockPinNudgeSeen` on the way up.
 *
 * Persistent on purpose: it's asked exactly once in the life of an install, and
 * a four-second toast would make that a coin flip on whether anyone read it.
 */
export function offerDockPin(): void {
  setSetting('behavior.dockPinNudgeSeen', true)
  void trackEvent('dock_pin_offered')
  addToast(DockPinNudgeToastContent, {
    level: 'info',
    dismissal: 'persistent',
    id: DOCK_PIN_NUDGE_TOAST_ID,
    // Fires only on the frame's ×, never on the buttons' own `dismissToast`.
    onDismiss: () => {
      recordDockPinAnswer('dismissed')
    },
  })
}
