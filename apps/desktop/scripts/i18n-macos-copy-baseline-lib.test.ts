import { describe, expect, it } from 'vitest'

import { addMissingEntries, type MacosCopyBaseline, type ResolvedValue } from './i18n-macos-copy-baseline-lib.ts'

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
