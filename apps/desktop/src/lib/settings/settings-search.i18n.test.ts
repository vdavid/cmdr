/**
 * Settings search in every shipped language, against the real catalogs: the index follows the UI
 * language, every script is searchable, accents are optional, English stays a fallback, and the
 * Language setting is findable from any language.
 */
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { setLocale, _clearCompiledCacheForTests, _setCatalogForTests } from '#lib/intl/messages.svelte.js'
import {
  clearSearchIndex,
  getMatchIndicesForLabel,
  getMatchingSections,
  getMatchingSettingIdsInSection,
  searchSettings,
} from './settings-search'

function ids(query: string): string[] {
  return searchSettings(query).map((r) => r.entry.id)
}

/** Every index `[start, end)` covers, the shape `getMatchIndicesForLabel` returns. */
function span(start: number, end: number): number[] {
  return Array.from({ length: end - start }, (_, i) => start + i)
}

beforeEach(() => {
  setLocale('en')
})

afterEach(() => {
  setLocale(null)
  _clearCompiledCacheForTests()
})

describe('the index follows the UI language', () => {
  it('rebuilds after a language switch, with highlights from the new text', () => {
    // Build the English index first, so a stale cache would answer the Hungarian queries.
    expect(ids('language')).toContain('appearance.language')

    setLocale('hu')
    expect(ids('nyelv')).toContain('appearance.language')
    expect(getMatchIndicesForLabel('nyelv', 'appearance.language')).toEqual(span(0, 5))

    setLocale('en')
    expect(getMatchIndicesForLabel('language', 'appearance.language')).toEqual(span(0, 8))
  })

  it('puts the translated section titles in the index', () => {
    setLocale('hu')
    expect(ids('megjelenés')).toContain('appearance.uiDensity')
    // The section path sits before the label, so the label highlight must not shift.
    expect(getMatchIndicesForLabel('sűrűsége', 'appearance.uiDensity')).toEqual(span(8, 16))
  })
})

describe('other scripts', () => {
  it.each([
    ['ru', 'язык'],
    ['zh', '语言'],
    ['zh-Hant', '語言'],
  ])('finds Language in %s by %s', (locale, query) => {
    setLocale(locale)
    expect(ids(query)).toContain('appearance.language')
  })

  it('allows no typo in a CJK query', () => {
    setLocale('zh')
    // The zh description says `选择 Cmdr 使用的语言`; one character off is a different phrase.
    expect(ids('使用的语言')).toContain('appearance.language')
    expect(ids('使用的语文')).toEqual([])
  })
})

describe('accents are optional', () => {
  it('finds a Hungarian label typed without accents, highlighting the accented letters', () => {
    setLocale('hu')
    expect(ids('tema')).toContain('theme.mode') // "Témamód"
    expect(ids('felulet')).toContain('appearance.uiDensity') // "Felület sűrűsége"
    expect(getMatchIndicesForLabel('felulet', 'appearance.uiDensity')).toEqual(span(0, 7))
  })

  it('finds a Vietnamese label typed without diacritics', () => {
    setLocale('vi')
    expect(ids('ngon ngu')).toContain('appearance.language') // "Ngôn ngữ"
    expect(getMatchIndicesForLabel('ngon ngu', 'appearance.language')).toEqual([...span(0, 4), ...span(5, 8)])
  })
})

describe('English as a fallback', () => {
  it('finds a row by its English label in another language, without a highlight', () => {
    setLocale('hu')
    expect(ids('density')).toContain('appearance.uiDensity')
    expect(getMatchIndicesForLabel('density', 'appearance.uiDensity')).toEqual([])
  })

  it('finds a row by its English keyword in another language', () => {
    setLocale('hu')
    expect(ids('locale')).toContain('appearance.language')
  })

  it('leaves the English description out, since long common-word sentences make short queries noisy', () => {
    expect(ids('compounds')).toContain('appearance.textSize')
    setLocale('hu')
    expect(ids('compounds')).not.toContain('appearance.textSize')
  })
})

describe('keywords from the catalog, terms from TS', () => {
  // A stand-in Hungarian catalog: the shipped one may not translate these keys yet, and the test
  // must not depend on what a translator wrote. Every key it lacks falls back to English.
  // The index is keyed by locale, and swapping a catalog's contents keeps the tag, so drop any
  // index built from the other catalog, on the way in and on the way out.
  beforeEach(() => {
    clearSearchIndex()
  })
  afterEach(() => {
    _setCatalogForTests('hu', null)
    clearSearchIndex()
  })

  function textOf(id: string, query: string): string {
    const hit = searchSettings(query).find((r) => r.entry.id === id)
    if (!hit) throw new Error(`${id} not found by "${query}"`)
    return hit.searchableText
  }

  /** Theme mode in Hungarian, so an English word in its text can only come from the keywords. */
  const huThemeMode = {
    'settings.theme.mode.label': 'Témamód',
    'settings.theme.mode.description': 'Válassz a világos, sötét vagy rendszer szerinti téma közül.',
  }

  it('finds a row by a keyword the UI language wrote for it', () => {
    _setCatalogForTests('hu', { ...huThemeMode, 'settings.theme.mode.keywords': 'éjszakai mód , kontraszt,,' })
    setLocale('hu')
    expect(ids('éjszakai')).toContain('theme.mode')
    expect(ids('kontraszt')).toContain('theme.mode')
    // Split on the commas, trimmed, and the empty ones dropped: no stray separators or gaps.
    expect(textOf('theme.mode', 'kontraszt')).toMatch(/ejszakai mod kontraszt \S/)
  })

  it('keeps the English keywords as a fallback', () => {
    _setCatalogForTests('hu', { ...huThemeMode, 'settings.theme.mode.keywords': 'éjszakai mód' })
    setLocale('hu')
    expect(ids('dark')).toContain('theme.mode')
  })

  it('indexes an untranslated keyword list once, through the fallback alone', () => {
    // A key the language hasn't translated resolves to English; indexing that as the
    // language's own keywords too would put every English word in twice.
    _setCatalogForTests('hu', huThemeMode)
    setLocale('hu')
    expect(textOf('theme.mode', 'témamód').match(/\bdark\b/g)).toHaveLength(1)
  })

  it.each(['en', 'hu', 'zh', 'ru'])('matches language-neutral terms in %s', (locale) => {
    setLocale(locale)
    expect(ids('openai')).toContain('ai.cloudProvider')
    expect(ids('ssh')).toContain('row:network.trustedHostKeys')
  })

  it('indexes the terms once, outside English too', () => {
    setLocale('hu')
    expect(textOf('ai.cloudProvider', 'openrouter').match(/\bopenrouter\b/g)).toHaveLength(1)
  })
})

describe('the Language setting from any language', () => {
  it.each(['språk', 'sprak', 'magyar', 'Русский', 'deutsch', 'idioma'])(
    'finds Language by %s with French selected',
    (query) => {
      setLocale('fr')
      expect(ids(query)).toContain('appearance.language')
    },
  )

  it('finds Language by another language name in English too', () => {
    expect(ids('nyelv')).toContain('appearance.language')
  })

  it('stays the only setting searched across languages', () => {
    setLocale('fr')
    // "Téma" is the Hungarian theme label; French search mustn't reach it.
    expect(ids('témamód')).not.toContain('theme.mode')
  })
})

describe('sections a hit lights up', () => {
  // These words once lit License and AI through a hardcoded list in `getMatchingSections`,
  // which lit the sidebar while the page itself showed nothing. Each must reach a real row.
  it.each([
    ['License', 'license'],
    ['License', 'activation'],
    ['License', 'purchase'],
    ['License', 'pricing'],
    ['AI', 'artificial intelligence'],
    ['AI', 'llama'],
    ['AI', 'ollama'],
    ['AI', 'anthropic'],
    ['AI', 'api key'],
  ])('lights %s for %s, and the page has a row to show', (section, query) => {
    expect(getMatchingSections(query)).toContain(section)
    expect(getMatchingSettingIdsInSection(query, [section]).size).toBeGreaterThan(0)
  })
})
