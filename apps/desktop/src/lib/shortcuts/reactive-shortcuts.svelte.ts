/**
 * Reactive view over the shortcuts store, for long-lived UI like tooltips and hints.
 *
 * `getEffectiveShortcuts` is a plain function over a module-level Map, so a component
 * that reads it once goes stale when the user rebinds a shortcut in Settings. This
 * wrapper bumps a `$state` version on every store change (including the initial
 * custom-shortcut load in `initializeShortcuts`), so `$derived` consumers re-read.
 *
 * Three readers, same version tick:
 *   - `getEffectiveShortcutsReactive(commandId)` — the full effective list (the palette
 *     shows up to three).
 *   - `getFirstShortcutReactive(commandId)` — `[0]` of that list (what menus and inline
 *     chips show).
 *   - `getFirstShiftShortcutReactive(commandId)` — the first SHIFTED binding (what the
 *     F-key bar's Shift row shows).
 *
 * One-off reads at event time (toasts, context menus) don't need this; they keep
 * calling `getEffectiveShortcuts` directly — and then apply `toDisplayShortcut`
 * themselves.
 *
 * Both readers return the DISPLAY form (`⌘⌫`, not the canonical `⌘Backspace`):
 * everything reading them renders the string to a user. Anything comparing or
 * dispatching a combo must call `getEffectiveShortcuts` instead.
 */
import { getEffectiveShortcuts, onShortcutChange } from './shortcuts-store'
import { comboHasShift, toDisplayShortcut } from './key-capture'
import type { CommandId } from '$lib/commands/command-ids'
import { dependOn } from '$lib/utils/reactivity'

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
 * All effective shortcuts for a command in display form, reactively. Returns a fresh
 * array on every call (`getEffectiveShortcuts` copies the store's data), so consumers
 * can't mutate the store — don't cache the reference. Empty when the command has no
 * binding.
 */
export function getEffectiveShortcutsReactive(commandId: CommandId): string[] {
  ensureSubscribed()
  dependOn(version) // Subscribe $derived/$effect consumers to shortcut changes
  return getEffectiveShortcuts(commandId).map(toDisplayShortcut)
}

/**
 * The first effective shortcut for a command (the one menus show) in display form,
 * reactively. Returns `undefined` when the command has no binding.
 */
export function getFirstShortcutReactive(commandId: string): string | undefined {
  return getEffectiveShortcutsReactive(commandId as CommandId)[0]
}

/**
 * The first effective shortcut for a command that carries Shift, in display form,
 * reactively. `undefined` when the command has no shifted binding.
 *
 * Deliberately no fallback to the unshifted binding: in the F-key bar's Shift row a
 * chip is a claim about what Shift+<key> does, and `file.rename` (bound to both `F2`
 * and `⇧F6`) would otherwise put a dead `F2` in the row's F6 slot. No chip — the
 * label alone, still clickable — beats a wrong one.
 */
export function getFirstShiftShortcutReactive(commandId: CommandId): string | undefined {
  ensureSubscribed()
  dependOn(version) // Subscribe $derived/$effect consumers to shortcut changes
  const shifted = getEffectiveShortcuts(commandId).find(comboHasShift)
  return shifted === undefined ? undefined : toDisplayShortcut(shifted)
}
