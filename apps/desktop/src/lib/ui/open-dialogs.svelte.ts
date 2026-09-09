/**
 * Which soft dialogs are on screen in THIS window right now.
 *
 * `ModalDialog` registers every dialog it renders, from the same mount/destroy pair
 * that tells the Rust `SoftDialogTracker`. That's what makes the set EXHAUSTIVE
 * without anyone maintaining a list: rendering a soft dialog means rendering a
 * `ModalDialog` with a `SoftDialogId`, so a dialog nobody remembered to mention still
 * shows up here. Svelte guarantees the pairing, and a missed close would leave file
 * operations blocked for the rest of the session.
 *
 * `OnboardingWizard.svelte` is the ONE other registrar: it wears `ModalDialog`'s
 * chrome but not the component (bespoke sheet, no Escape, a portal target of its own),
 * so it makes the same paired `onMount` / `onDestroy` announcement by hand. A third one
 * would be a bad sign; reach for `ModalDialog` instead. ❌ Never mark a dialog open from
 * anywhere else.
 *
 * Per-window by construction: each webview evaluates its own copy of this module,
 * so the viewer's dialogs never appear in the main window's set. The Rust tracker
 * is the app-wide view, and it's the one MCP reads.
 */

import { SvelteSet } from 'svelte/reactivity'
import { dialogBlocksOperations, type SoftDialogId } from './dialog-registry'

const openDialogs = new SvelteSet<SoftDialogId>()

/** Called by `ModalDialog` (and `OnboardingWizard`) on mount. */
export function markDialogOpen(id: SoftDialogId): void {
  openDialogs.add(id)
}

/** Called by `ModalDialog` (and `OnboardingWizard`) on destroy. */
export function markDialogClosed(id: SoftDialogId): void {
  openDialogs.delete(id)
}

/** Whether any soft dialog is on screen in this window. Reactive. */
export function isAnySoftDialogOpen(): boolean {
  return openDialogs.size > 0
}

/**
 * The dialog standing in the way of starting a file operation, or `null` when
 * nothing is. Reactive.
 *
 * The MOST RECENTLY opened one wins: dialogs do stack (a rollback confirmation
 * over the progress dialog, the quit countdown over anything), and the topmost is
 * the one the user or the agent has to deal with first.
 */
export function blockingSoftDialog(): SoftDialogId | null {
  let blocking: SoftDialogId | null = null
  for (const id of openDialogs) {
    if (dialogBlocksOperations(id)) blocking = id
  }
  return blocking
}

/** Test-only reset. Production code never empties the set by hand. */
export function _resetOpenDialogsForTesting(): void {
  openDialogs.clear()
}
