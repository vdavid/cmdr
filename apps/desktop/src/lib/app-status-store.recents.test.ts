import { describe, it, expect } from 'vitest'
import { dedupAndPrependRecent, RECENT_COMMANDS_LIMIT } from './app-status-store'

describe('dedupAndPrependRecent', () => {
  it('prepends a new ID to the front', () => {
    expect(dedupAndPrependRecent(['b', 'c'], 'a')).toEqual(['a', 'b', 'c'])
  })

  it('moves an existing ID to the front (LRU)', () => {
    expect(dedupAndPrependRecent(['a', 'b', 'c'], 'c')).toEqual(['c', 'a', 'b'])
  })

  it('keeps the list at most RECENT_COMMANDS_LIMIT entries', () => {
    const existing = Array.from({ length: RECENT_COMMANDS_LIMIT }, (_, i) => `cmd${String(i)}`)
    const next = dedupAndPrependRecent(existing, 'new')
    expect(next).toHaveLength(RECENT_COMMANDS_LIMIT)
    expect(next[0]).toBe('new')
    // The oldest entry was dropped.
    expect(next).not.toContain(`cmd${String(RECENT_COMMANDS_LIMIT - 1)}`)
  })

  it('starts a new list from an empty input', () => {
    expect(dedupAndPrependRecent([], 'a')).toEqual(['a'])
  })
})
