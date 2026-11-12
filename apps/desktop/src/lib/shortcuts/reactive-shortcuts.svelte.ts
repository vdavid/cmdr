/**
 * Reactive view over the shortcuts store, for long-lived UI like tooltips and hints.
 *
 * The store's readers are plain functions over a module-level Map, so a component
 * that reads one once goes stale when the user rebinds a shortcut in Settings. This
 * wrapper bumps a `$state` version on every store change (including the initial
 * custom-shortcut load in `initializeShortcuts`), so `$derived` consumers re-read.
 *
 * Every reader shows only LIVE keys (`getLiveShortcuts`): a default another command's
 * user binding shadows runs that other command (`binding-priority.ts`), so a hint naming
 * it would lie. Three readers, same version tick:
 *   - `getLiveShortcutsReactive(commandId)` — every live key (the palette shows up to
 *     three).
 *   - `getFirstShortcutReactive(commandId)` — the first live key (what menus and inline
 *     chips show).
 *   - `getFirstShiftShortcutReactive(commandId)` — the first live SHIFTED key (what the
 *     F-key bar's Shift row shows).
 *
 * Those three serve the binding surfaces (`ShortcutChip`, the F-key bar, the palette via
 * the chip). Feature code reads through two more, enforced by `cmdr/no-raw-shortcut-text`,
 * and they answer for live keys too:
 *   - `commandHasShortcut(commandId)`, the yes/no a sentence around a chip needs.
 *   - `firstShortcutIs(commandId, combo)`, for a sentence true of one key only.
 *   - `shortcutAsText(commandId, sink)`, for a sink that can only hold text.
 *
 * All readers return the DISPLAY form (`⌘⌫`, not the canonical `⌘Backspace`):
 * everything reading them renders the string to a user. Anything comparing or
 * dispatching a combo must call `getEffectiveShortcuts` instead.
 */
import { getLiveShortcuts, onShortcutChange } from './shortcuts-store'
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

/** A command's live keys in canonical form, subscribing the caller to shortcut changes. */
function liveReactive(commandId: string): string[] {
  ensureSubscribed()
  dependOn(version) // Subscribe $derived/$effect consumers to shortcut changes
  return getLiveShortcuts(commandId)
}

/**
 * Every live shortcut for a command in display form, reactively. A fresh array on every
 * call, so consumers can't mutate the store — don't cache the reference. Empty when no
 * key runs the command.
 */
export function getLiveShortcutsReactive(commandId: CommandId): string[] {
  return liveReactive(commandId).map(toDisplayShortcut)
}

/**
 * The first live shortcut for a command (the one menus show) in display form,
 * reactively. `undefined` when no key runs the command.
 */
export function getFirstShortcutReactive(commandId: string): string | undefined {
  const first = liveReactive(commandId).at(0)
  return first === undefined ? undefined : toDisplayShortcut(first)
}

/**
 * The first live shortcut for a command that carries Shift, in display form,
 * reactively. `undefined` when the command has no shifted live key.
 *
 * Deliberately no fallback to the unshifted binding: in the F-key bar's Shift row a
 * chip is a claim about what Shift+<key> does, and `file.rename` (bound to both `F2`
 * and `⇧F6`) would otherwise put a dead `F2` in the row's F6 slot. No chip — the
 * label alone, still clickable — beats a wrong one.
 */
export function getFirstShiftShortcutReactive(commandId: CommandId): string | undefined {
  const shifted = liveReactive(commandId).find(comboHasShift)
  return shifted === undefined ? undefined : toDisplayShortcut(shifted)
}

/**
 * Whether a key runs a command right now, reactively. The yes/no a sentence around a
 * `<ShortcutChip commandId>` needs: the chip renders nothing when no key is live, so the
 * caller picks the wording that doesn't name a key.
 */
export function commandHasShortcut(commandId: CommandId): boolean {
  return liveReactive(commandId).length > 0
}

/**
 * Whether the key a `<ShortcutChip commandId>` shows (the command's first live key) is
 * `combo`, in canonical form, reactively. For a sentence that's only true of one key: the
 * Quick Look hint's "works in Finder, too" holds for ⇧Space alone.
 */
export function firstShortcutIs(commandId: CommandId, combo: string): boolean {
  return liveReactive(commandId).at(0) === combo
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
 * - `settingDescription`: a setting's description, a string the row and the settings search share.
 *
 * Visible prose isn't on the list: there the shortcut is a `<ShortcutChip>` placed by a
 * `<Trans>` tag, so the key reads as a key and not as punctuation (`⌘⇧.` ending a sentence).
 */
export type ShortcutTextSink =
  | 'announcement'
  | 'ariaLabel'
  | 'tooltip'
  | 'toast'
  | 'errorMessage'
  | 'menuItem'
  | 'settingDescription'

/**
 * A command's first live key as display text (`⌘⌫`), reactively, for a sink that can only
 * hold text; `undefined` when no key runs it. The ONE way feature code reads a shortcut as text
 * (`cmdr/no-raw-shortcut-text`): naming the sink keeps each exception explicit and
 * searchable, and on screen a shortcut renders through `ShortcutChip`.
 */
export function shortcutAsText(commandId: CommandId, _sink: ShortcutTextSink): string | undefined {
  return getFirstShortcutReactive(commandId)
}
