import { describe, it, expect } from 'vitest'
import { applySizeFromAi, applyDateFromAi } from './apply-ai-filters'
import { createQueryFilterState } from './query-filter-state.svelte'

describe('applySizeFromAi', () => {
  it('returns false and touches nothing when both bounds are null', () => {
    const state = createQueryFilterState({ defaultMode: 'filename' })
    expect(applySizeFromAi(state, null, null)).toBe(false)
    expect(state.getSizeFilter()).toBe('any')
  })

  it('sets a gte filter from a min-only bound', () => {
    const state = createQueryFilterState({ defaultMode: 'filename' })
    expect(applySizeFromAi(state, 1024 * 1024, null)).toBe(true)
    expect(state.getSizeFilter()).toBe('gte')
    expect(state.getSizeValue()).toBe('1')
    expect(state.getSizeUnit()).toBe('MB')
  })

  it('sets an lte filter from a max-only bound', () => {
    const state = createQueryFilterState({ defaultMode: 'filename' })
    expect(applySizeFromAi(state, null, 5 * 1024 * 1024)).toBe(true)
    expect(state.getSizeFilter()).toBe('lte')
    expect(state.getSizeValue()).toBe('5')
    expect(state.getSizeUnit()).toBe('MB')
  })

  it('sets a between filter from both bounds', () => {
    const state = createQueryFilterState({ defaultMode: 'filename' })
    expect(applySizeFromAi(state, 1024, 1024 * 1024 * 1024)).toBe(true)
    expect(state.getSizeFilter()).toBe('between')
    expect(state.getSizeValue()).toBe('1')
    expect(state.getSizeUnit()).toBe('KB')
    expect(state.getSizeValueMax()).toBe('1')
    expect(state.getSizeUnitMax()).toBe('GB')
  })

  // When the AI returns min == max, that's an exact-size match: set the `eq` comparator so
  // the chip reads "= N" rather than "between N and N".
  it('sets an eq filter when min == max (so "size = 5 MB" reads as = not between)', () => {
    const state = createQueryFilterState({ defaultMode: 'filename' })
    expect(applySizeFromAi(state, 5 * 1024 * 1024, 5 * 1024 * 1024)).toBe(true)
    expect(state.getSizeFilter()).toBe('eq')
    expect(state.getSizeValue()).toBe('5')
    expect(state.getSizeUnit()).toBe('MB')
  })

  it('sets an eq 0 B filter when the AI returns min == max == 0 (find empty files)', () => {
    const state = createQueryFilterState({ defaultMode: 'filename' })
    expect(applySizeFromAi(state, 0, 0)).toBe(true)
    expect(state.getSizeFilter()).toBe('eq')
    expect(state.getSizeValue()).toBe('0')
    expect(state.getSizeUnit()).toBe('B')
  })
})

describe('applyDateFromAi', () => {
  it('returns false and touches nothing when both bounds are null', () => {
    const state = createQueryFilterState({ defaultMode: 'filename' })
    expect(applyDateFromAi(state, null, null)).toBe(false)
    expect(state.getDateFilter()).toBe('any')
  })

  it('sets an after filter from an after-only bound', () => {
    const state = createQueryFilterState({ defaultMode: 'filename' })
    expect(applyDateFromAi(state, '2026-01-01', null)).toBe(true)
    expect(state.getDateFilter()).toBe('after')
    expect(state.getDateValue()).toBe('2026-01-01')
  })

  it('sets a before filter from a before-only bound', () => {
    const state = createQueryFilterState({ defaultMode: 'filename' })
    expect(applyDateFromAi(state, null, '2026-05-01')).toBe(true)
    expect(state.getDateFilter()).toBe('before')
    expect(state.getDateValue()).toBe('2026-05-01')
  })

  it('sets a between filter from both bounds', () => {
    const state = createQueryFilterState({ defaultMode: 'filename' })
    expect(applyDateFromAi(state, '2026-01-01', '2026-05-01')).toBe(true)
    expect(state.getDateFilter()).toBe('between')
    expect(state.getDateValue()).toBe('2026-01-01')
    expect(state.getDateValueMax()).toBe('2026-05-01')
  })
})
