import { describe, expect, it } from 'vitest'
import type { MultiRenamePreset } from '#lib/tauri-commands/index.js'
import { presetMenuSections, presetNameClash, type PresetMenuInput } from './preset-menu'
import { DEFAULT_SPEC } from './spec'

const labels = {
  saved: 'Your presets',
  reset: 'Reset all fields',
  saveAs: 'Save current as…',
  rename: 'Rename…',
  update: 'Update',
  moveUp: 'Move up',
  moveDown: 'Move down',
  delete: 'Delete',
}

function saved(count: number): MultiRenamePreset[] {
  return Array.from({ length: count }, (_, i) => ({
    id: `p${String(i + 1)}`,
    name: `Preset ${String(i + 1)}`,
    spec: { ...DEFAULT_SPEC, nameMask: `[N]-${String(i + 1)}` },
  }))
}

function input(overrides: Partial<PresetMenuInput> = {}): PresetMenuInput {
  return {
    saved: saved(2),
    loadedId: null,
    current: DEFAULT_SPEC,
    labels,
    ...overrides,
  }
}

describe('presetMenuSections', () => {
  it('lists the presets top to bottom, then Reset all fields and Save current as…', () => {
    const sections = presetMenuSections(input())
    expect(sections.map((s) => s.items.map((i) => i.label))).toEqual([
      ['Preset 1', 'Preset 2'],
      ['Reset all fields', 'Save current as…'],
    ])
  })

  it('heads the presets “Your presets”', () => {
    const [savedSection, actions] = presetMenuSections(input())
    expect(savedSection.heading).toBe('Your presets')
    expect(actions.heading).toBeUndefined()
  })

  it('numbers the first nine saved presets, and only those', () => {
    const [savedSection] = presetMenuSections(input({ saved: saved(11) }))
    expect(savedSection.items.map((i) => i.accelerator)).toEqual([
      ...['1', '2', '3', '4', '5', '6', '7', '8', '9'],
      undefined,
      undefined,
    ])
  })

  it('gives each preset Rename, Update, Move up, Move down, and Delete', () => {
    const [savedSection] = presetMenuSections(input())
    expect(savedSection.items[0].submenu?.map((i) => i.data)).toEqual([
      { kind: 'rename', id: 'p1' },
      { kind: 'update', id: 'p1' },
      { kind: 'move', id: 'p1', direction: 'up' },
      { kind: 'move', id: 'p1', direction: 'down' },
      { kind: 'delete', id: 'p1' },
    ])
  })

  it('greys Move up on the top preset and Move down on the bottom one', () => {
    const [savedSection] = presetMenuSections(input({ saved: saved(3) }))
    const moves = savedSection.items.map((row) =>
      (row.submenu ?? []).filter((i) => i.data?.kind === 'move').map((i) => i.disabled === true),
    )
    expect(moves).toEqual([
      [true, false],
      [false, false],
      [false, true],
    ])
  })

  it('offers Update only where it would change the preset', () => {
    const [savedSection] = presetMenuSections(input({ current: saved(1)[0].spec }))
    const updateOf = (index: number) => savedSection.items[index].submenu?.find((i) => i.data?.kind === 'update')
    expect(updateOf(0)?.disabled).toBe(true)
    expect(updateOf(1)?.disabled).toBeFalsy()
  })

  it('marks the loaded preset, and every row loads or acts through its data', () => {
    const sections = presetMenuSections(input({ loadedId: 'p2' }))
    const rows = sections.flatMap((s) => s.items)
    expect(rows.find((i) => i.check)?.label).toBe('Preset 2')
    expect(rows.map((i) => i.data)).toEqual([
      { kind: 'load', id: 'p1' },
      { kind: 'load', id: 'p2' },
      { kind: 'reset' },
      { kind: 'saveAs' },
    ])
    const values = [...rows, ...rows.flatMap((i) => i.submenu ?? [])].map((i) => i.value)
    expect(new Set(values).size).toBe(values.length)
  })

  it('leaves out the presets section while there are none', () => {
    expect(presetMenuSections(input({ saved: [] })).map((s) => s.id)).toEqual(['actions'])
  })
})

describe('presetNameClash', () => {
  const list = saved(2)
  const presetNamed = (name: string) => list.find((p) => p.name.toLowerCase() === name.trim().toLowerCase())

  it('saving under a taken name clashes with that preset', () => {
    expect(presetNameClash({ kind: 'save' }, ' preset 2', presetNamed)?.id).toBe('p2')
    expect(presetNameClash({ kind: 'save' }, 'Fresh', presetNamed)).toBeUndefined()
  })

  it('renaming onto another preset clashes, onto its own name never does', () => {
    expect(presetNameClash({ kind: 'rename', id: 'p1' }, 'Preset 2', presetNamed)?.id).toBe('p2')
    expect(presetNameClash({ kind: 'rename', id: 'p1' }, 'PRESET 1', presetNamed)).toBeUndefined()
  })
})
