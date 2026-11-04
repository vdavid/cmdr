import { afterEach, describe, it, expect } from 'vitest'
import { _setLocaleForTests } from '#lib/intl/locale.js'
import { formatFreePercent } from './free-percent'
import freePercentCases from './free-percent-cases.json'

describe('formatFreePercent', () => {
  afterEach(() => {
    _setLocaleForTests('en-US')
  })

  // The same table the backend's emit gate is tested against (`space_poller/readout.rs`).
  for (const c of freePercentCases.cases) {
    it(`${c.name}: ${String(c.percent)}%`, () => {
      _setLocaleForTests('en-US')
      expect(formatFreePercent(c.percent)).toBe(c.text)
      expect(formatFreePercent(c.percent, 1)).toBe(c.toastText)
    })
  }

  it('writes the decimal mark the way the locale does', () => {
    _setLocaleForTests('de-DE')
    expect(formatFreePercent(97.86)).toBe('97,8')
    expect(formatFreePercent(99.648)).toBe('99,64')
  })

  it('clamps out-of-range input to the two ends', () => {
    _setLocaleForTests('en-US')
    expect(formatFreePercent(120)).toBe('100')
    expect(formatFreePercent(-3)).toBe('0')
    expect(formatFreePercent(Number.NaN)).toBe('0')
  })
})
