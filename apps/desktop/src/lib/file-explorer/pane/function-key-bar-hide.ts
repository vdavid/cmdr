/**
 * "Hide function key bar" from the bar's own right-click context menu: turns
 * the bar off and raises a self-dismissing toast pointing back at the row
 * that turns it on again.
 */
import { setSetting } from '$lib/settings'
import { addToast } from '$lib/ui/toast'
import FunctionKeyBarHiddenToastContent from './FunctionKeyBarHiddenToastContent.svelte'

/** Dedup id for the "function key bar hidden" INFO toast. */
export const FUNCTION_KEY_BAR_HIDDEN_TOAST_ID = 'function-key-bar-hidden'

/**
 * Twice the 4 s default, because this toast asks for more than a glance: it names
 * where the setting lives and carries the link that opens it. It still goes away on
 * its own (unlike the Quick Look and double-click hints, which stay until dismissed),
 * since the bar is back one Settings toggle away and the user just asked for it gone.
 */
const HIDDEN_TOAST_MS = 8000

/** Handles the `function-key-bar-hide-requested` event from the native context menu. */
export function handleFunctionKeyBarHideRequested(): void {
  setSetting('appearance.showFunctionKeyBar', false)
  addToast(FunctionKeyBarHiddenToastContent, {
    id: FUNCTION_KEY_BAR_HIDDEN_TOAST_ID,
    level: 'info',
    timeoutMs: HIDDEN_TOAST_MS,
  })
}
