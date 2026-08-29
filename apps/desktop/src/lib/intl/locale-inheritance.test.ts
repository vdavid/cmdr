/**
 * The truth table for "which catalog may this locale inherit from", at its home.
 *
 * Three layers depend on this answer (the message runtime, the i18n check layer,
 * and, through the generated table, Rust's auto-selection), so the cases below
 * are deliberately stated as language-vs-script pairs rather than as any one
 * layer's behavior. Each layer's own use of it is covered where it lives:
 * `messages.svelte.test.ts` (per-key fallback) and
 * `scripts/i18n-catalog-lib.test.ts` (overlay classification).
 */
import { describe, it, expect } from 'vitest'
import { ancestorTags, inheritableAncestors, likelyScript } from './locale-inheritance'

describe('likelyScript', () => {
  it('reads CLDR likely subtags, including the region-implies-script case', () => {
    expect(likelyScript('en')).toBe('latn')
    expect(likelyScript('zh')).toBe('hans') // the bare language is Simplified
    expect(likelyScript('zh-Hant')).toBe('hant')
    expect(likelyScript('zh-TW')).toBe('hant') // no script subtag; the REGION says so
  })

  it('answers empty rather than throwing on a tag Intl can not resolve', () => {
    // The synthetic test locales (`zz-ZZ`) and any malformed tag land here, and
    // an empty answer matches another empty one, so fallback still works.
    expect(likelyScript('not a tag')).toBe('')
    expect(likelyScript('zz-ZZ')).toBe(likelyScript('zz'))
  })
})

describe('ancestorTags', () => {
  it('drops one subtag at a time, nearest first, excluding the tag itself', () => {
    expect(ancestorTags('zh-Hant-TW')).toEqual(['zh-Hant', 'zh'])
    expect(ancestorTags('en-GB')).toEqual(['en'])
    expect(ancestorTags('en')).toEqual([])
  })
})

describe('inheritableAncestors', () => {
  const shipped = ['en', 'de', 'pt', 'zh']

  it('lets a regional variant inherit its language base, which is the point', () => {
    expect(inheritableAncestors('pt-PT', shipped)).toEqual(['pt'])
    expect(inheritableAncestors('en-GB', shipped)).toEqual(['en'])
  })

  it('refuses to cross a script boundary, by script subtag or by region', () => {
    expect(inheritableAncestors('zh-Hant', shipped)).toEqual([])
    expect(inheritableAncestors('zh-TW', shipped)).toEqual([])
  })

  it('still inherits within the same script', () => {
    expect(inheritableAncestors('zh-CN', shipped)).toEqual(['zh'])
  })

  it('prefers the nearest readable ancestor and skips the unreadable one behind it', () => {
    expect(inheritableAncestors('zh-Hant-TW', [...shipped, 'zh-Hant'])).toEqual(['zh-Hant'])
  })

  it('returns nothing when the ancestor does not exist', () => {
    expect(inheritableAncestors('fr-CA', shipped)).toEqual([])
  })
})
