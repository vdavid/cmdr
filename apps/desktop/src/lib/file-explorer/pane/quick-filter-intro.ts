/**
 * The once-ever "What just happened?" toast the first quick filter raises.
 *
 * Filter is the default typing mode, so the first letter typed in a pane hides
 * every row that doesn't match, which surprises anyone who expected a jump. The
 * toast says what happened, that Esc brings every file back, and offers Jump.
 *
 * Its buttons' actions live in `quick-filter-intro-actions.ts`, which the toast
 * component imports (this module imports the component, so not from here).
 *
 * The seen-flag is stamped when the toast goes UP, ❌ never on an answer: a crash
 * mid-toast costs one explanation, where the other order risks repeating it.
 */

import { addToast } from '$lib/ui/toast'
import { getSetting, setSetting } from '$lib/settings'
import { QUICK_FILTER_INTRO_TOAST_ID } from './quick-filter-intro-actions'
import QuickFilterIntroToastContent from './QuickFilterIntroToastContent.svelte'

/** Raises the toast unless it was ever shown. */
export function maybeShowQuickFilterIntro(): void {
  if (getSetting('fileExplorer.quickFilterIntroSeen')) return
  setSetting('fileExplorer.quickFilterIntroSeen', true)
  // Persistent: it's shown once in the life of an install, so it waits to be read.
  addToast(QuickFilterIntroToastContent, {
    level: 'info',
    dismissal: 'persistent',
    id: QUICK_FILTER_INTRO_TOAST_ID,
  })
}
