import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { _setLocaleForTests } from './locale'
import { _clearCachesForTests, formatInteger, getGroupSeparator, getNumberFormatter } from './number-format'

describe('formatInteger', () => {
  afterEach(() => {
    _setLocaleForTests(null)
  })

  it('groups thousands with the en-US comma by default-equivalent locale', () => {
    _setLocaleForTests('en-US')
    expect(formatInteger(0)).toBe('0')
    expect(formatInteger(999)).toBe('999')
    expect(formatInteger(1000)).toBe('1,000')
    expect(formatInteger(1234567)).toBe('1,234,567')
  })

  it('switches grouping to the active locale', () => {
    _setLocaleForTests('de-DE')
    // German groups with a period.
    expect(formatInteger(1234567)).toBe('1.234.567')
  })
})

describe('getGroupSeparator', () => {
  afterEach(() => {
    _setLocaleForTests(null)
  })

  it('is a comma for en-US', () => {
    _setLocaleForTests('en-US')
    expect(getGroupSeparator()).toBe(',')
  })

  it('is a period for de-DE', () => {
    _setLocaleForTests('de-DE')
    expect(getGroupSeparator()).toBe('.')
  })
})

describe('getNumberFormatter (memoization)', () => {
  beforeEach(() => {
    _clearCachesForTests()
  })

  afterEach(() => {
    _setLocaleForTests(null)
    vi.restoreAllMocks()
  })

  it('reuses one Intl.NumberFormat instance across many calls for the same (locale, options)', () => {
    _setLocaleForTests('en-US')
    const spy = vi.spyOn(Intl, 'NumberFormat')
    const opts = { minimumFractionDigits: 2, maximumFractionDigits: 2, useGrouping: false } as const
    for (let i = 0; i < 50; i++) getNumberFormatter(opts)
    expect(spy).toHaveBeenCalledTimes(1)
  })

  it('builds a fresh instance when the locale changes', () => {
    const spy = vi.spyOn(Intl, 'NumberFormat')
    const opts = { maximumFractionDigits: 0 } as const
    _setLocaleForTests('en-US')
    getNumberFormatter(opts)
    getNumberFormatter(opts)
    _setLocaleForTests('de-DE')
    getNumberFormatter(opts)
    expect(spy).toHaveBeenCalledTimes(2)
  })

  it('builds separate instances for different option sets', () => {
    _setLocaleForTests('en-US')
    const spy = vi.spyOn(Intl, 'NumberFormat')
    getNumberFormatter({ maximumFractionDigits: 0 })
    getNumberFormatter({ minimumFractionDigits: 2, maximumFractionDigits: 2 })
    getNumberFormatter({ maximumFractionDigits: 0 }) // reuse of the first
    expect(spy).toHaveBeenCalledTimes(2)
  })
})
