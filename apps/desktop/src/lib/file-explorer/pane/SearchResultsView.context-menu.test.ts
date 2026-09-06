/**
 * The two rules `SearchResultsView.svelte`'s row context menu runs on, unit-tested
 * on the pure helpers behind it. That the view actually calls them with its own
 * rows and selection is pinned one tier up, in `SearchResultsView.svelte.test.ts`
 * (a real `contextmenu` event on a rendered row).
 */
import { describe, it, expect } from 'vitest'
import { snapshotBasename, snapshotContextMenuPaths } from './snapshot-context-menu'

const ROWS = [{ path: '/Users/test/a.txt' }, { path: '/Users/test/b.txt' }, { path: '/Users/test/c.txt' }]

describe('snapshotContextMenuPaths', () => {
  it('acts on the whole selection when the clicked row is part of it', () => {
    expect(snapshotContextMenuPaths('/Users/test/a.txt', ROWS, new Set([0, 2]))).toEqual([
      '/Users/test/a.txt',
      '/Users/test/c.txt',
    ])
  })

  it('acts on the clicked row alone when it sits outside the selection', () => {
    expect(snapshotContextMenuPaths('/Users/test/b.txt', ROWS, new Set([0, 2]))).toEqual(['/Users/test/b.txt'])
  })

  it('acts on the clicked row alone when nothing is selected', () => {
    expect(snapshotContextMenuPaths('/Users/test/b.txt', ROWS, new Set())).toEqual(['/Users/test/b.txt'])
  })

  it('returns the selection in row order, whatever order the user picked it in', () => {
    expect(snapshotContextMenuPaths('/Users/test/c.txt', ROWS, new Set([2, 0]))).toEqual([
      '/Users/test/a.txt',
      '/Users/test/c.txt',
    ])
  })

  it('drops a selected index the rows no longer have', () => {
    expect(snapshotContextMenuPaths('/Users/test/a.txt', ROWS, new Set([0, 42]))).toEqual(['/Users/test/a.txt'])
  })
})

describe('snapshotBasename', () => {
  it('returns just the filename from an absolute path', () => {
    expect(snapshotBasename('/Users/test/Library/foo/report.pdf')).toBe('report.pdf')
  })

  it('returns the input when no slashes are present', () => {
    expect(snapshotBasename('report.pdf')).toBe('report.pdf')
  })

  it('handles paths ending in a slash by returning empty', () => {
    expect(snapshotBasename('/Users/test/Library/foo/')).toBe('')
  })

  it('handles single-letter filenames', () => {
    expect(snapshotBasename('/a')).toBe('a')
  })
})
