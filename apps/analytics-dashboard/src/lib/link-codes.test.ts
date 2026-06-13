import { describe, it, expect } from 'vitest'
import { isValidCode, sanitizeUtmValue, validateLinkCode, toRows, exampleLink } from './link-codes.js'

describe('isValidCode', () => {
  it('accepts lowercase alphanumerics and . _ -', () => {
    expect(isValidCode('hn')).toBe(true)
    expect(isValidCode('reddit-rust')).toBe(true)
    expect(isValidCode('v1.2_beta')).toBe(true)
  })

  it('rejects empty, uppercase, spaces, and other chars', () => {
    expect(isValidCode('')).toBe(false)
    expect(isValidCode('HN')).toBe(false)
    expect(isValidCode('has space')).toBe(false)
    expect(isValidCode('with/slash')).toBe(false)
  })

  it('rejects codes over 64 chars', () => {
    expect(isValidCode('a'.repeat(64))).toBe(true)
    expect(isValidCode('a'.repeat(65))).toBe(false)
  })
})

describe('sanitizeUtmValue', () => {
  it('lowercases and strips disallowed chars', () => {
    expect(sanitizeUtmValue('Hacker News')).toBe('hackernews')
    expect(sanitizeUtmValue('reddit-rust')).toBe('reddit-rust')
  })

  it('returns empty string for nullish/empty input', () => {
    expect(sanitizeUtmValue('')).toBe('')
    expect(sanitizeUtmValue(undefined)).toBe('')
    expect(sanitizeUtmValue(null)).toBe('')
  })
})

describe('validateLinkCode', () => {
  it('accepts a valid full entry and normalizes it', () => {
    const result = validateLinkCode({ code: 'HN', utm_source: 'Hacker News', utm_medium: 'Social', note: ' launch ' })
    expect(result).toEqual({ ok: true, code: 'hn', utm_source: 'hackernews', utm_medium: 'social', note: 'launch' })
  })

  it('accepts a minimal entry (code + source only)', () => {
    const result = validateLinkCode({ code: 'nl', utm_source: 'newsletter' })
    expect(result).toEqual({ ok: true, code: 'nl', utm_source: 'newsletter' })
  })

  it('omits empty medium and note', () => {
    const result = validateLinkCode({ code: 'x', utm_source: 'x', utm_medium: '', note: '  ' })
    expect(result).toEqual({ ok: true, code: 'x', utm_source: 'x' })
  })

  it('rejects an invalid code', () => {
    const result = validateLinkCode({ code: 'bad code', utm_source: 'x' })
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error).toContain('lowercase')
  })

  it('rejects a missing source', () => {
    const result = validateLinkCode({ code: 'hn', utm_source: '' })
    expect(result.ok).toBe(false)
    if (result.ok) return
    expect(result.error).toContain('source is required')
  })

  it('rejects a source that sanitizes to empty', () => {
    const result = validateLinkCode({ code: 'hn', utm_source: '!!!' })
    expect(result.ok).toBe(false)
  })
})

describe('toRows', () => {
  it('flattens the map into sorted rows with empty-string fallbacks', () => {
    const rows = toRows({
      rr: { utm_source: 'reddit-rust', utm_medium: 'social' },
      hn: { utm_source: 'hackernews', utm_medium: 'social', note: 'launch' },
      nl: { utm_source: 'newsletter' },
    })
    expect(rows.map((r) => r.code)).toEqual(['hn', 'nl', 'rr'])
    expect(rows[1]).toEqual({ code: 'nl', utm_source: 'newsletter', utm_medium: '', note: '' })
  })
})

describe('exampleLink', () => {
  it('builds the getcmdr.com example link', () => {
    expect(exampleLink('hn')).toBe('getcmdr.com/?r=hn')
  })
})
