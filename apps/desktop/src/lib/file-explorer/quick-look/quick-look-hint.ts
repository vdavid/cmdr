/**
 * Educational hint for Finder converts who press plain Space in the file list
 * expecting Quick Look. Cmdr uses Space for selection; ⇧Space is Quick Look.
 *
 * Behavior:
 *
 * - First Space press: toast appears with the explanation + a deep link to
 *   Settings > Keyboard shortcuts + a "Don't show again" button.
 * - X button on the toast: closes this instance only. The next Space press
 *   shows the toast again — the reminder keeps reminding until the user opts
 *   out explicitly.
 * - "Don't show again" button: closes the toast AND sets the
 *   `fileExplorer.suppressQuickLookHint` setting to `true`. Future Space
 *   presses never show the hint until the user flips the setting back off in
 *   Settings > Advanced.
 * - While the toast is already on screen: subsequent Space presses are a
 *   no-op (we don't re-add or replace the toast — the existing instance just
 *   sits there).
 *
 * The setting lives in Settings > Advanced as "Suppress the Space-key Quick
 * Look hint" so users can also turn it off pre-emptively without ever seeing
 * the toast.
 *
 * Trigger site: `FilePane.svelte`'s Space (unshifted) selection-toggle case.
 * The toast complements the selection — the selection still toggles, the
 * toast just educates.
 */

import { getSetting } from '$lib/settings'
import { addToast, getToasts } from '$lib/ui/toast'

import QuickLookHintToastContent from './QuickLookHintToastContent.svelte'
import { QUICK_LOOK_HINT_TOAST_ID } from './quick-look-hint-id'

export { QUICK_LOOK_HINT_TOAST_ID }

/**
 * Show the Quick Look hint toast unless (a) the user has permanently opted
 * out, or (b) the toast is already visible. Safe to call on every Space
 * press; the gates make repeated calls cheap.
 */
export function maybeShowQuickLookHint(): void {
  if (getSetting('fileExplorer.suppressQuickLookHint')) return
  // If the toast is already on screen (any prior Space press in this session
  // that hasn't been dismissed), do nothing. We don't re-add or replace —
  // mashing Space shouldn't churn the toast UI.
  if (getToasts().some((t) => t.id === QUICK_LOOK_HINT_TOAST_ID)) return
  addToast(QuickLookHintToastContent, {
    id: QUICK_LOOK_HINT_TOAST_ID,
    level: 'info',
    dismissal: 'persistent',
  })
}
