/**
 * The Multi-rename sheet's preset keys: F2 opens the Presets menu (Total Commander's
 * key for it) and ⌘S saves the current fields as a preset. Both are registry commands
 * (`multiRename.openPresets`, `multiRename.savePreset`), so the sheet asks the registry
 * rather than testing key flags.
 */

import { eventMatchesCommand } from '$lib/shortcuts'

/** What a keypress in the sheet means for its presets, if anything. */
export type PresetKey = 'openMenu' | 'save'

export function presetKeyOf(event: KeyboardEvent): PresetKey | null {
  // A key that finishes an IME composition belongs to the field, never to the sheet.
  if (event.isComposing) return null
  if (eventMatchesCommand(event, 'multiRename.openPresets')) return 'openMenu'
  if (eventMatchesCommand(event, 'multiRename.savePreset')) return 'save'
  return null
}

/** The two roads one F2 can arrive by: the sheet's keydown, and File > Rename's accelerator. */
export type KeyRoad = 'keyboard' | 'menu'

/**
 * The two fires of one keypress land milliseconds apart; a person can't press the key and
 * pick the menu item inside this. Same window as the dispatch core's (`dispatch-dedup.ts`).
 */
const ECHO_WINDOW_MS = 300

/**
 * Tells the second half of a keyboard + menu double fire from a real press: the OTHER
 * road within the window is the echo, while a second press on the same road (a quick
 * double tap, key repeat) always counts. An echo doesn't extend the window. The sheet's
 * F2 toggles its menu, so an echo let through would close what the first half opened.
 */
export function createKeyRoadEcho(now: () => number = Date.now): (road: KeyRoad) => boolean {
  let last: { road: KeyRoad; at: number } | null = null
  return (road) => {
    const at = now()
    if (last !== null && last.road !== road && at - last.at < ECHO_WINDOW_MS) return true
    last = { road, at }
    return false
  }
}
