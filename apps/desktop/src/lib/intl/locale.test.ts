import { afterEach, describe, expect, it } from 'vitest'

import { _setLocaleForTests, getLocale } from './locale'

describe('getLocale', () => {
  afterEach(() => {
    _setLocaleForTests(null)
  })

  it('returns a non-empty BCP 47 locale string by default', () => {
    const locale = getLocale()
    expect(typeof locale).toBe('string')
    expect(locale.length).toBeGreaterThan(0)
    // Looks like a locale tag (e.g. "en-US", "de", "sv-SE").
    expect(locale).toMatch(/^[a-z]{2,3}(-[A-Za-z0-9]+)*$/)
  })

  it('returns the same value the runtime Intl default resolves to', () => {
    expect(getLocale()).toBe(new Intl.NumberFormat().resolvedOptions().locale)
  })

  it('honors a locale injected for tests', () => {
    _setLocaleForTests('de-DE')
    expect(getLocale()).toBe('de-DE')
  })

  it('reverts to the runtime default when the test override is cleared', () => {
    _setLocaleForTests('de-DE')
    _setLocaleForTests(null)
    expect(getLocale()).toBe(new Intl.NumberFormat().resolvedOptions().locale)
  })
})
