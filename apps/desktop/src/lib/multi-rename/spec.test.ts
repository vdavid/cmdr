import { describe, it, expect } from 'vitest'
import { BUILT_IN_PRESETS, DEFAULT_SPEC, insertAtCaret, specsEqual } from './spec'

describe('multi-rename spec helpers', () => {
  it('the default changes nothing', () => {
    expect(DEFAULT_SPEC.nameMask).toBe('[N]')
    expect(DEFAULT_SPEC.extensionMask).toBe('[E]')
    expect(DEFAULT_SPEC.removeDiacritics).toBe(false)
  })

  it('ships a remove-diacritics preset that only removes diacritics', () => {
    const preset = BUILT_IN_PRESETS.find((p) => p.id === 'builtin:remove-diacritics')
    expect(preset?.spec).toEqual({ ...DEFAULT_SPEC, removeDiacritics: true })
  })

  it('compares two specs field by field, whatever order the fields came in', () => {
    const reordered = Object.fromEntries(Object.entries(DEFAULT_SPEC).reverse()) as typeof DEFAULT_SPEC
    expect(specsEqual(DEFAULT_SPEC, reordered)).toBe(true)
    expect(specsEqual(DEFAULT_SPEC, { ...DEFAULT_SPEC, counterDigits: 2 })).toBe(false)
    expect(specsEqual(DEFAULT_SPEC, { ...DEFAULT_SPEC, regex: true })).toBe(false)
  })

  it('inserts a placeholder at the caret, or at the end', () => {
    expect(insertAtCaret('IMG ', '[C]', 4)).toEqual({ mask: 'IMG [C]', caret: 7 })
    expect(insertAtCaret('[N]', ' [C]', null)).toEqual({ mask: '[N] [C]', caret: 7 })
    expect(insertAtCaret('ab', 'X', 1)).toEqual({ mask: 'aXb', caret: 2 })
  })
})
