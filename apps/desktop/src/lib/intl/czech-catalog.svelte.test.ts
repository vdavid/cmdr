/** Integration coverage for the shipped Czech catalog and its count-sensitive grammar. */
import { afterEach, describe, expect, it } from 'vitest'
import {
  _clearCompiledCacheForTests,
  availableLocales,
  getMessage,
  resolvedCatalogLocale,
  setLocale,
  tString,
} from './messages.svelte'
import { _setLocaleForTests } from './locale'

/** Restore the shared locale, compiled messages, and document language after each test. */
afterEach(() => {
  void setLocale(null)
  _setLocaleForTests(null)
  _clearCompiledCacheForTests()
  document.documentElement.lang = 'en'
})

/** Exercise production catalog discovery and formatting without injecting a test catalog. */
describe('the shipped Czech catalog', () => {
  /** A regional OS language must discover Czech and localize both HTML and native menus. */
  it('resolves cs-CZ to Czech and exposes translated raw menu labels', () => {
    void setLocale('cs-CZ')
    expect(availableLocales()).toContain('cs')
    expect(resolvedCatalogLocale('cs-CZ')).toBe('cs')
    expect(document.documentElement.lang).toBe('cs')
    expect(getMessage('menu.bar.file')).toBe('Soubor')
    expect(getMessage('menu.bar.edit')).toBe('Úpravy')
    expect(getMessage('menu.bar.help')).toBe('Nápověda')
  })

  /** Czech `one` is 1, `few` 2–4, `many` only decimals, and `other` 0 and 5+ (21 and 101 included). */
  it('renders the real file counter across Czech plural boundaries', () => {
    void setLocale('cs')
    const cases: [number, string][] = [
      [0, 'souborů'],
      [1, 'soubor'],
      [2, 'soubory'],
      [4, 'soubory'],
      [5, 'souborů'],
      [21, 'souborů'],
      [22, 'souborů'],
      [101, 'souborů'],
    ]
    for (const [count, noun] of cases) {
      expect(tString('transfer.delete', { count, countText: String(count) })).toBe(
        `Mazání dokončeno: ${String(count)} ${noun}`,
      )
    }
  })

  /** Only exactly one operation uses the countless wording; 2–4 and 5+ show their number in the right case. */
  it('distinguishes one operation from a few and from many', () => {
    void setLocale('cs')
    expect(tString('main.quit.title', { count: 1, countText: '1' })).toBe('Ukončit, i\u00a0když ještě běží operace?')
    expect(tString('main.quit.title', { count: 3, countText: '3' })).toBe('Ukončit, i\u00a0když ještě běží 3 operace?')
    expect(tString('main.quit.title', { count: 21, countText: '21' })).toBe(
      'Ukončit, i\u00a0když ještě běží 21 operací?',
    )
  })
})
