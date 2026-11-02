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
 * Two others register by hand. `OnboardingWizard.svelte` wears `ModalDialog`'s chrome but
 * not the component (bespoke sheet, no Escape, a portal target of its own), so it makes
 * the same paired `onMount` / `onDestroy` announcement. The inline rename editor isn't a
 * dialog at all, but it waits on a person like one (`rename-editor-presence.ts`). A fourth
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

/** Whether any soft dialog OTHER than `id` is on screen in this window. Reactive.
 *  For a dialog that waits for the window to be free, where its own previous
 *  question still being up is no reason to hold the next one. */
export function isAnySoftDialogOpenBesides(id: SoftDialogId): boolean {
  return isAnySoftDialogOpenBesidesAll([id])
}

/** Whether any soft dialog other than every one of `ids` is on screen in this
 *  window. Reactive. For a prompt that may stack over one dialog it belongs to,
 *  like a move's question over that move's own progress dialog. */
export function isAnySoftDialogOpenBesidesAll(ids: readonly SoftDialogId[]): boolean {
  for (const open of openDialogs) {
    if (!ids.includes(open)) return true
  }
  return false
}

/**
 * Whether one named soft dialog is on screen in this window. Reactive.
 *
 * The answer comes from the same paired mount/destroy announcement every dialog makes, so
 * it can't drift from what's rendered: an ambient affordance that has to step aside for a
 * dialog (the FDA badge, while onboarding's own FDA step is up) asks here instead of
 * threading the hosting component's visibility flag down to itself.
 */
export function isDialogOpen(id: SoftDialogId): boolean {
  return openDialogs.has(id)
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
  return blockingSoftDialogBesides([])
}

/** {@link blockingSoftDialog}, not counting `ids`: for an action that belongs to one of them. */
export function blockingSoftDialogBesides(ids: readonly SoftDialogId[]): SoftDialogId | null {
  let blocking: SoftDialogId | null = null
  for (const id of openDialogs) {
    if (dialogBlocksOperations(id) && !ids.includes(id)) blocking = id
  }
  return blocking
}

/** Test-only reset. Production code never empties the set by hand. */
export function _resetOpenDialogsForTesting(): void {
  openDialogs.clear()
}
