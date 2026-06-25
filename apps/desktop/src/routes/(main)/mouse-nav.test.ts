import { describe, it, expect } from 'vitest'
import { navCommandForMouseButton } from './mouse-nav'

describe('navCommandForMouseButton', () => {
  it('maps the fourth button (X1) to nav.back', () => {
    expect(navCommandForMouseButton(3)).toBe('nav.back')
  })

  it('maps the fifth button (X2) to nav.forward', () => {
    expect(navCommandForMouseButton(4)).toBe('nav.forward')
  })

  it('returns null for the primary, middle, and secondary buttons', () => {
    expect(navCommandForMouseButton(0)).toBeNull()
    expect(navCommandForMouseButton(1)).toBeNull()
    expect(navCommandForMouseButton(2)).toBeNull()
  })

  it('returns null for any unknown higher button code', () => {
    expect(navCommandForMouseButton(5)).toBeNull()
    expect(navCommandForMouseButton(-1)).toBeNull()
  })
})
