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
 * Those three serve the binding surfaces (`ShortcutChip`, the F-key bar, the palette via
 * the chip). Feature code reads through two more, enforced by `cmdr/no-raw-shortcut-text`:
 *   - `commandHasShortcut(commandId)`, the yes/no a sentence around a chip needs.
 *   - `shortcutAsText(commandId, sink)`, for a sink that can only hold text.
 *
 * All readers return the DISPLAY form (`⌘⌫`, not the canonical `⌘Backspace`):
 * everything reading them renders the string to a user. Anything comparing or
 * dispatching a combo must call `getEffectiveShortcuts` instead.
 */
import { getEffectiveShortcuts, onShortcutChange } from './shortcuts-store'
import { comboHasShift, toDisplayShortcut } from './key-capture'
import type { CommandId } from '#lib/commands/command-ids.js'
import { dependOn } from '#lib/utils/reactivity.js'

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

/**
 * Whether a command has a binding right now, reactively. The yes/no a sentence around a
 * `<ShortcutChip commandId>` needs: the chip renders nothing when unbound, so the caller
 * picks the wording that doesn't name a key.
 */
export function commandHasShortcut(commandId: CommandId): boolean {
  return getEffectiveShortcutsReactive(commandId).length > 0
}

/**
 * The places a shortcut may appear as plain text, because they can't hold a component:
 *
 * - `announcement`: a live region a screen reader speaks.
 * - `ariaLabel`: an accessible name or description.
 * - `tooltip`: the tooltip action's `shortcut` field.
 * - `toast`: a toast whose content is a string.
 * - `errorMessage`: a message built as a string (the transfer error catalog).
 * - `menuItem`: a menu or select option's label.
 *
 * Visible prose isn't on the list: there the shortcut is a `<ShortcutChip>` placed by a
 * `<Trans>` tag, so the key reads as a key and not as punctuation (`⌘⇧.` ending a sentence).
 */
export type ShortcutTextSink = 'announcement' | 'ariaLabel' | 'tooltip' | 'toast' | 'errorMessage' | 'menuItem'

/**
 * A command's first binding as display text (`⌘⌫`), reactively, for a sink that can only
 * hold text; `undefined` when unbound. The ONE way feature code reads a shortcut as text
 * (`cmdr/no-raw-shortcut-text`): naming the sink keeps each exception explicit and
 * searchable, and on screen a shortcut renders through `ShortcutChip`.
 */
export function shortcutAsText(commandId: CommandId, _sink: ShortcutTextSink): string | undefined {
  return getEffectiveShortcutsReactive(commandId)[0]
}
