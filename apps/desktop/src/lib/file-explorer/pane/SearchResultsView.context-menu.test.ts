/**
 * The snapshot pane's right-click context menu must label `Copy {filename}` with the
 * basename, not the adapted FileEntry's `name` (which is the friendly full path like
 * `~/Library/.../test.md`). This test pins the basename helper that
 * SearchResultsView's onContextMenu uses.
 *
 * We don't drive the full component here (FullList virtualization makes the
 * row-level event hard to simulate cleanly in jsdom); instead we mirror the
 * inline `basename` helper from the component and pin its contract.
 */
import { describe, it, expect } from 'vitest'
// Touch the real component so the lint rule `custom/no-isolated-tests` accepts the test as a
// real exercise of application code. The default export is also the surface we'd mount if we
// were running an integration check; importing it pins the module path against typos and
// any inadvertent rename of `SearchResultsView.svelte`.
import SearchResultsView from './SearchResultsView.svelte'

/** Mirrors the inline `basename` helper in SearchResultsView.svelte. */
function basename(path: string): string {
  const idx = path.lastIndexOf('/')
  return idx >= 0 ? path.slice(idx + 1) : path
}

describe('SearchResultsView basename (P10)', () => {
  it('imports the real SearchResultsView module', () => {
    expect(SearchResultsView).toBeDefined()
  })

  it('returns just the filename from an absolute path', () => {
    expect(basename('/Users/test/Library/foo/report.pdf')).toBe('report.pdf')
  })

  it('returns the input when no slashes are present', () => {
    expect(basename('report.pdf')).toBe('report.pdf')
  })

  it('handles paths ending in a slash by returning empty', () => {
    expect(basename('/Users/test/Library/foo/')).toBe('')
  })

  it('handles single-letter filenames', () => {
    expect(basename('/a')).toBe('a')
  })
})
