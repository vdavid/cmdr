/**
 * TDD for the shipped-locale codegen (`gen-shipped-locales-lib.ts`): catalog dir
 * names → the Rust table the locale resolver reads for its script guard.
 *
 * The script facts come from Node's CLDR data via `Intl.Locale().maximize()`,
 * so these tests assert the SHAPE of the answer and the two facts we depend on
 * (Chinese splits by script, Latin-script languages don't), never a full region
 * list, which drifts with every ICU update.
 */
import { describe, it, expect } from 'vitest'
import {
  buildEntry,
  buildParentLocales,
  buildShippedLocales,
  emitRustModule,
  PSEUDO_LOCALE,
} from './gen-shipped-locales-lib.ts'

/** A slice of CLDR's shape, small enough to assert against in full. */
const PARENT_LOCALE_FIXTURE = {
  'en-NZ': 'en-001',
  'en-AT': 'en-150',
  'en-150': 'en-001',
  'zh-Hant': 'und',
  'ca-FR': 'ca-ES',
}

describe('buildEntry', () => {
  it('reads the likely script of a Latin-script language and finds no splits', () => {
    const entry = buildEntry('hu')
    expect(entry).toMatchObject({ tag: 'hu', script: 'latn', defaultScript: 'latn' })
    expect(entry.regionScripts).toEqual([])
  })

  it('marks the Traditional-Chinese regions as differing from the `zh` default', () => {
    const entry = buildEntry('zh')
    expect(entry.script).toBe('hans')
    const regions = entry.regionScripts.filter((r) => r.script === 'hant').map((r) => r.region)
    // The three the roster doc calls out; CLDR lists more (overseas communities).
    expect(regions).toEqual(expect.arrayContaining(['tw', 'hk', 'mo']))
    // The Simplified heartland must NOT be listed, or `zh-CN` would be blocked
    // from the very catalog it belongs to.
    expect(regions).not.toContain('cn')
  })

  it('separates a script-named catalog from its language default', () => {
    const entry = buildEntry('zh-Hant')
    expect(entry).toMatchObject({ tag: 'zh-Hant', script: 'hant', defaultScript: 'hans' })
  })

  it('keeps the catalog directory name verbatim, since the frontend keys on it', () => {
    expect(buildEntry('pt-BR').tag).toBe('pt-BR')
  })
})

describe('buildShippedLocales', () => {
  it('sorts by tag so the generated file is stable across filesystem orderings', () => {
    expect(buildShippedLocales(['zh', 'de', 'en']).map((e) => e.tag)).toEqual(['de', 'en', 'zh'])
  })

  it('drops the pseudolocale, which is what makes it unreachable by auto-selection', () => {
    const tags = buildShippedLocales(['en', PSEUDO_LOCALE]).map((e) => e.tag)
    expect(tags).toEqual(['en'])
  })
})

describe('buildParentLocales', () => {
  it('keeps only the languages we ship, since the resolver rejects the rest first', () => {
    const pairs = buildParentLocales(PARENT_LOCALE_FIXTURE, ['en', 'zh'])
    expect(pairs.map(([child]) => child)).not.toContain('ca-fr')
  })

  it('lowercases and sorts, matching the tags the resolver compares', () => {
    expect(buildParentLocales(PARENT_LOCALE_FIXTURE, ['en', 'zh'])).toEqual([
      ['en-150', 'en-001'],
      ['en-at', 'en-150'],
      ['en-nz', 'en-001'],
      ['zh-hant', 'und'],
    ])
  })

  it('keeps `und`, which is CLDR saying a locale has no parent at all', () => {
    const pairs = buildParentLocales(PARENT_LOCALE_FIXTURE, ['zh'])
    expect(pairs).toEqual([['zh-hant', 'und']])
  })

  it('keeps entries for a language whose regional catalog has not landed yet', () => {
    // `es-MX` → `es-419` is inert while only base `es` ships, and goes live by
    // itself the day an `es-419` catalog does.
    expect(buildParentLocales({ 'es-MX': 'es-419' }, ['es'])).toEqual([['es-mx', 'es-419']])
  })
})

describe('emitRustModule', () => {
  it('emits a table Rust can compile, with the fields the resolver reads', () => {
    const source = emitRustModule(buildShippedLocales(['en', 'zh']), [])
    expect(source).toContain('pub(crate) const SHIPPED_LOCALES: &[ShippedLocale] = &[')
    expect(source).toContain('tag: "zh",')
    expect(source).toContain('default_script: "hans",')
    expect(source).toContain('("tw", "hant")')
    expect(source).toContain('DO NOT EDIT BY HAND')
  })

  it('emits an empty region list rather than omitting the field', () => {
    expect(emitRustModule(buildShippedLocales(['en']), [])).toContain('region_scripts: &[],')
  })

  it('emits the parent table as pairs the resolver can scan', () => {
    const source = emitRustModule(buildShippedLocales(['en']), [['en-nz', 'en-001']])
    expect(source).toContain('pub(crate) const PARENT_LOCALES: &[(&str, &str)] = &[')
    expect(source).toContain('("en-nz", "en-001"),')
  })

  it('gives the British catalog the World-English node to answer for', () => {
    // The one editorial line in the generator, and the whole reason an `en-NZ`
    // reader gets "Bin": without it the walk falls past `en-001` to US English.
    expect(emitRustModule(buildShippedLocales(['en-GB']), [])).toContain('covers: &["en-001"],')
    expect(emitRustModule(buildShippedLocales(['en']), [])).toContain('covers: &[],')
  })
})
