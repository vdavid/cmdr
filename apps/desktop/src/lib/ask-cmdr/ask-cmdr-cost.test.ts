/**
 * Unit tests for the pure cost-formatting helpers (the per-thread footer + settings spend).
 * `formatUsdMicros` reads the locale via the intl layer; these assert en-US output.
 */

import { describe, it, expect, beforeAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { formatUsdMicros, isLocalOnly, totalTokens } from './ask-cmdr-cost'

beforeAll(() => {
  _setLocaleForTests('en-US')
})

describe('totalTokens', () => {
  it('sums prompt and completion tokens', () => {
    expect(totalTokens({ promptTokens: 300, completionTokens: 70 })).toBe(370)
  })
})

describe('formatUsdMicros', () => {
  it('shows up to four fraction digits for sub-dollar amounts (never rounds a real cost to $0.00)', () => {
    // 1,200 micro-USD = $0.0012 (four digits, exact).
    expect(formatUsdMicros(1_200)).toBe('$0.0012')
    // A tiny real cost still shows a non-zero amount, never a misleading $0.00.
    expect(formatUsdMicros(400)).toBe('$0.0004')
  })

  it('rounds to cents for a dollar or more', () => {
    // 1,230,000 micro-USD = $1.23.
    expect(formatUsdMicros(1_230_000)).toBe('$1.23')
  })

  it('formats zero as $0.00', () => {
    expect(formatUsdMicros(0)).toBe('$0.00')
  })
})

describe('isLocalOnly', () => {
  it('is true only when every provider is local', () => {
    expect(isLocalOnly(['local'])).toBe(true)
    expect(isLocalOnly(['local', 'local'])).toBe(true)
  })

  it('is false when any provider is a cloud one, or the list is empty', () => {
    expect(isLocalOnly(['local', 'openai'])).toBe(false)
    expect(isLocalOnly(['anthropic'])).toBe(false)
    expect(isLocalOnly([])).toBe(false)
  })
})
