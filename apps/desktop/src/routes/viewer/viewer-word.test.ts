import { describe, it, expect, afterEach, vi } from 'vitest'

import { findWordBoundsAt, findWordEndAfter, findWordStartBefore } from './viewer-word'

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

// `foo, bar baz`: f0 o1 o2 ,3 ␣4 b5 a6 r7 ␣8 b9 a10 z11, length 12.
const PUNCTUATED = 'foo, bar baz'

describe('findWordEndAfter', () => {
  it('from inside a word returns that word’s end', () => {
    expect(findWordEndAfter(PUNCTUATED, 1)).toBe(3)
  })

  it('from a word’s end skips to the next word’s end', () => {
    expect(findWordEndAfter(PUNCTUATED, 3)).toBe(8)
  })

  it('from whitespace returns the following word’s end', () => {
    expect(findWordEndAfter(PUNCTUATED, 4)).toBe(8)
  })

  it('from punctuation skips it and returns the following word’s end', () => {
    // Offset 3 is the comma, and the `, ` run holds no word, so `bar` is the answer.
    expect(findWordEndAfter('foo, bar', 3)).toBe(8)
  })

  it('from the line start returns the first word’s end', () => {
    expect(findWordEndAfter(PUNCTUATED, 0)).toBe(3)
  })

  it('returns null at the line end', () => {
    expect(findWordEndAfter(PUNCTUATED, 12)).toBeNull()
  })

  it('returns null on an empty line', () => {
    expect(findWordEndAfter('', 0)).toBeNull()
  })

  it('returns null on a line of separators only', () => {
    expect(findWordEndAfter('   ...   ', 0)).toBeNull()
  })

  it('clamps a negative offset to the line start', () => {
    expect(findWordEndAfter(PUNCTUATED, -5)).toBe(3)
  })

  it('clamps an offset past the line end', () => {
    expect(findWordEndAfter(PUNCTUATED, 99)).toBeNull()
  })
})

describe('findWordStartBefore', () => {
  it('from inside a word returns that word’s start', () => {
    expect(findWordStartBefore(PUNCTUATED, 10)).toBe(9)
  })

  it('from a word’s start skips to the previous word’s start', () => {
    expect(findWordStartBefore(PUNCTUATED, 9)).toBe(5)
  })

  it('from whitespace returns the preceding word’s start', () => {
    expect(findWordStartBefore(PUNCTUATED, 4)).toBe(0)
  })

  it('from punctuation skips it and returns the preceding word’s start', () => {
    expect(findWordStartBefore(PUNCTUATED, 3)).toBe(0)
  })

  it('from the line end returns the last word’s start', () => {
    expect(findWordStartBefore(PUNCTUATED, 12)).toBe(9)
  })

  it('returns null at the line start', () => {
    expect(findWordStartBefore(PUNCTUATED, 0)).toBeNull()
  })

  it('returns null on an empty line', () => {
    expect(findWordStartBefore('', 0)).toBeNull()
  })

  it('returns null on a line of separators only', () => {
    expect(findWordStartBefore('   ...   ', 9)).toBeNull()
  })

  it('clamps an offset past the line end', () => {
    expect(findWordStartBefore(PUNCTUATED, 99)).toBe(9)
  })

  it('clamps a negative offset to the line start', () => {
    expect(findWordStartBefore(PUNCTUATED, -5)).toBeNull()
  })
})

describe('the directional walkers on JavaScriptCore (the app’s real engine)', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('walks forward onto an identifier ending in digits rather than past it', () => {
    stubJavaScriptCoreSegmenter()
    expect(findWordEndAfter('sha256 rocks', 0)).toBe(6)
  })

  it('walks backward onto an identifier ending in digits rather than past it', () => {
    stubJavaScriptCoreSegmenter()
    expect(findWordStartBefore('sha256 rocks', 6)).toBe(0)
  })

  it('walks forward onto a bare number rather than the closing quote past it', () => {
    stubJavaScriptCoreSegmenter()
    expect(findWordEndAfter('    "fbid": "1292507278647433"', 13)).toBe(29)
  })

  it('walks backward onto a bare number rather than the key before it', () => {
    stubJavaScriptCoreSegmenter()
    expect(findWordStartBefore('    "fbid": "1292507278647433"', 29)).toBe(13)
  })
})
