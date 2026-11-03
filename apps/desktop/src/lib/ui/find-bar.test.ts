import { describe, expect, it } from 'vitest'
import { stepMatchIndex } from './find-bar'

describe('stepMatchIndex', () => {
  it('steps and wraps past both ends', () => {
    expect(stepMatchIndex(0, 3, 'next')).toBe(1)
    expect(stepMatchIndex(2, 3, 'next')).toBe(0)
    expect(stepMatchIndex(0, 3, 'previous')).toBe(2)
    expect(stepMatchIndex(1, 3, 'previous')).toBe(0)
  })

  it('lands on the first or the last when nothing is current', () => {
    expect(stepMatchIndex(-1, 3, 'next')).toBe(0)
    expect(stepMatchIndex(-1, 3, 'previous')).toBe(2)
  })

  it('is -1 with no matches', () => {
    expect(stepMatchIndex(0, 0, 'next')).toBe(-1)
  })
})
