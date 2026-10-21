import { describe, it, expect } from 'vitest'
import { DEFAULT_SPEC, PRESET_SEEDS, insertAtCaret, specsEqual } from './spec'

describe('multi-rename spec helpers', () => {
  it('the default changes nothing', () => {
    expect(DEFAULT_SPEC.nameMask).toBe('[N]')
    expect(DEFAULT_SPEC.extensionMask).toBe('[E]')
    expect(DEFAULT_SPEC.removeDiacritics).toBe(false)
    expect(DEFAULT_SPEC.greekToLatin).toBe(false)
    expect(DEFAULT_SPEC.normalizeUnicode).toBe(false)
  })

  it('seeds Date and time, then one preset per whole-name option, each changing that alone', () => {
    expect(PRESET_SEEDS.map((seed) => seed.nameKey)).toEqual([
      'multiRename.preset.dateAndTime',
      'multiRename.preset.removeDiacritics',
      'multiRename.preset.greekToLatin',
      'multiRename.preset.normalizeUnicode',
    ])
    expect(PRESET_SEEDS.map((seed) => seed.spec)).toEqual([
      { ...DEFAULT_SPEC, nameMask: '[Y-M-D h.m.s]' },
      { ...DEFAULT_SPEC, removeDiacritics: true },
      { ...DEFAULT_SPEC, greekToLatin: true },
      { ...DEFAULT_SPEC, normalizeUnicode: true },
    ])
  })

  it('compares two specs field by field, whatever order the fields came in', () => {
    const reordered = Object.fromEntries(Object.entries(DEFAULT_SPEC).reverse()) as typeof DEFAULT_SPEC
    expect(specsEqual(DEFAULT_SPEC, reordered)).toBe(true)
    expect(specsEqual(DEFAULT_SPEC, { ...DEFAULT_SPEC, nameMask: '[N] [C]' })).toBe(false)
    expect(specsEqual(DEFAULT_SPEC, { ...DEFAULT_SPEC, regex: true })).toBe(false)
    expect(specsEqual(DEFAULT_SPEC, { ...DEFAULT_SPEC, normalizeUnicode: true })).toBe(false)
  })

  it('inserts a placeholder at the caret, or at the end', () => {
    expect(insertAtCaret('IMG ', '[C]', 4)).toEqual({ mask: 'IMG [C]', caret: 7 })
    expect(insertAtCaret('[N]', ' [C]', null)).toEqual({ mask: '[N] [C]', caret: 7 })
    expect(insertAtCaret('ab', 'X', 1)).toEqual({ mask: 'aXb', caret: 2 })
  })
})
