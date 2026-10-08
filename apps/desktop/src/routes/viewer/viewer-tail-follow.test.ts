import { describe, expect, it } from 'vitest'
import { isScrolledToEnd } from './viewer-tail-follow'

describe('isScrolledToEnd', () => {
  it('is true at the exact bottom', () => {
    expect(isScrolledToEnd({ scrollTop: 900, scrollHeight: 1000, clientHeight: 100 }, 20)).toBe(true)
  })

  it('forgives a sub-row gap, which fractional zoom leaves at the bottom', () => {
    expect(isScrolledToEnd({ scrollTop: 885.5, scrollHeight: 1000, clientHeight: 100 }, 20)).toBe(true)
  })

  it('is false once the user scrolled up by more than a row', () => {
    expect(isScrolledToEnd({ scrollTop: 870, scrollHeight: 1000, clientHeight: 100 }, 20)).toBe(false)
  })

  it('is true for content shorter than the viewport', () => {
    expect(isScrolledToEnd({ scrollTop: 0, scrollHeight: 80, clientHeight: 100 }, 20)).toBe(true)
  })
})
