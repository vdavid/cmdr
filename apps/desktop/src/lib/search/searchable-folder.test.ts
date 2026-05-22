/**
 * Round 2 D12: pinning the "Use current folder" smart fallback for the
 * Search-in popover.
 */
import { describe, it, expect } from 'vitest'
import { resolveSearchableFolder } from './searchable-folder'

describe('resolveSearchableFolder', () => {
  it('returns the current path when it is a real folder', () => {
    const out = resolveSearchableFolder({
      currentPath: '/Users/me/projects',
      history: ['/Users/me', '/Users/me/projects'],
    })
    expect(out).toEqual({ path: '/Users/me/projects', disabled: false, disabledReason: '' })
  })

  it('walks back to the most recent real folder when on a search-results pane', () => {
    const out = resolveSearchableFolder({
      currentPath: 'search-results://sr-7',
      history: ['/', '/Users/me', '/Users/me/projects', 'search-results://sr-7'],
    })
    expect(out.path).toBe('/Users/me/projects')
    expect(out.disabled).toBe(false)
  })

  it('skips through multiple search-results entries to find a real folder', () => {
    const out = resolveSearchableFolder({
      currentPath: 'search-results://sr-9',
      history: ['/Users/me', '/Users/me/projects', 'search-results://sr-1', 'search-results://sr-9'],
    })
    expect(out.path).toBe('/Users/me/projects')
    expect(out.disabled).toBe(false)
  })

  it('returns disabled + canonical tooltip when history has only search-results entries', () => {
    const out = resolveSearchableFolder({
      currentPath: 'search-results://sr-3',
      history: ['search-results://sr-1', 'search-results://sr-3'],
    })
    expect(out.path).toBeNull()
    expect(out.disabled).toBe(true)
    expect(out.disabledReason).toContain('search results')
    expect(out.disabledReason).toContain('Open a real folder')
  })

  it('returns disabled with an empty history on a search-results pane', () => {
    const out = resolveSearchableFolder({
      currentPath: 'search-results://sr-1',
      history: [],
    })
    expect(out.path).toBeNull()
    expect(out.disabled).toBe(true)
  })

  it('uses the most recent (last) real folder, not the oldest', () => {
    const out = resolveSearchableFolder({
      currentPath: 'search-results://sr-2',
      history: ['/Users/old', '/Users/middle', '/Users/recent', 'search-results://sr-2'],
    })
    expect(out.path).toBe('/Users/recent')
  })
})
