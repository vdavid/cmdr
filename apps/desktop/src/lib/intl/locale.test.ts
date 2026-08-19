import { afterEach, describe, expect, it } from 'vitest'

import { _setFormatLocaleForTests, _setLocaleForTests, getFormatLocale, getUiLocale } from './locale'

/** Looks like a locale tag (e.g. "en-US", "de", "sv-SE"). */
const LOCALE_TAG = /^[a-z]{2,3}(-[A-Za-z0-9]+)*$/

afterEach(() => {
  _setLocaleForTests(null)
  _setFormatLocaleForTests(null)
})

describe.each([
  ['getUiLocale', getUiLocale],
  ['getFormatLocale', getFormatLocale],
])('%s', (_name, read) => {
  it('returns a non-empty BCP 47 locale string by default', () => {
    const locale = read()
    expect(typeof locale).toBe('string')
    expect(locale.length).toBeGreaterThan(0)
    expect(locale).toMatch(LOCALE_TAG)
  })

  it('returns the same value the runtime Intl default resolves to', () => {
    expect(read()).toBe(new Intl.NumberFormat().resolvedOptions().locale)
  })

  it('honors a locale injected for tests', () => {
    _setLocaleForTests('de-DE')
    expect(read()).toBe('de-DE')
  })

  it('reverts to the runtime default when the test override is cleared', () => {
    _setLocaleForTests('de-DE')
    _setLocaleForTests(null)
    expect(read()).toBe(new Intl.NumberFormat().resolvedOptions().locale)
  })
})

describe('the two readers', () => {
  it('move apart when only the formatting half is pinned', () => {
    _setLocaleForTests('hu')
    _setFormatLocaleForTests('sv-SE')
    expect(getUiLocale()).toBe('hu')
    expect(getFormatLocale()).toBe('sv-SE')
  })

  it('leaves the UI half alone when the formatting override is cleared', () => {
    _setLocaleForTests('hu')
    _setFormatLocaleForTests('sv-SE')
    _setFormatLocaleForTests(null)
    expect(getUiLocale()).toBe('hu')
    expect(getFormatLocale()).toBe(new Intl.NumberFormat().resolvedOptions().locale)
  })
})
