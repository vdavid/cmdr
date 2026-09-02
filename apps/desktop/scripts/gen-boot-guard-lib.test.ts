/**
 * Unit tests for the boot guard's build-time data.
 *
 * The fixture catalogs are deliberately tiny: what's under test is the RESOLUTION
 * (fallback chain, ICU unescaping, script-boundary aliases, pruning), not the
 * copy. One test does read the real catalogs, because "every shipped locale
 * resolves all three keys" is the property that breaks when someone adds a locale
 * and forgets the guard exists.
 */
import { describe, it, expect } from 'vitest'
import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import {
  BOOT_GUARD_MARKER,
  buildBootGuardData,
  buildLocaleAliases,
  injectBootGuardData,
  resolutionChain,
} from './gen-boot-guard-lib.ts'
import { listLocales } from './i18n-catalog-lib.ts'
import { BOOT_GUARD_KEYS } from '../src/lib/utils/boot-guard-keys.ts'

/** Writes a throwaway `messages/` tree and returns its root. */
function fixtureRoot(locales: Record<string, Record<string, string>>): string {
  const root = mkdtempSync(join(tmpdir(), 'cmdr-boot-guard-'))
  for (const [locale, messages] of Object.entries(locales)) {
    mkdirSync(join(root, locale), { recursive: true })
    writeFileSync(join(root, locale, 'main.json'), JSON.stringify(messages))
  }
  return root
}

/** The three keys with distinguishable values, so a test can tell them apart. */
function catalogFor(marker: string): Record<string, string> {
  return {
    [BOOT_GUARD_KEYS.title]: `title ${marker}`,
    [BOOT_GUARD_KEYS.body]: `body ${marker}`,
    [BOOT_GUARD_KEYS.quit]: `quit ${marker}`,
  }
}

describe('resolutionChain', () => {
  it('walks the locale, then what it may inherit, then English', () => {
    expect(resolutionChain('en-GB', ['en', 'en-GB', 'zh', 'zh-Hant'])).toEqual(['en-GB', 'en'])
    expect(resolutionChain('de', ['de', 'en'])).toEqual(['de', 'en'])
  })

  it('never lets a Traditional reader fall through to Simplified', () => {
    expect(resolutionChain('zh-Hant', ['en', 'zh', 'zh-Hant'])).toEqual(['zh-Hant', 'en'])
  })
})

describe('buildBootGuardData', () => {
  it('formats ICU away, so an escaped apostrophe reaches the screen as one', () => {
    const root = fixtureRoot({
      en: { ...catalogFor('en'), [BOOT_GUARD_KEYS.body]: "Cmdr''s interface is too new" },
    })
    const data = buildBootGuardData({ locales: ['en'], messagesRoot: root })
    expect(data.strings.en.body).toBe("Cmdr's interface is too new")
  })

  it('gives a full translation its own strings', () => {
    const root = fixtureRoot({ en: catalogFor('en'), de: catalogFor('de') })
    const data = buildBootGuardData({ locales: ['en', 'de'], messagesRoot: root })
    expect(data.strings.de.title).toBe('title de')
    expect(data.aliases.de).toBe('de')
  })

  it('drops an overlay that hasn\'t forked this copy, so the tag falls through to its base', () => {
    const root = fixtureRoot({ en: catalogFor('en'), 'en-GB': {} })
    const data = buildBootGuardData({ locales: ['en', 'en-GB'], messagesRoot: root })
    expect(Object.keys(data.strings)).toEqual(['en'])
    expect(data.aliases['en-gb']).toBeUndefined()
  })

  it('keeps an overlay that HAS forked this copy', () => {
    const root = fixtureRoot({ en: catalogFor('en'), 'en-GB': catalogFor('en-GB') })
    const data = buildBootGuardData({ locales: ['en', 'en-GB'], messagesRoot: root })
    expect(data.strings['en-GB'].title).toBe('title en-GB')
    expect(data.aliases['en-gb']).toBe('en-GB')
  })

  it('excludes the generated pseudolocale', () => {
    const root = fixtureRoot({ en: catalogFor('en'), 'en-XA': catalogFor('en-XA') })
    const data = buildBootGuardData({ locales: ['en', 'en-XA'], messagesRoot: root })
    expect(data.strings['en-XA']).toBeUndefined()
  })

  it('says which key is missing rather than shipping a blank screen', () => {
    const root = fixtureRoot({ en: { [BOOT_GUARD_KEYS.title]: 'only a title' } })
    expect(() => buildBootGuardData({ locales: ['en'], messagesRoot: root })).toThrow(
      new RegExp(BOOT_GUARD_KEYS.body),
    )
  })

  it('carries the forced block only when asked', () => {
    const root = fixtureRoot({ en: catalogFor('en') })
    expect(buildBootGuardData({ locales: ['en'], messagesRoot: root }).force).toBe(false)
    expect(buildBootGuardData({ locales: ['en'], messagesRoot: root, force: true }).force).toBe(true)
  })
})

describe('buildLocaleAliases', () => {
  const locales = ['de', 'en', 'en-GB', 'zh', 'zh-Hant']

  it('sends a Traditional-script region to the Traditional catalog', () => {
    const aliases = buildLocaleAliases(locales)
    // ❗ The whole reason this map exists. Dropping `-TW` and matching `zh` would
    // hand a Traditional reader Simplified text, which they can't read.
    expect(aliases['zh-tw']).toBe('zh-Hant')
    expect(aliases['zh-hk']).toBe('zh-Hant')
    expect(aliases['zh-hant']).toBe('zh-Hant')
  })

  it('leaves a Simplified region to fall through to the base catalog', () => {
    const aliases = buildLocaleAliases(locales)
    expect(aliases['zh-cn']).toBe('zh')
    expect(aliases.zh).toBe('zh')
  })

  it('keys everything lowercase, since the guard lowercases before it looks', () => {
    const aliases = buildLocaleAliases(locales)
    expect(Object.keys(aliases).every((key) => key === key.toLowerCase())).toBe(true)
  })
})

describe('injectBootGuardData', () => {
  const data = { strings: { en: { title: 't', body: 'b', quit: 'q' } }, aliases: { en: 'en' }, force: false }

  it('replaces the marker with the payload', () => {
    const out = injectBootGuardData(`var DATA = ${BOOT_GUARD_MARKER}\n`, data)
    expect(out).not.toContain(BOOT_GUARD_MARKER)
    expect(out).toContain('"quit":"q"')
  })

  it('fails the build when the marker is gone, instead of shipping a guard with no strings', () => {
    expect(() => injectBootGuardData('var DATA = null\n', data)).toThrow(/boot-guard marker/)
  })
})

describe('the real catalogs', () => {
  it('resolves all three keys for every shipped locale', () => {
    const data = buildBootGuardData({ locales: listLocales() })
    expect(Object.keys(data.strings).length).toBeGreaterThan(1)
    for (const [tag, strings] of Object.entries(data.strings)) {
      for (const [part, value] of Object.entries(strings)) {
        expect(value, `${tag}.${part}`).not.toBe('')
        // ICU escaping is undone at build time, so nothing doubled survives.
        expect(value, `${tag}.${part}`).not.toContain("''")
      }
    }
  })

  it('routes every shipped tag to a strings block that exists', () => {
    const data = buildBootGuardData({ locales: listLocales() })
    for (const tag of Object.values(data.aliases)) {
      expect(data.strings[tag], tag).toBeDefined()
    }
  })
})
