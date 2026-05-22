import { describe, it, expect } from 'vitest'

import { findWordBoundsAt } from './viewer-word'

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

  it('clamps negative offsets to 0', () => {
    expect(findWordBoundsAt('hello world', -5)).toEqual({ start: 0, end: 5 })
  })
})
