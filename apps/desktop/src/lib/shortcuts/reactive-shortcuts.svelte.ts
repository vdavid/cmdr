/**
 * Reactive view over the shortcuts store, for long-lived UI like tooltips and hints.
 *
 * `getEffectiveShortcuts` is a plain function over a module-level Map, so a component
 * that reads it once goes stale when the user rebinds a shortcut in Settings. This
 * wrapper bumps a `$state` version on every store change (including the initial
 * custom-shortcut load in `initializeShortcuts`), so `$derived` consumers re-read.
 *
 * One-off reads at event time (toasts, context menus) don't need this; they keep
 * calling `getEffectiveShortcuts` directly.
 */
import { getEffectiveShortcuts, onShortcutChange } from './shortcuts-store'

let version = $state(0)
let subscribed = false

// Lazy so merely importing the module doesn't register a listener; the subscription
// lives for the window's lifetime once any component reads a shortcut reactively.
function ensureSubscribed(): void {
  if (subscribed) return
  subscribed = true
  onShortcutChange(() => {
    version++
  })
}

/**
 * The first effective shortcut for a command (the one menus show), reactively.
 * Returns `undefined` when the command has no binding.
 */
export function getFirstShortcutReactive(commandId: string): string | undefined {
  ensureSubscribed()
  void version // Subscribe $derived/$effect consumers to shortcut changes
  return getEffectiveShortcuts(commandId)[0]
}
