/**
 * The query dialog's keys that aren't registry commands: the mode chips, the filter popovers'
 * openers, the scope popover's two presets, and the Enter combinations. One canonical combo per
 * platform, so each chip SHOWS the same combo its router MATCHES, and the two can't drift. The
 * dialog's three registry keys (new query, recent items, the numbered modes) are
 * `sources/search-dialog.ts`'s.
 *
 * ⌥ / Alt with a letter on every platform: the dialog owns the keyboard while it's up, and on
 * Linux the main window's menu mnemonics are masked meanwhile, so Alt+F reaches it
 * (`src-tauri/src/menu/DETAILS.md`). ⌥← / ⌥→ are deliberately absent: they're macOS's native
 * move-by-word in the focused query input.
 */

import { sameOnAll, type PlatformShortcuts } from '#lib/commands/platform-shortcuts.js'
import { getPlatform, isMacOS } from '#lib/platform/index.js'
import {
  formatChord,
  isAltGraphHeld,
  keyComboCandidates,
  toAriaKeyShortcut,
  toDisplayShortcut,
} from '#lib/shortcuts/key-capture.js'

const QUERY_KEYS = {
  modeAi: sameOnAll('⌥A'),
  modeFilename: sameOnAll('⌥F'),
  modeRegex: sameOnAll('⌥R'),
  sizeFilter: sameOnAll('⌥S'),
  dateFilter: sameOnAll('⌥M'),
  scopeFilter: sameOnAll('⌥I'),
  scopeCurrentFolder: sameOnAll('⌥C'),
  scopeVolume: sameOnAll('⌥V'),
  /** Fires the primary action over the result set (Search's "Show all in main window"). */
  primaryAction: sameOnAll('⌥Enter'),
  /** Bare Enter: what the run button and the footer's Go to file name. */
  enter: sameOnAll('Enter'),
} as const satisfies Record<string, PlatformShortcuts>

export type QueryKey = keyof typeof QUERY_KEYS

/** The key's combo on this platform, in its canonical spelling. */
export function queryKey(key: QueryKey): string {
  return QUERY_KEYS[key][getPlatform()][0]
}

/** Whether the keypress is exactly this key's combo, modifiers and all. */
export function matchesQueryKey(event: KeyboardEvent, key: QueryKey): boolean {
  const combos: readonly string[] = QUERY_KEYS[key][getPlatform()]
  const optionLetter = optionLetterCombo(event)
  return (
    keyComboCandidates(event).some((combo) => combos.includes(combo)) ||
    (optionLetter !== null && combos.includes(optionLetter))
  )
}

/**
 * ⌥ / Alt plus a letter, named by its key position, or `null`. macOS's ⌥ composes a glyph
 * (⌥F reports `ƒ`), and the registry's matcher reads a bare ⌥ letter as typing, so the chips'
 * keys need the position. With ⌘ / Ctrl / Super held it's another combo, and AltGr is typing.
 */
function optionLetterCombo(event: KeyboardEvent): string | null {
  if (!event.altKey || event.metaKey || event.ctrlKey || isAltGraphHeld(event)) return null
  const letter = /^Key([A-Z])$/.exec(event.code)?.[1]
  return letter === undefined ? null : formatChord({ altKey: true, shiftKey: event.shiftKey }, letter)
}

/**
 * The key as a chip or tooltip shows it: `⌥F` on macOS, `Alt+F` on Windows and Linux. On macOS
 * the dialog writes Enter as `⏎` (its run button reads "Search ⏎"), where the app-wide display
 * glyph is `↩`.
 */
export function queryKeyLabel(key: QueryKey): string {
  const combo = queryKey(key)
  if (isMacOS() && combo.endsWith('Enter')) return `${combo.slice(0, -'Enter'.length)}⏎`
  return toDisplayShortcut(combo)
}

/** The key in `aria-keyshortcuts` syntax (`Alt+F`). */
export function queryKeyAria(key: QueryKey): string {
  return toAriaKeyShortcut(queryKey(key))
}
