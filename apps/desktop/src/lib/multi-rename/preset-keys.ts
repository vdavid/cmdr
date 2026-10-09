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
