import { describe, expect, it } from 'vitest'
import { basenameOf, parentOf, toCanonical } from './canonical'

const HOME = '/Users/foo'

describe('toCanonical', () => {
  it('expands bare ~ to home dir', () => {
    expect(toCanonical('~', HOME)).toBe(HOME)
  })

  it('expands ~/sub to home + sub', () => {
    expect(toCanonical('~/Documents', HOME)).toBe('/Users/foo/Documents')
  })

  it('passes absolute POSIX paths through', () => {
    expect(toCanonical('/Users/foo/bar', HOME)).toBe('/Users/foo/bar')
    expect(toCanonical('/', HOME)).toBe('/')
  })

  it('passes virtual-volume URLs through', () => {
    expect(toCanonical('mtp://dev/65537/Music', HOME)).toBe('mtp://dev/65537/Music')
    expect(toCanonical('smb://host/share/dir', HOME)).toBe('smb://host/share/dir')
    expect(toCanonical('search-results://snap-1', HOME)).toBe('search-results://snap-1')
  })

  it('throws on relative paths', () => {
    expect(() => toCanonical('foo/bar', HOME)).toThrow(/not absolute/)
    expect(() => toCanonical('.', HOME)).toThrow(/not absolute/)
  })

  it('throws when expanding ~ without a home dir', () => {
    expect(() => toCanonical('~', '')).toThrow(/homeDir is empty/)
    expect(() => toCanonical('~/Docs', '')).toThrow(/homeDir is empty/)
  })
})

describe('parentOf', () => {
  it('returns the parent of an absolute path', () => {
    expect(parentOf(toCanonical('/Users/foo/bar', HOME))).toBe('/Users/foo')
    expect(parentOf(toCanonical('/Users', HOME))).toBe('/')
  })

  it('returns / for /', () => {
    expect(parentOf(toCanonical('/', HOME))).toBe('/')
  })

  // The bug this whole module exists to prevent: navigating `..` from `~`
  // used to land on `/` because `~.lastIndexOf('/')` is -1.
  it('returns /Users when called on canonicalised ~', () => {
    expect(parentOf(toCanonical('~', HOME))).toBe('/Users')
  })

  it('handles virtual-volume URLs by slash arithmetic', () => {
    expect(parentOf(toCanonical('mtp://dev/65537/Music', HOME))).toBe('mtp://dev/65537')
  })
})

describe('basenameOf', () => {
  it('returns the final segment', () => {
    expect(basenameOf(toCanonical('/Users/foo/bar', HOME))).toBe('bar')
    expect(basenameOf(toCanonical('/Users', HOME))).toBe('Users')
  })

  it('returns "" for /', () => {
    expect(basenameOf(toCanonical('/', HOME))).toBe('')
  })

  // Same root cause as the parentOf bug: `~.split('/').pop()` used to return '~'.
  it('returns the home folder name when called on canonicalised ~', () => {
    expect(basenameOf(toCanonical('~', HOME))).toBe('foo')
  })
})
