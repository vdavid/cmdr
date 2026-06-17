import { describe, it, expect } from 'vitest'
import { getAdvancedSettings } from '../settings-registry'
import { groupAdvancedByCard } from './advanced-grouping'
import type { SettingDefinition } from '../types'

/** Minimal stub for the pure grouping unit tests (only `id` and `card` are read). */
function stub(id: string, card?: string): SettingDefinition {
  return { id, card } as unknown as SettingDefinition
}

describe('groupAdvancedByCard (pure)', () => {
  it('groups settings by their resolved card title in first-appearance order', () => {
    const groups = groupAdvancedByCard([
      stub('a', 'Performance'),
      stub('b', 'Performance'),
      stub('c', 'Input'),
      stub('d', 'Performance'),
    ])
    expect(groups.map((g) => g.title)).toEqual(['Performance', 'Input'])
    expect(groups[0].settings.map((s) => s.id)).toEqual(['a', 'b', 'd'])
    expect(groups[1].settings.map((s) => s.id)).toEqual(['c'])
  })

  it('buckets settings with no card into a trailing untitled group', () => {
    const groups = groupAdvancedByCard([stub('a', 'Performance'), stub('b', undefined), stub('c', 'Performance')])
    expect(groups.map((g) => g.title)).toEqual(['Performance', ''])
    expect(groups[1].settings.map((s) => s.id)).toEqual(['b'])
  })

  it('returns an empty array for no settings', () => {
    expect(groupAdvancedByCard([])).toEqual([])
  })
})

describe('groupAdvancedByCard (registry guard)', () => {
  it('every Advanced setting lands in exactly one card (set-equality)', () => {
    const advanced = getAdvancedSettings()
    const groups = groupAdvancedByCard(advanced)
    const grouped = groups.flatMap((g) => g.settings)

    // No setting appears twice.
    const ids = grouped.map((s) => s.id)
    expect(new Set(ids).size).toBe(ids.length)

    // The union of grouped settings === every Advanced setting. This is the
    // regression guard: a new `showInAdvanced` setting without a `cardKey` would
    // fall into the untitled "Other" bucket rather than vanish — which still
    // shows up here as a non-empty `''` group, flagging that it needs a real
    // card home.
    expect(new Set(ids)).toEqual(new Set(advanced.map((s) => s.id)))
  })

  it('leaves no Advanced setting in the untitled "Other" bucket (every one has a cardKey)', () => {
    const groups = groupAdvancedByCard(getAdvancedSettings())
    const other = groups.find((g) => g.title === '')
    expect(
      other,
      'an Advanced setting is missing a cardKey: ' + (other?.settings.map((s) => s.id).join(', ') ?? ''),
    ).toBeUndefined()
  })

  it('places the natural-section mirrors under their reused card title', () => {
    const groups = groupAdvancedByCard(getAdvancedSettings())
    const byId = (id: string) => groups.find((g) => g.settings.some((s) => s.id === id))
    // The three natural-page mirrors reuse their natural-page card titles.
    expect(byId('network.smbConcurrency')?.title).toBe('Performance and timeouts')
    expect(byId('fileOperations.maxConflictsToShow')?.title).toBe('Conflicts and progress')
    expect(byId('fileOperations.progressUpdateInterval')?.title).toBe('Conflicts and progress')
  })
})
