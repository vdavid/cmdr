import { describe, it, expect, afterEach, vi } from 'vitest'

import { findWordBoundsAt } from './viewer-word'

/**
 * Replaces `Intl.Segmenter` with one that keeps the real boundaries but reports
 * `isWordLike` the way JavaScriptCore does: `false` for every segment ICU classifies as
 * numeric, which is any word segment ENDING in a digit. See `viewer-word.ts` for the
 * measurements. Vitest runs on Node (correct ICU flags), so this is the only way to keep
 * the app's real engine covered.
 */
function stubJavaScriptCoreSegmenter(): void {
  const Real = Intl.Segmenter
  class JscSegmenter extends Real {
    segment(input: string): Intl.Segments {
      const segments = super.segment(input)
      const patch = (seg: Intl.SegmentData): Intl.SegmentData => ({
        ...seg,
        isWordLike: seg.isWordLike === true && !/\d$/.test(seg.segment),
      })
      return {
        containing: (index?: number) => {
          const seg = segments.containing(index)
          return seg === undefined ? seg : patch(seg)
        },
        [Symbol.iterator]: function* () {
          for (const seg of segments) yield patch(seg)
        },
      } as Intl.Segments
    }
  }
  vi.stubGlobal('Intl', { ...Intl, Segmenter: JscSegmenter })
}

describe('findWordBoundsAt', () => {
  it('returns zero-length at offset 0 for an empty line', () => {
    expect(findWordBoundsAt('', 0)).toEqual({ start: 0, end: 0 })
  })

  it('caret inside a word returns that word', () => {
    expect(findWordBoundsAt('hello world', 2)).toEqual({ start: 0, end: 5 })
    expect(findWordBoundsAt('hello world', 4)).toEqual({ start: 0, end: 5 })
  })

  it('caret on a word boundary takes the word that starts there', () => {
    // Caret at offset 6 in "hello world": index 6 = 'w', start of "world".
    expect(findWordBoundsAt('hello world', 6)).toEqual({ start: 6, end: 11 })
  })

  it('caret on a separator (whitespace) returns the preceding word', () => {
    // Offset 5 = the space between "hello" and "world".
    expect(findWordBoundsAt('hello world', 5)).toEqual({ start: 0, end: 5 })
  })

  it('caret on punctuation returns the adjacent word', () => {
    // "foo, bar" — offset 3 lands on the comma. The preceding word "foo" wins.
    expect(findWordBoundsAt('foo, bar', 3)).toEqual({ start: 0, end: 3 })
  })

  it('caret on the leading separator returns the next word', () => {
    // " foo bar" with caret at index 0 (the leading space) returns "foo".
    expect(findWordBoundsAt(' foo bar', 0)).toEqual({ start: 1, end: 4 })
  })

  it('caret past the end returns the last word', () => {
    expect(findWordBoundsAt('hello', 99)).toEqual({ start: 0, end: 5 })
  })

  it('line of only separators returns zero-length at the caret', () => {
    expect(findWordBoundsAt('   ', 1)).toEqual({ start: 1, end: 1 })
  })

  it('underscores keep a snake-case identifier as one word (Unicode word boundary rule)', () => {
    // `Intl.Segmenter` treats `_` as part of the word for typical locales.
    expect(findWordBoundsAt('foo_bar baz', 4)).toEqual({ start: 0, end: 7 })
  })

  it('emoji in the line: caret inside a word past the emoji still returns just the word', () => {
    // "👋 hello" — offset 4 is inside "hello". The emoji is 2 UTF-16 units, space is 1 unit.
    expect(findWordBoundsAt('👋 hello', 4)).toEqual({ start: 3, end: 8 })
  })

  it('numbers are word-like', () => {
    expect(findWordBoundsAt('value=12345', 8)).toEqual({ start: 6, end: 11 })
  })

  it('a bare number in a JSON line is its own word', () => {
    // Offsets 13..29 are the digits of `"1292507278647433"`.
    expect(findWordBoundsAt('    "fbid": "1292507278647433"', 20)).toEqual({ start: 13, end: 29 })
  })

  it('an identifier ending in digits is one word', () => {
    expect(findWordBoundsAt('sha256 rocks', 3)).toEqual({ start: 0, end: 6 })
  })

  it('clamps negative offsets to 0', () => {
    expect(findWordBoundsAt('hello world', -5)).toEqual({ start: 0, end: 5 })
  })
})

describe('findWordBoundsAt on JavaScriptCore (the app’s real engine)', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('selects a bare number rather than the word before it', () => {
    stubJavaScriptCoreSegmenter()
    expect(findWordBoundsAt('    "fbid": "1292507278647433"', 20)).toEqual({ start: 13, end: 29 })
  })

  it('selects an identifier that ends in digits', () => {
    stubJavaScriptCoreSegmenter()
    expect(findWordBoundsAt('sha256 rocks', 3)).toEqual({ start: 0, end: 6 })
  })

  it('still returns the next word when the caret sits on a leading separator', () => {
    stubJavaScriptCoreSegmenter()
    expect(findWordBoundsAt(' 42 rocks', 0)).toEqual({ start: 1, end: 3 })
  })
})
