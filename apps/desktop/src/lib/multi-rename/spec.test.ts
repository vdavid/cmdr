import { describe, it, expect } from 'vitest'
import { BUILT_IN_PRESETS, DEFAULT_SPEC, countPreview, insertAtCaret } from './spec'
import type { PreviewRow } from '$lib/tauri-commands'

const row = (type: PreviewRow['status']['type']): PreviewRow =>
  ({ row: 0, oldName: 'a', newName: 'b', status: { type } }) as PreviewRow

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

  it('counts ready, unchanged, and problem rows', () => {
    expect(countPreview([row('ready'), row('ready'), row('unchanged'), row('duplicate'), row('targetExists')])).toEqual(
      {
        ready: 2,
        unchanged: 1,
        problems: 2,
      },
    )
  })

  it('inserts a placeholder at the caret, or at the end', () => {
    expect(insertAtCaret('IMG ', '[C]', 4)).toEqual({ mask: 'IMG [C]', caret: 7 })
    expect(insertAtCaret('[N]', ' [C]', null)).toEqual({ mask: '[N] [C]', caret: 7 })
    expect(insertAtCaret('ab', 'X', 1)).toEqual({ mask: 'aXb', caret: 2 })
  })
})
