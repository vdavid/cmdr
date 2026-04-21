import { describe, it, expect } from 'vitest'
import { isInvalidSelfDescendantDrop } from './drop-target-validation'

describe('isInvalidSelfDescendantDrop', () => {
  it('flags drop onto the source itself', () => {
    expect(isInvalidSelfDescendantDrop('/a/b', ['/a/b'])).toBe(true)
  })

  it('flags drop into a descendant of the source', () => {
    expect(isInvalidSelfDescendantDrop('/a/b/c', ['/a/b'])).toBe(true)
  })

  it('flags drop into a deeply-nested descendant', () => {
    expect(isInvalidSelfDescendantDrop('/a/b/c/d/e', ['/a/b'])).toBe(true)
  })

  it('allows drop into an ancestor', () => {
    expect(isInvalidSelfDescendantDrop('/a', ['/a/b'])).toBe(false)
  })

  it('allows drop into a sibling', () => {
    expect(isInvalidSelfDescendantDrop('/a/c', ['/a/b'])).toBe(false)
  })

  it('does not treat a name-prefix match as a descendant', () => {
    // "/a/bc" starts with "/a/b" textually but is not a child of "/a/b"
    expect(isInvalidSelfDescendantDrop('/a/bc', ['/a/b'])).toBe(false)
  })

  it('flags the first matching source in a multi-source drag', () => {
    expect(isInvalidSelfDescendantDrop('/a/b/c', ['/x/y', '/a/b'])).toBe(true)
  })

  it('returns false when no sources are given', () => {
    expect(isInvalidSelfDescendantDrop('/a/b', [])).toBe(false)
  })
})
