import { describe, it, expect } from 'vitest'
import { parseOcrSnippet } from './ocr-snippet'

describe('parseOcrSnippet', () => {
  it('returns an empty array for an empty string', () => {
    expect(parseOcrSnippet('')).toEqual([])
  })

  it('returns a single plain segment when there are no markers', () => {
    expect(parseOcrSnippet('plain text')).toEqual([{ text: 'plain text', matched: false }])
  })

  it('marks a bracketed run as matched', () => {
    expect(parseOcrSnippet('the [invoice] total')).toEqual([
      { text: 'the ', matched: false },
      { text: 'invoice', matched: true },
      { text: ' total', matched: false },
    ])
  })

  it('handles multiple matched runs', () => {
    expect(parseOcrSnippet('[beach] at [sunset]')).toEqual([
      { text: 'beach', matched: true },
      { text: ' at ', matched: false },
      { text: 'sunset', matched: true },
    ])
  })

  it('keeps the text of an unbalanced trailing open marker visible', () => {
    // A missing closing ']' must not drop the remaining text.
    expect(parseOcrSnippet('start [never closed')).toEqual([
      { text: 'start ', matched: false },
      { text: 'never closed', matched: true },
    ])
  })

  it('does not treat a closing bracket outside a match as a marker', () => {
    expect(parseOcrSnippet('a] b')).toEqual([{ text: 'a] b', matched: false }])
  })
})
