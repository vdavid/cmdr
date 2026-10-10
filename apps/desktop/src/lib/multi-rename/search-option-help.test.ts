import { describe, it, expect } from 'vitest'
import { TOGGLE_COMMANDS, WHOLE_NAME_TOGGLES, type ToggleField } from './option-keys'
import { MARK_END, MARK_START } from './rename-examples'
import { SEARCH_OPTIONS, searchExampleKey, searchOptionExamples } from './search-option-help'

describe('search option help', () => {
  it('covers every search option with a key, and gives each its own glyph', () => {
    const fields = SEARCH_OPTIONS.map((option) => option.field)
    const wholeName: readonly ToggleField[] = WHOLE_NAME_TOGGLES
    expect(new Set(fields)).toEqual(
      new Set(Object.keys(TOGGLE_COMMANDS).filter((f) => !wholeName.includes(f as ToggleField))),
    )
    expect(new Set(SEARCH_OPTIONS.map((option) => option.glyph)).size).toBe(SEARCH_OPTIONS.length)
  })

  it('asks for each option’s replace with it on and off, every other option off, the replacement marked', () => {
    const asked = searchOptionExamples(SEARCH_OPTIONS)
    expect(asked.size).toBe(SEARCH_OPTIONS.length * 2)
    const on = asked.get(searchExampleKey('regex', true))
    const off = asked.get(searchExampleKey('regex', false))
    expect(on?.fileName).toBe('IMG_0042.jpg')
    expect(on?.spec).toMatchObject({ search: '\\d+', replace: `${MARK_START}#${MARK_END}`, regex: true })
    expect(on?.spec).toMatchObject({
      caseSensitive: false,
      firstOnly: false,
      includeExtension: false,
      substitute: false,
    })
    expect(off?.spec.regex).toBe(false)
  })
})
