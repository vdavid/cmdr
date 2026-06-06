/**
 * Reactive view over the shortcuts store, for long-lived UI like tooltips and hints.
 *
 * `getEffectiveShortcuts` is a plain function over a module-level Map, so a component
 * that reads it once goes stale when the user rebinds a shortcut in Settings. This
 * wrapper bumps a `$state` version on every store change (including the initial
 * custom-shortcut load in `initializeShortcuts`), so `$derived` consumers re-read.
 *
 * Two readers, same version tick:
 *   - `getEffectiveShortcutsReactive(commandId)` — the full effective list (the palette
 *     shows up to three).
 *   - `getFirstShortcutReactive(commandId)` — `[0]` of that list (what menus and inline
 *     chips show).
 *
 * One-off reads at event time (toasts, context menus) don't need this; they keep
 * calling `getEffectiveShortcuts` directly.
 */
import { getEffectiveShortcuts, onShortcutChange } from './shortcuts-store'
import type { CommandId } from '$lib/commands/command-ids'

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
 * All effective shortcuts for a command, reactively. Returns a fresh array on every
 * call (`getEffectiveShortcuts` copies the store's data), so consumers can't mutate
 * the store — don't cache the reference. Empty when the command has no binding.
 */
export function getEffectiveShortcutsReactive(commandId: CommandId): string[] {
  ensureSubscribed()
  void version // Subscribe $derived/$effect consumers to shortcut changes
  return getEffectiveShortcuts(commandId)
}

/**
 * The first effective shortcut for a command (the one menus show), reactively.
 * Returns `undefined` when the command has no binding.
 */
export function getFirstShortcutReactive(commandId: string): string | undefined {
  return getEffectiveShortcutsReactive(commandId as CommandId)[0]
}
