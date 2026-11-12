import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'

import {
  addMissingEntries,
  macosCopyDrift,
  type AcceptedTranslationChange,
  type MacosCopyBaseline,
  type ResolvedValue,
} from './i18n-macos-copy-baseline-lib.ts'

type Resolved = Partial<Record<string, Partial<Record<string, ResolvedValue>>>>

const baseline = (): MacosCopyBaseline => ({
  readme: 'old readme',
  locales: { en: { 'a.key': 'A', 'c.key': 'C' }, de: { 'a.key': 'A-de', 'c.key': 'C-de' } },
})

const resolved: Resolved = {
  en: {
    'a.key': { value: 'A changed', from: 'en' },
    'b.key': { value: 'B', from: 'en' },
    'c.key': { value: 'C changed', from: 'en' },
  },
  de: {
    'a.key': { value: 'A-de changed', from: 'de' },
    'b.key': { value: 'B-de', from: 'de' },
    'c.key': { value: 'C-de', from: 'de' },
  },
}

describe('addMissingEntries', () => {
  it('adds only the absent keys and never rewrites an existing entry, even when the catalog moved', () => {
    const { baseline: next, added } = addMissingEntries(baseline(), resolved, ['a.key', 'b.key', 'c.key'], 'new')
    expect(next.locales.en).toEqual({ 'a.key': 'A', 'b.key': 'B', 'c.key': 'C' })
    expect(next.locales.de).toEqual({ 'a.key': 'A-de', 'b.key': 'B-de', 'c.key': 'C-de' })
    expect(added).toBe(2)
    expect(next.readme).toBe('new')
  })

  it('keeps each locale sorted by key, so the file diff is additions only', () => {
    const { baseline: next } = addMissingEntries(baseline(), resolved, ['b.key'], 'new')
    expect(Object.keys(next.locales.en)).toEqual(['a.key', 'b.key', 'c.key'])
  })

  it('refuses a key that already holds a platform block', () => {
    const blocked: Resolved = {
      en: { 'b.key': { value: '{platform, select, macos {B} other {b}}', from: 'en' } },
      de: { 'b.key': { value: 'B-de', from: 'de' } },
    }
    expect(() => addMissingEntries(baseline(), blocked, ['b.key'], 'new')).toThrow(/platform block/)
  })

  it('refuses a key no catalog defines', () => {
    expect(() => addMissingEntries(baseline(), resolved, ['z.key'], 'new')).toThrow(/z\.key/)
  })

  it('never adds a locale the baseline lacks', () => {
    const extra: Resolved = { ...resolved, fr: { 'b.key': { value: 'B-fr', from: 'fr' } } }
    expect(Object.keys(addMissingEntries(baseline(), extra, ['b.key'], 'new').baseline.locales)).toEqual(['en', 'de'])
  })
})

/**
 * The escape hatch for a wrong frozen translation (#1125): a reviewed entry in
 * `ACCEPTED_TRANSLATION_CHANGES` is the only way a non-English Mac text moves.
 */
describe('macosCopyDrift: accepted translation changes', () => {
  const key = 'menu.window.minimize'
  let root = ''

  /** Writes one `menu.json` per locale, holding `key` when the locale has a value, and returns the root. */
  function catalogs(values: Record<string, string | null>): string {
    root = mkdtempSync(join(tmpdir(), 'cmdr-macos-baseline-'))
    for (const [locale, value] of Object.entries(values)) {
      mkdirSync(join(root, locale))
      writeFileSync(join(root, locale, 'menu.json'), JSON.stringify(value === null ? {} : { [key]: value }))
    }
    return root
  }

  afterEach(() => {
    if (root) rmSync(root, { recursive: true, force: true })
    root = ''
  })

  const frozen: MacosCopyBaseline = {
    readme: '',
    locales: {
      en: { [key]: 'Minimize' },
      de: { [key]: 'Im Dock ablegen' },
      es: { [key]: 'Minimizar' },
      'es-419': { [key]: 'Minimizar' },
    },
  }
  const fixGerman: AcceptedTranslationChange = {
    locale: 'de',
    key,
    text: 'Minimieren',
    reason: 'the English is neutral; “Im Dock ablegen” is macOS vocabulary (#1103 decision 11)',
  }
  const today = { en: 'Minimize', de: 'Im Dock ablegen', es: 'Minimizar', 'es-419': null }

  it('flags a fixed translation that nothing accepted', () => {
    const at = catalogs({ ...today, de: 'Minimieren' })
    expect(macosCopyDrift(frozen, at, [])).toEqual([`de ${key}: "Im Dock ablegen" → "Minimieren"`])
  })

  it('lets an accepted fix through, as a plain value', () => {
    expect(macosCopyDrift(frozen, catalogs({ ...today, de: 'Minimieren' }), [fixGerman])).toEqual([])
  })

  it('lets an accepted fix through, as a block’s macOS branch', () => {
    const at = catalogs({ ...today, de: '{platform, select, macos {Minimieren} other {Fenster minimieren}}' })
    expect(macosCopyDrift(frozen, at, [fixGerman])).toEqual([])
  })

  it('holds the catalog to the accepted text, so an entry never applied is drift too', () => {
    expect(macosCopyDrift(frozen, catalogs(today), [fixGerman])).toEqual([
      `de ${key}: "Minimieren" → "Im Dock ablegen"`,
    ])
  })

  it('covers every locale that reads the value from the fixed catalog, an inheriting overlay included', () => {
    const fixSpanish: AcceptedTranslationChange = { locale: 'es', key, text: 'Minimizar ventana', reason: 'clearer' }
    const at = catalogs({ ...today, es: 'Minimizar ventana' })
    expect(macosCopyDrift(frozen, at, [])).toEqual([
      `es ${key}: "Minimizar" → "Minimizar ventana"`,
      `es-419 ${key}: "Minimizar" → "Minimizar ventana"`,
    ])
    expect(macosCopyDrift(frozen, at, [fixSpanish])).toEqual([])
  })

  it('refuses an English entry, a key nothing froze, a missing reason, and a duplicate', () => {
    const issues = macosCopyDrift(frozen, catalogs(today), [
      { locale: 'en', key, text: 'Minimize', reason: 'x' },
      { locale: 'de', key: 'menu.nothing.here', text: 'x', reason: 'x' },
      { ...fixGerman, reason: ' ' },
      fixGerman,
    ])
    expect(issues).toEqual(
      expect.arrayContaining([
        expect.stringContaining('English changes go in ACCEPTED_ENGLISH_CHANGES'),
        expect.stringContaining('the baseline froze no such key'),
        expect.stringContaining('needs a reason'),
        expect.stringContaining('listed twice'),
      ]),
    )
  })
})
