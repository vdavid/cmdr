import { describe, expect, it } from 'vitest'
import { shortenMiddle } from './shorten-middle'

/** Every character = 8px, including the ellipsis '…'. */
const mockMeasure = (text: string): number => text.length * 8

describe('shortenMiddle', () => {
  describe('returns text unchanged when it fits', () => {
    it('short text that fits', () => {
      expect(shortenMiddle('hello', 200, mockMeasure)).toBe('hello')
    })

    it('empty string', () => {
      expect(shortenMiddle('', 200, mockMeasure)).toBe('')
    })

    it('text exactly at maxWidth', () => {
      // 'hello' = 5 chars * 8px = 40px
      expect(shortenMiddle('hello', 40, mockMeasure)).toBe('hello')
    })
  })

  describe('truncates in the middle for plain text', () => {
    it('long text exceeding maxWidth contains ellipsis and preserves start/end', () => {
      const text = 'abcdefghijklmnop' // 16 chars = 128px
      const maxWidth = 80 // budget for 10 chars total
      const result = shortenMiddle(text, maxWidth, mockMeasure)

      expect(result).toContain('…')
      expect(text.startsWith(result.split('…')[0])).toBe(true)
      expect(text.endsWith(result.split('…')[1])).toBe(true)
    })

    it('result measured width does not exceed maxWidth', () => {
      const text = 'abcdefghijklmnopqrstuvwxyz' // 26 chars = 208px
      const maxWidth = 100
      const result = shortenMiddle(text, maxWidth, mockMeasure)

      expect(mockMeasure(result)).toBeLessThanOrEqual(maxWidth)
    })
  })

  describe('respects startRatio', () => {
    it('startRatio 0.7 keeps more from the start', () => {
      const text = 'abcdefghijklmnopqrstuvwxyz' // 26 chars = 208px
      const maxWidth = 100 // 12.5 char budget, minus 1 for ellipsis = 11.5 chars
      const result = shortenMiddle(text, maxWidth, mockMeasure, { startRatio: 0.7 })
      const [startPart, endPart] = result.split('…')

      expect(startPart.length).toBeGreaterThan(endPart.length)
    })

    it('startRatio 0.3 keeps more from the end', () => {
      const text = 'abcdefghijklmnopqrstuvwxyz'
      const maxWidth = 100
      const result = shortenMiddle(text, maxWidth, mockMeasure, { startRatio: 0.3 })
      const [startPart, endPart] = result.split('…')

      expect(endPart.length).toBeGreaterThan(startPart.length)
    })
  })

  describe('snaps to preferBreakAt character', () => {
    it('path with preferBreakAt "/" cuts at slash boundaries', () => {
      // '/aaa/bbb/ccc/ddd/eee/fff.txt' = 29 chars = 232px
      const text = '/aaa/bbb/ccc/ddd/eee/fff.txt'
      const maxWidth = 160 // 20 char budget, minus 1 for ellipsis = 19 chars
      const result = shortenMiddle(text, maxWidth, mockMeasure, { preferBreakAt: '/' })

      expect(result).toContain('…')
      expect(mockMeasure(result)).toBeLessThanOrEqual(maxWidth)

      // The start part should end at a '/' boundary (or be the full prefix up to one)
      const [startPart, endPart] = result.split('…')
      // Either the start ends with '/' or the end starts with '/' (snapped to boundary)
      const snappedToSlash = startPart.endsWith('/') || endPart.startsWith('/')
      expect(snappedToSlash).toBe(true)
    })

    it('text with no matching break char degrades to plain mid-split', () => {
      const text = 'abcdefghijklmnopqrstuvwxyz'
      const maxWidth = 100
      const result = shortenMiddle(text, maxWidth, mockMeasure, { preferBreakAt: '/' })

      expect(result).toContain('…')
      expect(mockMeasure(result)).toBeLessThanOrEqual(maxWidth)
    })

    it('path where snapping would waste >60% of budget falls back to raw cut', () => {
      // Break chars only near the very start — snapping would waste most of the start budget
      const text = '/a/bcdefghijklmnopqrstuvwxyz' // 28 chars = 224px
      const maxWidth = 120 // 15 char budget, minus 1 = 14 chars, start budget ~7 chars
      const result = shortenMiddle(text, maxWidth, mockMeasure, { preferBreakAt: '/' })

      expect(result).toContain('…')
      expect(mockMeasure(result)).toBeLessThanOrEqual(maxWidth)

      // The start part should NOT have snapped to '/' at position 2 because that would
      // waste too much of the ~7 char start budget (using only 2 of ~7 = 28%, well below 40%)
      const startPart = result.split('…')[0]
      expect(startPart.length).toBeGreaterThan(2)
    })
  })

  describe('handles edge cases', () => {
    it('text shorter than ellipsis width', () => {
      // 'a' = 8px, ellipsis = 8px, maxWidth = 4px (less than one char)
      expect(shortenMiddle('a', 4, mockMeasure)).toBe('a')
    })

    it('single character', () => {
      expect(shortenMiddle('x', 8, mockMeasure)).toBe('x')
    })

    it('all same character gives clean mid-split', () => {
      const text = 'aaaaaaaaaa' // 10 chars = 80px
      const maxWidth = 48 // 6 char budget, minus 1 = 5 chars
      const result = shortenMiddle(text, maxWidth, mockMeasure)

      expect(result).toContain('…')
      expect(mockMeasure(result)).toBeLessThanOrEqual(maxWidth)
      // Should still be composed of 'a's and ellipsis
      expect(result.replace('…', '')).toMatch(/^a+$/)
    })

    it('ellipsis character in input still works', () => {
      const text = 'abc…defghijklmnop' // has ellipsis in it
      const maxWidth = 80
      const result = shortenMiddle(text, maxWidth, mockMeasure)

      expect(mockMeasure(result)).toBeLessThanOrEqual(maxWidth)
    })
  })
})
