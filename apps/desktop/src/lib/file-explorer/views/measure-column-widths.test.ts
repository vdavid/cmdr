/**
 * Tests for measure-column-widths.ts. We replace the real pretext-backed measurer
 * with a deterministic `text.length * 7` stand-in so assertions stay readable.
 */
import { afterEach, describe, expect, it } from 'vitest'

import type { FileEntry } from '../types'

import { _setMeasureForTests, computeFullListColumnWidths } from './measure-column-widths'
import { _setLocaleForTests } from '$lib/intl/locale'
import { formatSizeForDisplay } from '../selection/selection-info-utils'

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

/**
 * Build a stub `FormattedDate` for tests that don't care about per-component
 * coloring. The whole string is one literal segment so `joinSegments`
 * reproduces it.
 */
function stubDate(text: string) {
  return {
    text,
    segments: [{ text, ageClass: null }],
  }
}

const baseArgs = {
  parentDirStats: null,
  formattedDate: (t: number) => stubDate(new Date(t * 1000).toISOString().slice(0, 19).replace('T', ' ')),
  sizeDisplayMode: 'smart' as const,
  indexing: false,
  showSizeMismatchWarning: false,
  sortBy: 'name' as const,
  sizeFormatOpts: { unit: 'bytes' as const, format: 'binary' as const },
}

describe('computeFullListColumnWidths', () => {
  afterEach(() => {
    _setMeasureForTests(null)
  })

  it('falls back to header-only widths when entries is empty', () => {
    _setMeasureForTests(fakeMeasure)
    const w = computeFullListColumnWidths({ ...baseArgs, entries: [] })
    // With sortBy='name', none of Ext/Size/Modified are active, so each gets
    // HEADER_CHROME_INACTIVE (0, labels sit flush with column-track edges).
    // "Ext" = 21 + 2 (pad) = 23 → clamped to MIN_EXT_WIDTH (28); "Size" = 30 →
    // clamped to MIN_SIZE_WIDTH (40); "Modified" = 58 → clamped to
    // MIN_DATE_WIDTH (70). Floors swallow the pad in the empty-entries case.
    expect(w.ext).toBe(28)
    expect(w.size).toBe(40)
    expect(w.date).toBe(70)
  })

  it('widens the active sort column to reserve room for the caret', () => {
    _setMeasureForTests(fakeMeasure)
    // The ext column is the one whose floor (MIN_EXT_WIDTH = 28) sits below
    // "Ext" + active chrome (21 + 12 = 33), so the caret allowance is visible.
    // Size and Date floors swallow the caret allowance whole, so we test ext.
    const nameSorted = computeFullListColumnWidths({ ...baseArgs, entries: [] })
    const extSorted = computeFullListColumnWidths({ ...baseArgs, entries: [], sortBy: 'extension' })
    expect(extSorted.ext).toBeGreaterThan(nameSorted.ext)
    expect(extSorted.size).toBe(nameSorted.size)
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

  it('caps the ext column so a pathological extension cannot dominate the row', () => {
    _setMeasureForTests(fakeMeasure)
    const longExt = 'extension-extension-extension-extension-extension'
    // Cap sample is "extensionxx" (11 chars × 7 = 77 px), text only, no chrome.
    const capped = computeFullListColumnWidths({
      ...baseArgs,
      entries: [entry({ name: `a.${longExt}` })],
    })
    // 11 × 7 = 77 measured + 2 px MEASUREMENT_SAFETY_PAD = 79.
    expect(capped.ext).toBe('extensionxx'.length * 7 + 2)
    // And: the cap doesn't shrink columns below what real shorter extensions deserve.
    const normal = computeFullListColumnWidths({
      ...baseArgs,
      entries: [entry({ name: 'a.js' })],
    })
    expect(capped.ext).toBeGreaterThan(normal.ext)
  })

  it('reserves no Ext width when showExtensionInName is on', () => {
    _setMeasureForTests(fakeMeasure)
    // A long extension would normally widen the Ext column; with the full name
    // folded into the Name column the Ext column is hidden, so its width is 0.
    const w = computeFullListColumnWidths({
      ...baseArgs,
      showExtensionInName: true,
      entries: [entry({ name: 'a.verylongext', size: 0, physicalSize: 0 })],
    })
    expect(w.ext).toBe(0)
    // Size and date are unaffected by the Ext column being hidden.
    expect(w.size).toBeGreaterThan(0)
    expect(w.date).toBeGreaterThan(0)
  })

  it('still reserves Ext width in the default split mode', () => {
    _setMeasureForTests(fakeMeasure)
    const w = computeFullListColumnWidths({
      ...baseArgs,
      showExtensionInName: false,
      entries: [entry({ name: 'a.verylongext' })],
    })
    expect(w.ext).toBeGreaterThan(0)
  })

  it('widens date column based on longest formatted date', () => {
    _setMeasureForTests(fakeMeasure)
    const short = computeFullListColumnWidths({
      ...baseArgs,
      formattedDate: () => stubDate('today'),
      entries: [entry({ name: 'a', modifiedAt: 1 })],
    })
    const long = computeFullListColumnWidths({
      ...baseArgs,
      formattedDate: () => stubDate('2026-12-31 23:59:59'),
      entries: [entry({ name: 'a', modifiedAt: 1 })],
    })
    expect(long.date).toBeGreaterThan(short.date)
  })

  it('widens the date column to fit the full formatted date', () => {
    _setMeasureForTests(fakeMeasure)
    const w = computeFullListColumnWidths({
      ...baseArgs,
      formattedDate: () => stubDate('2026-12-31 23:59'),
      entries: [entry({ name: 'a', modifiedAt: 1 })],
    })
    // "2026-12-31 23:59" = 16 chars × 7 = 112 + 2 px pad = 114. Beats MIN_DATE_WIDTH (70).
    expect(w.date).toBe(16 * 7 + 2)
  })

  it('uses the widest date across all rows', () => {
    _setMeasureForTests(fakeMeasure)
    let i = 0
    const formattedDate = () => {
      const texts = ['1/1 0:00', '2026-12-31 23:59']
      const text = texts[i % 2]
      i++
      return stubDate(text)
    }
    const w = computeFullListColumnWidths({
      ...baseArgs,
      formattedDate,
      entries: [entry({ name: 'a', modifiedAt: 1 }), entry({ name: 'b', modifiedAt: 2 })],
    })
    // Widest is "2026-12-31 23:59" = 16 × 7 = 112 + 2 px pad = 114.
    expect(w.date).toBe(16 * 7 + 2)
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

  it('reserves icon width when a directory is per-dir pending without global indexing', () => {
    _setMeasureForTests(fakeMeasure)
    const idle = computeFullListColumnWidths({
      ...baseArgs,
      entries: [entry({ name: 'd', isDirectory: true, recursiveSize: 12345 })],
    })
    const pending = computeFullListColumnWidths({
      ...baseArgs,
      indexing: false,
      entries: [entry({ name: 'd', isDirectory: true, recursiveSize: 12345, recursiveSizePending: true })],
    })
    expect(pending.size).toBeGreaterThanOrEqual(idle.size)
  })

  it('reserves icon width for a scanning directory with no size yet', () => {
    _setMeasureForTests(fakeMeasure)
    const idle = computeFullListColumnWidths({
      ...baseArgs,
      entries: [entry({ name: 'd', isDirectory: true, recursiveSize: undefined })],
    })
    const scanning = computeFullListColumnWidths({
      ...baseArgs,
      indexing: true,
      entries: [entry({ name: 'd', isDirectory: true, recursiveSize: undefined })],
    })
    // The `<dir>` placeholder text is the same in both, but the scanning row also
    // draws the hourglass, so its column must reserve the extra icon width.
    expect(scanning.size).toBeGreaterThan(idle.size)
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
        recursiveHasSymlinks: false,
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

  it('size column tracks the human-friendly format when enabled', () => {
    _setMeasureForTests(fakeMeasure)
    const big = entry({ name: 'z.bin', size: 123_456_789, physicalSize: 123_456_789 })
    const raw = computeFullListColumnWidths({ ...baseArgs, entries: [big] })
    const human = computeFullListColumnWidths({
      ...baseArgs,
      entries: [big],
      sizeFormatOpts: { unit: 'dynamic' as const, format: 'binary' as const },
    })
    // "123 456 789" (with thin spaces) is 11 visible chars; "117.74 MB" is 9.
    // With our deterministic length*7 measurer the human-friendly cell is narrower.
    expect(human.size).toBeLessThan(raw.size)
  })

  // Path column tests removed: the optional path column was dropped. The
  // search-results pane now shows full paths in the Name column
  // (mid-truncated via `useShortenMiddle`) instead.

  describe('comma-decimal locale (de-DE) size column', () => {
    afterEach(() => {
      _setLocaleForTests(null)
    })

    it('measures the localized size string the renderer produces (separators differ from ASCII)', () => {
      // The fake measurer is `length * 7`, so the measured width is a direct
      // proxy for the character count of the string the renderer emits. Render
      // and measure share `formatSizeForDisplay`, so the de-DE comma-decimal /
      // period-grouped string is what gets measured. Pin the size width to the
      // exact localized cell text + the safety pad, proving no second
      // ASCII-only formatting path sneaks in.
      _setMeasureForTests(fakeMeasure)
      _setLocaleForTests('de-DE')
      const big = entry({ name: 'z.bin', size: 1_073_208, physicalSize: 1_073_208 })
      const opts = { unit: 'dynamic' as const, format: 'binary' as const }

      // The exact cell string the renderer shows for this file under de-DE.
      const cell = formatSizeForDisplay(1_073_208, opts)
        .map((t) => t.value)
        .join('')
      expect(cell).toBe('1,02 MB') // comma decimal, ASCII space before the unit

      const w = computeFullListColumnWidths({ ...baseArgs, entries: [big], sizeFormatOpts: opts })
      // 7 chars × 7 px + 2 px MEASUREMENT_SAFETY_PAD. No clip (we measured the
      // real localized string), no over-reserve (no extra path widened it).
      expect(w.size).toBe(cell.length * 7 + 2)
    })

    it('raw-bytes triads use the locale group separator consistently in render and measure', () => {
      _setMeasureForTests(fakeMeasure)
      _setLocaleForTests('de-DE')
      const big = entry({ name: 'z.bin', size: 1_073_208, physicalSize: 1_073_208 })
      const opts = { unit: 'bytes' as const, format: 'binary' as const }

      const cell = formatSizeForDisplay(1_073_208, opts)
        .map((t) => t.value)
        .join('')
      expect(cell).toBe('1.073.208') // de-DE period grouping

      const w = computeFullListColumnWidths({ ...baseArgs, entries: [big], sizeFormatOpts: opts })
      expect(w.size).toBe(cell.length * 7 + 2)
    })

    it('keeps the size column consistent across locales for the same byte count (no drift)', () => {
      _setMeasureForTests(fakeMeasure)
      const big = entry({ name: 'z.bin', size: 1_073_208, physicalSize: 1_073_208 })
      const opts = { unit: 'bytes' as const, format: 'binary' as const }

      _setLocaleForTests('en-US')
      const enWidth = computeFullListColumnWidths({ ...baseArgs, entries: [big], sizeFormatOpts: opts }).size
      _setLocaleForTests('de-DE')
      const deWidth = computeFullListColumnWidths({ ...baseArgs, entries: [big], sizeFormatOpts: opts }).size

      // Same digit + separator count ("1,073,208" vs "1.073.208"), so the
      // shrink-wrapped width is identical: the separator swap doesn't drift it.
      expect(deWidth).toBe(enWidth)
    })
  })
})
