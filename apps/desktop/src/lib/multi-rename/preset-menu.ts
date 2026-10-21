/**
 * The Presets menu's rows (F2 in the Multi-rename sheet), built from data so the
 * house `Menu` draws and drives them. Pure: the sheet hands in the presets (top to
 * bottom), what's loaded, and the words; it gets back sections whose rows carry a typed action.
 */

import type { MenuItem, MenuSection } from '#lib/ui/menu-types.js'
import type { MultiRenamePreset, MultiRenameSpec } from '#lib/tauri-commands/index.js'
import { specsEqual } from './spec'

/** Presets past this many have no digit: the keys stop at 9. */
const NUMBERED = 9

/** What picking a row does. */
export type PresetAction =
  | { kind: 'load'; id: string }
  | { kind: 'reset' }
  | { kind: 'saveAs' }
  | { kind: 'rename'; id: string }
  | { kind: 'update'; id: string }
  | { kind: 'delete'; id: string }

export interface PresetMenuInput {
  /** Top to bottom: a preset's number is its place. */
  saved: MultiRenamePreset[]
  /** The id of the preset the fields came from. */
  loadedId: string | null
  /** The fields as they are now, so Update is offered only where it would change something. */
  current: MultiRenameSpec
  labels: { saved: string; reset: string; saveAs: string; rename: string; update: string; delete: string }
}

export function presetMenuSections(input: PresetMenuInput): MenuSection<PresetAction>[] {
  const { saved, loadedId, current, labels } = input

  const savedRows = saved.map((preset, index): MenuItem<PresetAction> => {
    return {
      value: `saved:${preset.id}`,
      label: preset.name,
      accelerator: index < NUMBERED ? String(index + 1) : undefined,
      check: preset.id === loadedId ? { kind: 'current' } : undefined,
      data: { kind: 'load', id: preset.id },
      submenu: [
        { value: `saved:${preset.id}:rename`, label: labels.rename, data: { kind: 'rename', id: preset.id } },
        {
          value: `saved:${preset.id}:update`,
          label: labels.update,
          disabled: specsEqual(preset.spec, current),
          data: { kind: 'update', id: preset.id },
        },
        { value: `saved:${preset.id}:delete`, label: labels.delete, data: { kind: 'delete', id: preset.id } },
      ],
    }
  })

  const sections: MenuSection<PresetAction>[] = []
  if (savedRows.length > 0) sections.push({ id: 'saved', heading: labels.saved, items: savedRows })
  sections.push({
    id: 'actions',
    items: [
      { value: 'reset', label: labels.reset, data: { kind: 'reset' } },
      { value: 'saveAs', label: labels.saveAs, data: { kind: 'saveAs' } },
    ],
  })
  return sections
}

/** What the name popover is for: saving the fields as a preset, or renaming preset `id`. */
export type PresetNameMode = { kind: 'save' } | { kind: 'rename'; id: string }

/**
 * The OTHER saved preset that naming one `name` would replace, which the popover asks
 * about first. A rename to its own name (in any case) replaces nothing.
 */
export function presetNameClash(
  mode: PresetNameMode,
  name: string,
  presetNamed: (name: string) => MultiRenamePreset | undefined,
): MultiRenamePreset | undefined {
  const taken = presetNamed(name)
  return mode.kind === 'rename' && taken?.id === mode.id ? undefined : taken
}
