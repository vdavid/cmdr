/**
 * Raising the offer to put "Open in Cmdr" at the top level of Finder's right-click
 * menu. Same shape as `reveal-nudge.ts`: the ledger is stamped as the toast goes UP,
 * and the toast is persistent, since it's asked once in the life of an install.
 *
 * Whether to call this is `should-show-finder-extension-nudge.ts`; what the answers
 * do is `finder-extension-nudge-answer.ts`.
 */

import { addToast } from '#lib/ui/toast/index.js'
import { markNudgeOffered } from '#lib/nudges/nudge-store.js'
import { trackEvent } from '#lib/tauri-commands/index.js'
import { recordFinderExtensionNudgeAnswer } from './finder-extension-nudge-answer'
import FinderExtensionNudgeToastContent from './FinderExtensionNudgeToastContent.svelte'

/** Dedup id of the offer toast, so a second raise can never stack two. */
export const FINDER_EXTENSION_NUDGE_TOAST_ID = 'finder-extension-nudge'

export function offerFinderExtension(): void {
  markNudgeOffered('finderExtension')
  void trackEvent('finder_extension_offered')
  addToast(FinderExtensionNudgeToastContent, {
    level: 'info',
    dismissal: 'persistent',
    id: FINDER_EXTENSION_NUDGE_TOAST_ID,
    // Fires only on the frame's ×, never on the buttons' own `dismissToast`.
    onDismiss: () => {
      recordFinderExtensionNudgeAnswer('dismissed')
    },
  })
}
