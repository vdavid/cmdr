import { describe, it, expect } from 'vitest'
import { splitPathSegments } from './path-segments'
import { enrichBreadcrumbSegments, type BreadcrumbNavContext } from './breadcrumb-navigation'

/** Build the enriched segments for a display path under a given context. */
function enrich(displayPath: string, ctx: BreadcrumbNavContext) {
  return enrichBreadcrumbSegments(splitPathSegments(displayPath), ctx)
}

const rootCtx = (currentPath: string, userHomePath = '/Users/dave'): BreadcrumbNavContext => ({
  volumeId: 'root',
  volumePath: '/',
  currentPath,
  userHomePath,
  isSearchResults: false,
})

describe('enrichBreadcrumbSegments', () => {
  it('maps home-collapsed (~) segments back to absolute paths', () => {
    // Display "~/projects/foo" with home /Users/dave.
    const segs = enrich('~/projects/foo', rootCtx('/Users/dave/projects/foo'))
    expect(segs.map((s) => s.text)).toEqual(['~', 'projects', 'foo'])
    // "~" navigates home.
    expect(segs[0].target).toBe('/Users/dave')
    // "projects" navigates to /Users/dave/projects.
    expect(segs[1].target).toBe('/Users/dave/projects')
    // The current folder ("foo", last segment) is NOT clickable.
    expect(segs[2].target).toBeNull()
  })

  it('uses the visible display prefix for the tooltip path', () => {
    const segs = enrich('~/projects/foo', rootCtx('/Users/dave/projects/foo'))
    expect(segs[0].displayPath).toBe('~')
    expect(segs[1].displayPath).toBe('~/projects')
  })

  it('maps absolute root-volume paths outside home', () => {
    const segs = enrich('/etc/ssl/certs', rootCtx('/etc/ssl/certs'))
    // Leading "" root marker is not clickable (nothing visible to click).
    expect(segs[0].text).toBe('')
    expect(segs[0].target).toBeNull()
    // "etc" -> /etc, "ssl" -> /etc/ssl, "certs" (current) -> null.
    expect(segs[1].target).toBe('/etc')
    expect(segs[2].target).toBe('/etc/ssl')
    expect(segs[3].target).toBeNull()
    expect(segs[1].displayPath).toBe('/etc')
  })

  it('prepends the volume path for non-root volumes', () => {
    const ctx: BreadcrumbNavContext = {
      volumeId: 'vol-x',
      volumePath: '/Volumes/X',
      currentPath: '/Volumes/X/sub/dir',
      userHomePath: '/Users/dave',
      isSearchResults: false,
    }
    // Display is volume-relative "/sub/dir".
    const segs = enrich('/sub/dir', ctx)
    expect(segs[0].text).toBe('')
    expect(segs[0].target).toBeNull()
    expect(segs[1].target).toBe('/Volumes/X/sub')
    expect(segs[2].target).toBeNull() // current folder
  })

  it('reconstructs MTP paths from the parsed device/storage', () => {
    const ctx: BreadcrumbNavContext = {
      volumeId: 'mtp-0-5:65537',
      volumePath: 'mtp://0-5/65537',
      currentPath: 'mtp://0-5/65537/DCIM/Camera',
      userHomePath: '/Users/dave',
      isSearchResults: false,
    }
    // MTP display path is "/DCIM/Camera".
    const segs = enrich('/DCIM/Camera', ctx)
    expect(segs[1].target).toBe('mtp://0-5/65537/DCIM')
    expect(segs[2].target).toBeNull() // current folder
  })

  it('never makes search-results panes clickable', () => {
    const ctx: BreadcrumbNavContext = {
      volumeId: 'search-results',
      volumePath: '/',
      currentPath: 'search-results://abc',
      userHomePath: '/Users/dave',
      isSearchResults: true,
    }
    // A search snapshot renders its label as a single segment; nothing clickable.
    const segs = enrichBreadcrumbSegments([{ text: '*.pdf', gitPortal: false }], ctx)
    expect(segs[0].target).toBeNull()
  })

  it('returns null targets when the home dir has not resolved yet', () => {
    const segs = enrich('~/projects/foo', rootCtx('/Users/dave/projects/foo', ''))
    expect(segs[0].target).toBeNull()
    expect(segs[1].target).toBeNull()
  })

  it('keeps git-portal segments clickable', () => {
    const segs = enrich('~/repo/.git/refs', rootCtx('/Users/dave/repo/.git/refs'))
    const dotGit = segs.find((s) => s.text === '.git')
    expect(dotGit?.gitPortal).toBe(true)
    expect(dotGit?.target).toBe('/Users/dave/repo/.git')
  })
})
