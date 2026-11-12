/**
 * Tests for `snapshot-stats.ts`, the search-results pane's footer arithmetic.
 *
 * They pin the four things the readout can get wrong: the file/dir split, a row
 * with no size counting as zero rather than `NaN`, an empty selection reporting
 * `null` (the shape the backend uses for "nothing selected"), and a stale index
 * from a delete-sync race being ignored instead of folding `undefined` in.
 */

import { describe, it, expect } from 'vitest'
import type { SearchResultEntry } from '#lib/ipc/bindings.js'
import { computeSnapshotStats } from './snapshot-stats'

function row(overrides: Partial<SearchResultEntry> = {}): SearchResultEntry {
  return {
    name: 'a.jpg',
    path: '/Users/test/Downloads/a.jpg',
    parentPath: '/Users/test/Downloads',
    isDirectory: false,
    size: 100,
    modifiedAt: 1_700_000_000,
    iconId: 'file',
    ...overrides,
  }
}

describe('computeSnapshotStats', () => {
  it('splits files from folders and sums the file sizes', () => {
    const stats = computeSnapshotStats(
      [row({ size: 100 }), row({ size: 250 }), row({ isDirectory: true, size: null })],
      [],
    )
    expect(stats.totalFiles).toBe(2)
    expect(stats.totalDirs).toBe(1)
    expect(stats.totalSize).toBe(350)
  })

  it('counts a row with no size as zero bytes', () => {
    const stats = computeSnapshotStats([row({ size: null }), row({ size: 40 })], [])
    expect(stats.totalSize).toBe(40)
  })

  it('mirrors the logical total into the on-disk total, so the "on disk" setting still shows a figure', () => {
    const stats = computeSnapshotStats([row({ size: 100 })], [0])
    expect(stats.totalPhysicalSize).toBe(stats.totalSize)
    expect(stats.selectedPhysicalSize).toBe(stats.selectedSize)
  })

  it('reports null selection fields when nothing is selected', () => {
    const stats = computeSnapshotStats([row()], [])
    expect(stats.selectedFiles).toBeNull()
    expect(stats.selectedDirs).toBeNull()
    expect(stats.selectedSize).toBeNull()
    expect(stats.selectedPhysicalSize).toBeNull()
  })

  it('folds only the rows the selection names', () => {
    const stats = computeSnapshotStats(
      [row({ size: 100 }), row({ size: 250 }), row({ isDirectory: true, size: null })],
      [1, 2],
    )
    expect(stats.selectedFiles).toBe(1)
    expect(stats.selectedDirs).toBe(1)
    expect(stats.selectedSize).toBe(250)
  })

  it('ignores an index past the end, which a delete-sync can leave behind', () => {
    const stats = computeSnapshotStats([row({ size: 100 })], [0, 7])
    expect(stats.selectedFiles).toBe(1)
    expect(stats.selectedSize).toBe(100)
  })

  it('reports an empty snapshot as empty, which the footer renders as its empty mode', () => {
    const stats = computeSnapshotStats([], [])
    expect(stats.totalFiles).toBe(0)
    expect(stats.totalDirs).toBe(0)
    expect(stats.totalSize).toBe(0)
  })
})
