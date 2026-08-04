/**
 * Pins the Search-in scope ladder: the "Use current folder" smart fallback, the volume
 * rung above it, and which of the two an unset scope defaults to.
 */
import { describe, it, expect } from 'vitest'
import { resolveSearchScope, resolveDefaultScope, volumeRootFor } from './searchable-folder'

const LOCAL_ONLY = ['/']
const WITH_NAS = ['/', '/Volumes/naspi']

describe('resolveSearchScope', () => {
  it('returns the current path when it is a real folder', () => {
    const out = resolveSearchScope({
      currentPath: '/Users/me/projects',
      history: ['/Users/me', '/Users/me/projects'],
      volumeRoots: LOCAL_ONLY,
    })
    expect(out.currentFolder).toBe('/Users/me/projects')
    expect(out.currentFolderUnavailableReason).toBe('')
  })

  it('walks back to the most recent real folder when on a search-results pane', () => {
    const out = resolveSearchScope({
      currentPath: 'search-results://sr-7',
      history: ['/', '/Users/me', '/Users/me/projects', 'search-results://sr-7'],
      volumeRoots: LOCAL_ONLY,
    })
    expect(out.currentFolder).toBe('/Users/me/projects')
    expect(out.currentFolderUnavailableReason).toBe('')
  })

  it('skips through multiple search-results entries to find a real folder', () => {
    const out = resolveSearchScope({
      currentPath: 'search-results://sr-9',
      history: ['/Users/me', '/Users/me/projects', 'search-results://sr-1', 'search-results://sr-9'],
      volumeRoots: LOCAL_ONLY,
    })
    expect(out.currentFolder).toBe('/Users/me/projects')
  })

  it('has no current folder, with the canonical tooltip, when history holds only search results', () => {
    const out = resolveSearchScope({
      currentPath: 'search-results://sr-3',
      history: ['search-results://sr-1', 'search-results://sr-3'],
      volumeRoots: LOCAL_ONLY,
    })
    expect(out.currentFolder).toBeNull()
    expect(out.currentFolderUnavailableReason).toContain('search results')
    expect(out.currentFolderUnavailableReason).toContain('Open a real folder')
  })

  it('has no current folder with an empty history on a search-results pane', () => {
    const out = resolveSearchScope({
      currentPath: 'search-results://sr-1',
      history: [],
      volumeRoots: LOCAL_ONLY,
    })
    expect(out.currentFolder).toBeNull()
  })

  it('uses the most recent (last) real folder, not the oldest', () => {
    const out = resolveSearchScope({
      currentPath: 'search-results://sr-2',
      history: ['/Users/old', '/Users/middle', '/Users/recent', 'search-results://sr-2'],
      volumeRoots: LOCAL_ONLY,
    })
    expect(out.currentFolder).toBe('/Users/recent')
  })

  it("resolves 'this volume' to the mount root the current folder is on", () => {
    const out = resolveSearchScope({
      currentPath: '/Volumes/naspi/photos/2026',
      history: ['/Volumes/naspi/photos/2026'],
      volumeRoots: WITH_NAS,
    })
    expect(out.volumeRoot).toBe('/Volumes/naspi')
  })
})

describe('volumeRootFor', () => {
  it('picks the longest containing root, not the first match', () => {
    // Every path is under `/`, so a naive scan would call a NAS folder a boot-disk one.
    expect(volumeRootFor(WITH_NAS, '/Volumes/naspi/photos')).toBe('/Volumes/naspi')
    expect(volumeRootFor(WITH_NAS, '/Users/me')).toBe('/')
  })

  it('matches whole segments, so a same-prefix sibling volume never wins', () => {
    expect(volumeRootFor(['/', '/Volumes/nas'], '/Volumes/nas-backup/x')).toBe('/')
  })

  it('handles a non-filesystem volume root, like MTP', () => {
    expect(volumeRootFor(['/', 'mtp://phone-1/65537'], 'mtp://phone-1/65537/DCIM')).toBe('mtp://phone-1/65537')
  })

  it('falls back to the boot volume when nothing contains the path or there is no path', () => {
    expect(volumeRootFor(['/Volumes/gone'], '/Users/me')).toBe('/')
    expect(volumeRootFor(WITH_NAS, null)).toBe('/')
  })
})

describe('resolveDefaultScope', () => {
  it('defaults to the current folder', () => {
    const presets = { currentFolder: '/Users/me/projects', currentFolderUnavailableReason: '', volumeRoot: '/' }
    expect(resolveDefaultScope(presets)).toEqual({ path: '/Users/me/projects', kind: 'currentFolder' })
  })

  it('falls back to this volume when the pane has no real folder, so a search still runs', () => {
    // A snapshot pane with nothing but search results behind it: there's no folder to
    // default to, and refusing to search would be worse than searching one rung wider.
    const presets = { currentFolder: null, currentFolderUnavailableReason: 'nope', volumeRoot: '/Volumes/naspi' }
    expect(resolveDefaultScope(presets)).toEqual({ path: '/Volumes/naspi', kind: 'thisVolume' })
  })
})
