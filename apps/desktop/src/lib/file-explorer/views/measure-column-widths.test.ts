/**
 * Tests for measure-column-widths.ts. We replace the real pretext-backed measurer
 * with a deterministic `text.length * 7` stand-in so assertions stay readable.
 */
import { afterEach, describe, expect, it } from 'vitest'

import type { FileEntry } from '../types'

import { _setMeasureForTests, computeFullListColumnWidths } from './measure-column-widths'

const fakeMeasure = (text: string): number => text.length * 7

function entry(overrides: Partial<FileEntry>): FileEntry {
  return {
    name: 'file.txt',
    path: '/x/file.txt',
    isDirectory: false,
    isSymlink: false,
    size: 0,
    physicalSize: 0,
    modifiedAt: undefined,
    permissions: 0o644,
    owner: 'u',
    group: 'g',
    iconId: 'text',
    extendedMetadataLoaded: false,
    ...overrides,
  }
}

const baseArgs = {
  parentDirStats: null,
  formatDateTime: (t: number) => new Date(t * 1000).toISOString().slice(0, 19).replace('T', ' '),
  sizeDisplayMode: 'smart' as const,
  indexing: false,
  showSizeMismatchWarning: false,
  sortBy: 'name' as const,
}

describe('computeFullListColumnWidths', () => {
  afterEach(() => {
    _setMeasureForTests(null)
  })

  it('falls back to header-only widths when entries is empty', () => {
    _setMeasureForTests(fakeMeasure)
    const w = computeFullListColumnWidths({ ...baseArgs, entries: [] })
    // With sortBy='name', none of Ext/Size/Modified are active, so each gets
    // HEADER_CHROME_INACTIVE (8). "Ext" = 21 + 8 = 29; "Size" = 28 + 8 = 36
    // → clamped to MIN_SIZE_WIDTH (40); "Modified" = 56 + 8 = 64 → clamped to
    // MIN_DATE_WIDTH (70).
    expect(w.ext).toBe(29)
    expect(w.size).toBe(40)
    expect(w.date).toBe(70)
  })

  it('widens the active sort column to reserve room for the caret', () => {
    _setMeasureForTests(fakeMeasure)
    const nameSorted = computeFullListColumnWidths({ ...baseArgs, entries: [] })
    const sizeSorted = computeFullListColumnWidths({ ...baseArgs, entries: [], sortBy: 'size' })
    // size col picks up the caret (+12 chrome) when sortBy==='size'; ext stays inactive.
    expect(sizeSorted.size).toBeGreaterThan(nameSorted.size)
    expect(sizeSorted.ext).toBe(nameSorted.ext)
  })

  it('widens size column when a large file is present', () => {
    _setMeasureForTests(fakeMeasure)
    const small = computeFullListColumnWidths({
      ...baseArgs,
      entries: [entry({ name: 'a.txt', size: 0, physicalSize: 0 })],
    })
    const big = computeFullListColumnWidths({
      ...baseArgs,
      entries: [entry({ name: 'z.bin', size: 100_000_000, physicalSize: 100_000_000 })],
    })
    expect(big.size).toBeGreaterThan(small.size)
  })

  it('widens ext column based on actual extensions', () => {
    _setMeasureForTests(fakeMeasure)
    const short = computeFullListColumnWidths({ ...baseArgs, entries: [entry({ name: 'a.js' })] })
    const long = computeFullListColumnWidths({ ...baseArgs, entries: [entry({ name: 'a.verylongext' })] })
    expect(long.ext).toBeGreaterThan(short.ext)
  })

  it('widens date column based on longest formatted date', () => {
    _setMeasureForTests(fakeMeasure)
    const short = computeFullListColumnWidths({
      ...baseArgs,
      formatDateTime: () => 'today',
      entries: [entry({ name: 'a', modifiedAt: 1 })],
    })
    const long = computeFullListColumnWidths({
      ...baseArgs,
      formatDateTime: () => '2026-12-31 23:59:59',
      entries: [entry({ name: 'a', modifiedAt: 1 })],
    })
    expect(long.date).toBeGreaterThan(short.date)
  })

  it('reserves icon width when a directory has a stale size during indexing', () => {
    _setMeasureForTests(fakeMeasure)
    const idle = computeFullListColumnWidths({
      ...baseArgs,
      entries: [entry({ name: 'd', isDirectory: true, recursiveSize: 12345 })],
    })
    const busy = computeFullListColumnWidths({
      ...baseArgs,
      indexing: true,
      entries: [entry({ name: 'd', isDirectory: true, recursiveSize: 12345 })],
    })
    expect(busy.size).toBeGreaterThanOrEqual(idle.size)
  })

  it('includes parentDirStats size when provided', () => {
    _setMeasureForTests(fakeMeasure)
    const without = computeFullListColumnWidths({
      ...baseArgs,
      entries: [entry({ name: 'a', size: 1 })],
    })
    const withParent = computeFullListColumnWidths({
      ...baseArgs,
      entries: [entry({ name: 'a', size: 1 })],
      parentDirStats: {
        path: '/x',
        recursiveSize: 999_999_999_999,
        recursivePhysicalSize: 999_999_999_999,
        recursiveFileCount: 1,
        recursiveDirCount: 0,
      },
    })
    expect(withParent.size).toBeGreaterThan(without.size)
  })

  it('never returns widths below the floor', () => {
    _setMeasureForTests(() => 0) // pathological: everything measures to zero
    const w = computeFullListColumnWidths({ ...baseArgs, entries: [entry({ name: 'a' })] })
    expect(w.ext).toBeGreaterThanOrEqual(28)
    expect(w.size).toBeGreaterThanOrEqual(40)
    expect(w.date).toBeGreaterThanOrEqual(70)
  })
})
