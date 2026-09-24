import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { _setLocaleForTests } from './locale'
import { _clearListFormatCacheForTests, formatConjunctionList } from './list-format'

describe('formatConjunctionList', () => {
  afterEach(() => {
    _setLocaleForTests(null)
  })

  it('reads as one name, two joined by "and", and an Oxford-comma list from three', () => {
    _setLocaleForTests('en-US')
    expect(formatConjunctionList([])).toBe('')
    expect(formatConjunctionList(['Preview'])).toBe('Preview')
    expect(formatConjunctionList(['Preview', 'Warp'])).toBe('Preview and Warp')
    expect(formatConjunctionList(['Preview', 'Warp', 'Photos'])).toBe('Preview, Warp, and Photos')
  })

  it('joins in the UI language, so a German toast never reads an English "and"', () => {
    _setLocaleForTests('de-DE')
    expect(formatConjunctionList(['Preview', 'Warp'])).toBe('Preview und Warp')
  })

  it('spaces a Chinese conjunction off the Latin names beside it, in both scripts', () => {
    // CLDR joins tight (`Warp和其他 App`); both Chinese style guides space Han
    // against Latin. The enumeration comma is full-width punctuation: no space.
    _setLocaleForTests('zh-Hant')
    expect(formatConjunctionList(['Preview', 'Warp', 'Photos', '其他 App'])).toBe('Preview、Warp、Photos 和其他 App')
    _setLocaleForTests('zh')
    expect(formatConjunctionList(['Preview', 'Warp'])).toBe('Preview 和 Warp')
    expect(formatConjunctionList(['预览', 'Warp'])).toBe('预览和 Warp')
  })

  it('leaves Japanese tight, since it never spaces kana against Latin', () => {
    _setLocaleForTests('ja')
    expect(formatConjunctionList(['Preview', 'Warp'])).toBe(
      new Intl.ListFormat('ja', { type: 'conjunction' }).format(['Preview', 'Warp']),
    )
  })
})

describe('formatConjunctionList (memoization)', () => {
  beforeEach(() => {
    _clearListFormatCacheForTests()
  })

  afterEach(() => {
    _setLocaleForTests(null)
    vi.unstubAllGlobals()
  })

  it('reuses one Intl.ListFormat instance for a locale, and builds a fresh one when the locale changes', () => {
    // A stand-in constructor swapped in wholesale, rather than a spy on the real
    // one: a spied constructor hands back an instance carrying the SPY's
    // prototype, so the stand-in's own `format` never reaches the caller.
    const builtFor: string[] = []
    class CountingListFormat {
      constructor(locale: string) {
        builtFor.push(locale)
      }
      format(items: string[]): string {
        return items.join(' + ')
      }
    }
    vi.stubGlobal('Intl', { ...Intl, ListFormat: CountingListFormat })

    _setLocaleForTests('en-US')
    for (let i = 0; i < 20; i++) formatConjunctionList(['Preview', 'Warp'])
    expect(builtFor).toEqual(['en-US'])

    _setLocaleForTests('de-DE')
    formatConjunctionList(['Preview', 'Warp'])
    expect(builtFor).toEqual(['en-US', 'de-DE'])
  })
})
