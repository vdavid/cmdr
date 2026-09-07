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

/** Handles the `function-key-bar-hide-requested` event from the native context menu. */
export function handleFunctionKeyBarHideRequested(): void {
  setSetting('appearance.showFunctionKeyBar', false)
  addToast(FunctionKeyBarHiddenToastContent, {
    id: FUNCTION_KEY_BAR_HIDDEN_TOAST_ID,
    level: 'info',
  })
}
