/**
 * The Language picker's row labels. The question a label has to answer is "which
 * of these rows is mine?", so what matters is that the list in front of the user
 * is unambiguous, and that nothing carries a qualifier that distinguishes nothing.
 */
import { describe, it, expect } from 'vitest'
import { localeDisplayName } from './locale-display-names'
import { availableLocales } from './messages.svelte'

const SHIPPED = ['de', 'en', 'en-AU', 'en-GB', 'es', 'fr', 'hu', 'nl', 'pt', 'sv', 'vi', 'zh', 'zh-Hant']

describe('localeDisplayName', () => {
  it('names a language in its own language', () => {
    expect(localeDisplayName('de', SHIPPED)).toBe('Deutsch')
    expect(localeDisplayName('sv', SHIPPED)).toBe('Svenska')
  })

  it('leaves a language with no sibling undecorated', () => {
    // ❌ Never "Deutsch (Lateinisch)". A script nothing else contradicts is noise.
    expect(localeDisplayName('hu', SHIPPED)).not.toContain('(')
    expect(localeDisplayName('fr', SHIPPED)).toBe('Français')
  })

  it('separates region siblings by dialect, without naming their shared script', () => {
    expect(localeDisplayName('en', SHIPPED)).toBe('English')
    expect(localeDisplayName('en-GB', SHIPPED)).toBe('British English')
    expect(localeDisplayName('en-AU', SHIPPED)).toBe('Australian English')
  })

  it('names the script when a sibling is written in a different one', () => {
    // The bug this fixes: CLDR's endonym for bare `zh` is just 中文, so a
    // Traditional reader could not tell which row was Simplified.
    expect(localeDisplayName('zh', SHIPPED)).toBe('简体中文')
    expect(localeDisplayName('zh-Hant', SHIPPED)).toBe('繁體中文')
  })

  it('drops the script qualifier when no sibling contradicts it', () => {
    // Deliberately list-dependent: with only one Chinese on offer, 中文 is the
    // natural endonym and the qualifier answers a question nobody is asking.
    expect(localeDisplayName('zh', ['de', 'en', 'zh'])).toBe('中文')
  })

  it('falls back to the raw tag when Intl refuses the tag outright', () => {
    // A malformed tag throws inside `Intl`; the row still has to render something.
    expect(localeDisplayName('!!', ['en', '!!'])).toBe('!!')
  })

  it('gives every shipped catalog a label no other row shares', () => {
    // The guard on the whole design: two rows reading the same thing is the
    // failure mode, and it must fail loudly the moment a new catalog lands.
    const shipped = availableLocales()
    const labels = shipped.map((tag) => localeDisplayName(tag, shipped))
    expect(new Set(labels).size).toBe(labels.length)
  })
})
