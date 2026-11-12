import { describe, it, expect } from 'vitest'
import { fitBreadcrumbSegments, middleTruncate } from './breadcrumb-fit'

/** Monospace stand-in: every character, `…` included, is 10px wide. */
const measure = (text: string): number => Array.from(text).length * 10

function fit(texts: string[], availableWidth: number): string[] {
  return fitBreadcrumbSegments({ texts, availableWidth, separatorWidth: 10, measure })
}

describe('middleTruncate', () => {
  it('returns the text unchanged when it fits', () => {
    expect(middleTruncate('Documents', 90, measure)).toBe('Documents')
  })

  it('keeps the start and the end around a middle ellipsis', () => {
    expect(middleTruncate('abcdefghij', 50, measure)).toBe('ab…ij')
  })

  it('gives the odd character to the start', () => {
    expect(middleTruncate('abcdefghij', 60, measure)).toBe('abc…ij')
  })

  it('falls back to a lone ellipsis when not even one character each side fits', () => {
    expect(middleTruncate('abcdefghij', 20, measure)).toBe('…')
    expect(middleTruncate('abcdefghij', 0, measure)).toBe('…')
  })

  it('never splits a surrogate pair', () => {
    expect(middleTruncate('😀😀😀😀😀😀', 30, measure)).toBe('😀…😀')
  })
})

describe('fitBreadcrumbSegments', () => {
  it('leaves a path that fits untouched', () => {
    // "/private/tmp" = 1 + 7 + 1 + 3 chars.
    expect(fit(['', 'private', 'tmp'], 120)).toEqual(['', 'private', 'tmp'])
  })

  it('middle-truncates the one long segment that does not fit, keeping every other segment whole', () => {
    // "/private/tmp/claude-501/-Users-test_user_3-projects/scratchpad": the long one gives way.
    const texts = ['', 'private', 'tmp', 'claude-501', '-Users-test_user_3-projects', 'scratchpad']
    const fitted = fit(texts, 500)
    expect(fitted.slice(0, 4)).toEqual(['', 'private', 'tmp', 'claude-501'])
    expect(fitted[5]).toBe('scratchpad')
    expect(fitted[4]).toBe('-Users-…rojects')
    // Five separators (50px) plus the texts fill the width without passing it.
    expect(fitted.reduce((sum, t) => sum + measure(t), 0) + 50).toBeLessThanOrEqual(500)
  })

  it('shrinks the widest segments to one shared cap when several are long', () => {
    // "~/aaaaaaaaaaaaaaaaaaaa/bbbbbbbbbbbbbbbbbbbb/c": 1 + 20 + 20 + 1 chars, three separators.
    const fitted = fit(['~', 'a'.repeat(20), 'b'.repeat(20), 'c'], 250)
    expect(fitted).toEqual(['~', 'aaaaa…aaaa', 'bbbbb…bbbb', 'c'])
  })

  it('truncates the current folder too when it is the long one', () => {
    expect(fit(['', 'Users', 'a-very-long-folder-name'], 170)).toEqual(['', 'Users', 'a-ver…name'])
  })

  it('reserves a short segment its floor, so a path of one-letter folders never clips', () => {
    const texts = ['a', 'b', 'c', 'dddddddddd']
    // Text alone is 13 chars + 3 separators = 160px, but the floors add 3 × (20 − 10).
    expect(fitBreadcrumbSegments({ texts, availableWidth: 160, separatorWidth: 10, measure, floor: () => 20 })).toEqual(
      ['a', 'b', 'c', 'ddd…ddd'],
    )
    expect(fitBreadcrumbSegments({ texts, availableWidth: 190, separatorWidth: 10, measure, floor: () => 20 })).toEqual(
      texts,
    )
  })

  it('gives the root marker no floor: it renders no box', () => {
    expect(
      fitBreadcrumbSegments({ texts: ['', 'abc'], availableWidth: 40, separatorWidth: 10, measure, floor: () => 20 }),
    ).toEqual(['', 'abc'])
  })

  it('counts no separator before the root marker or the first segment', () => {
    // "~/abcdefghij" is exactly 120px: one separator, not two.
    expect(fit(['~', 'abcdefghij'], 120)).toEqual(['~', 'abcdefghij'])
    // "/abcdefghij" is 110px: the root marker's slash is the only separator.
    expect(fit(['', 'abcdefghij'], 110)).toEqual(['', 'abcdefghij'])
  })

  it('shows lone ellipses when even that overflows, leaving the clip to the container', () => {
    expect(fit(['', 'abcdefghij', 'klmnopqrst'], 30)).toEqual(['', '…', '…'])
  })

  it('returns the texts as they are before the width is known', () => {
    expect(fit(['', 'private', 'tmp'], 0)).toEqual(['', 'private', 'tmp'])
  })
})
