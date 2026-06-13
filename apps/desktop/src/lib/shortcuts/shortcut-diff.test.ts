import { describe, it, expect } from 'vitest'
import { diffShortcuts, isModifiedDiff } from './shortcut-diff'

describe('diffShortcuts', () => {
  it('marks every key active when effective matches the defaults', () => {
    expect(diffShortcuts(['⌘↑', 'Backspace'], ['⌘↑', 'Backspace'])).toEqual([
      { key: '⌘↑', status: 'active' },
      { key: 'Backspace', status: 'active' },
    ])
  })

  it('marks an extra user binding as added, keeping the default active', () => {
    expect(diffShortcuts(['⌘↑'], ['⌘↑', '⌘P'])).toEqual([
      { key: '⌘↑', status: 'active' },
      { key: '⌘P', status: 'added' },
    ])
  })

  it('marks a replaced binding as added + the original as disabled', () => {
    expect(diffShortcuts(['⌘↑'], ['⌘P'])).toEqual([
      { key: '⌘P', status: 'added' },
      { key: '⌘↑', status: 'disabled' },
    ])
  })

  it('marks a fully removed default as disabled', () => {
    expect(diffShortcuts(['⌘↑'], [])).toEqual([{ key: '⌘↑', status: 'disabled' }])
  })

  it('marks an added binding for a command that shipped with none', () => {
    expect(diffShortcuts([], ['⌘P'])).toEqual([{ key: '⌘P', status: 'added' }])
  })

  it('returns nothing when there are no shortcuts at all', () => {
    expect(diffShortcuts([], [])).toEqual([])
  })

  it('orders effective keys first, then disabled defaults in default order', () => {
    expect(diffShortcuts(['A', 'B', 'C'], ['C', 'X'])).toEqual([
      { key: 'C', status: 'active' },
      { key: 'X', status: 'added' },
      { key: 'A', status: 'disabled' },
      { key: 'B', status: 'disabled' },
    ])
  })

  it('dedupes a key repeated in the effective list', () => {
    expect(diffShortcuts(['A'], ['A', 'A'])).toEqual([{ key: 'A', status: 'active' }])
  })
})

describe('isModifiedDiff', () => {
  it('is false when all chips are active', () => {
    expect(isModifiedDiff(diffShortcuts(['⌘↑'], ['⌘↑']))).toBe(false)
  })

  it('is true when a binding was added', () => {
    expect(isModifiedDiff(diffShortcuts(['⌘↑'], ['⌘↑', '⌘P']))).toBe(true)
  })

  it('is true when a binding was disabled', () => {
    expect(isModifiedDiff(diffShortcuts(['⌘↑'], []))).toBe(true)
  })
})
