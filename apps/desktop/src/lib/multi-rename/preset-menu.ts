/**
 * The Presets menu's rows (F2 in the Multi-rename sheet), built from data so the
 * house `Menu` draws and drives them. Pure: the sheet hands in the saved presets,
 * what's loaded, and the words; it gets back sections whose rows carry a typed action.
 */

import type { MenuItem, MenuSection } from '$lib/ui/menu-types'
import type { MultiRenamePreset, MultiRenameSpec } from '$lib/tauri-commands'
import type { LoadedPreset } from './multi-rename-state.svelte'
import { specsEqual } from './spec'

/** Saved presets past this many have no digit: the keys stop at 9. */
const NUMBERED = 9

/** What picking a row does. */
export type PresetAction =
  | { kind: 'load'; preset: LoadedPreset }
  | { kind: 'reset' }
  | { kind: 'saveAs' }
  | { kind: 'rename'; id: string }
  | { kind: 'update'; id: string }
  | { kind: 'delete'; id: string }

export interface PresetMenuInput {
  saved: MultiRenamePreset[]
  builtIns: { id: string; name: string }[]
  loaded: LoadedPreset | null
  /** The fields as they are now, so Update is offered only where it would change something. */
  current: MultiRenameSpec
  labels: { saved: string; reset: string; saveAs: string; rename: string; update: string; delete: string }
}

export function presetMenuSections(input: PresetMenuInput): MenuSection<PresetAction>[] {
  const { saved, builtIns, loaded, current, labels } = input
  const isLoaded = (preset: LoadedPreset): boolean => loaded?.kind === preset.kind && loaded.id === preset.id

  const savedRows = saved.map((preset, index): MenuItem<PresetAction> => {
    const self: LoadedPreset = { kind: 'saved', id: preset.id }
    return {
      value: `saved:${preset.id}`,
      label: preset.name,
      accelerator: index < NUMBERED ? String(index + 1) : undefined,
      check: isLoaded(self) ? { kind: 'current' } : undefined,
      data: { kind: 'load', preset: self },
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

  const builtInRows = builtIns.map((preset): MenuItem<PresetAction> => {
    const self: LoadedPreset = { kind: 'builtIn', id: preset.id }
    return {
      value: `builtIn:${preset.id}`,
      label: preset.name,
      check: isLoaded(self) ? { kind: 'current' } : undefined,
      data: { kind: 'load', preset: self },
    }
  })

  const sections: MenuSection<PresetAction>[] = []
  if (savedRows.length > 0) sections.push({ id: 'saved', heading: labels.saved, items: savedRows })
  sections.push({ id: 'builtIn', items: builtInRows })
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
