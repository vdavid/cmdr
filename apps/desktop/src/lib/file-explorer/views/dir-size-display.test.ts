/**
 * Tests for directory size display logic in FullList.
 * Covers the display states and tooltip building for directories
 * with/without index data and scanning status.
 */
import { describe, it, expect, vi, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import {
  getDirSizeDisplayState,
  isDirSizeUpdating,
  buildDirSizeTooltip,
  getDisplaySize,
  buildFileSizeTooltip,
  hasSizeMismatch,
} from './full-list-utils'

// Mock settings store (required by full-list-utils)
vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn(() => 20),
}))

// The tooltip labels/counts now resolve through the i18n catalog; pin en-US so
// the asserted English strings (file/files, No files, Content:, On disk:) hold.
beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

// Test helpers
const formatSize = (bytes: number): string => `${String(bytes)} bytes`
const formatNum = (n: number): string => String(n)

/** Extracts the html from a tooltip result, or returns the string as-is */
function tooltipHtml(result: string | { html: string }): string {
  return typeof result === 'object' ? result.html : result
}

// ============================================================================
// getDirSizeDisplayState
// ============================================================================

describe('getDirSizeDisplayState', () => {
  // The CONTENT state is a pure function of {recursiveSize, complete, stale}.
  // The in-flux hourglass (indexing||pending) is ORTHOGONAL — see `isDirSizeUpdating`.

  it('returns "dir" when no size data (not enriched yet) and not updating', () => {
    expect(getDirSizeDisplayState(undefined)).toBe('dir')
    expect(getDirSizeDisplayState(null)).toBe('dir')
  })

  it('returns "scanning" when no size data and an update is in flight', () => {
    // 4th arg is the orthogonal `updating` flag (indexing||pending).
    expect(getDirSizeDisplayState(undefined, undefined, undefined, true)).toBe('scanning')
  })

  it('returns "unknown" (—) when incomplete and size is 0', () => {
    // The crux: an incomplete subtree with nothing known below renders `—`,
    // distinct from a genuinely-empty "0 bytes" folder.
    expect(getDirSizeDisplayState(0, false, false)).toBe('unknown')
  })

  it('returns "lower-bound" (≥) when incomplete and size > 0', () => {
    expect(getDirSizeDisplayState(1234, false, false)).toBe('lower-bound')
  })

  it('returns "size" when complete and fresh', () => {
    expect(getDirSizeDisplayState(1234, true, false)).toBe('size')
  })

  it('returns "size" (0 bytes) when complete, fresh, and size is 0 — genuinely empty', () => {
    // complete + size 0 is a KNOWN "0 bytes", NOT the unknown `—`.
    expect(getDirSizeDisplayState(0, true, false)).toBe('size')
  })

  it('returns "size-stale" when complete but stale (older epoch)', () => {
    expect(getDirSizeDisplayState(1234, true, true)).toBe('size-stale')
    // Genuinely-empty but stale is still a stale exact "0 bytes".
    expect(getDirSizeDisplayState(0, true, true)).toBe('size-stale')
  })

  it('treats absent complete/stale flags as exact + fresh (pre-honest-sizes callers)', () => {
    // A dir enriched before the flags exist (or a test fixture) defaults to the
    // exact, fresh rendering rather than masquerading as unknown.
    expect(getDirSizeDisplayState(1234)).toBe('size')
    expect(getDirSizeDisplayState(0)).toBe('size')
  })
})

describe('isDirSizeUpdating', () => {
  // Orthogonal in-flux hourglass: a full scan/aggregation is running OR this dir
  // has live writes in flight. Applies on TOP of any content state.
  it('is true when globally indexing', () => {
    expect(isDirSizeUpdating(true, false)).toBe(true)
  })

  it('is true when the per-dir pending flag is set', () => {
    expect(isDirSizeUpdating(false, true)).toBe(true)
  })

  it('is false when neither indexing nor pending', () => {
    expect(isDirSizeUpdating(false, false)).toBe(false)
    // Default (omitted) pending arg behaves as false.
    expect(isDirSizeUpdating(false)).toBe(false)
  })
})

// ============================================================================
// buildDirSizeTooltip
// ============================================================================

describe('buildDirSizeTooltip', () => {
  it('returns empty string when no data and not scanning', () => {
    expect(buildDirSizeTooltip(undefined, undefined, 0, 0, false, formatSize, formatNum)).toBe('')
  })

  it('returns the size-readiness hint when no data and scanning is active', () => {
    expect(buildDirSizeTooltip(undefined, undefined, 0, 0, true, formatSize, formatNum)).toBe(
      'Sizes appear as the scan progresses',
    )
  })

  it('returns HTML tooltip with size and counts when recursive size is available', () => {
    const result = buildDirSizeTooltip(1234, undefined, 10, 3, false, formatSize, formatNum)
    const html = tooltipHtml(result)
    expect(html).toContain('1234 bytes')
    expect(html).toContain('10 files')
    expect(html).toContain('3 folders')
    expect(html).toContain('<br>')
  })

  it('appends stale warning when scanning with existing data', () => {
    const result = buildDirSizeTooltip(1234, undefined, 10, 3, true, formatSize, formatNum)
    const html = tooltipHtml(result)
    expect(html).toContain('1234 bytes')
    expect(html).toContain('10 files')
    expect(html).toContain('Updating index')
  })

  it('uses singular form for 1 file', () => {
    const html = tooltipHtml(buildDirSizeTooltip(100, undefined, 1, 5, false, formatSize, formatNum))
    expect(html).toContain('1 file')
    expect(html).not.toContain('1 files')
  })

  it('uses singular form for 1 folder', () => {
    const html = tooltipHtml(buildDirSizeTooltip(100, undefined, 5, 1, false, formatSize, formatNum))
    expect(html).toContain('1 folder')
    expect(html).not.toContain('1 folders')
  })

  it('uses "No files" and "no folders" for zero counts', () => {
    const html = tooltipHtml(buildDirSizeTooltip(100, undefined, 0, 0, false, formatSize, formatNum))
    expect(html).toContain('No files')
    expect(html).toContain('no folders')
  })

  it('handles zero-size directory correctly', () => {
    const html = tooltipHtml(buildDirSizeTooltip(0, undefined, 0, 0, false, formatSize, formatNum))
    expect(html).toContain('0 bytes')
    expect(html).toContain('No files')
    expect(html).toContain('no folders')
  })

  it('handles zero-size directory while scanning', () => {
    const html = tooltipHtml(buildDirSizeTooltip(0, undefined, 0, 0, true, formatSize, formatNum))
    expect(html).toContain('0 bytes')
    expect(html).toContain('Updating index')
  })

  it('handles large file and folder counts', () => {
    const html = tooltipHtml(buildDirSizeTooltip(1_000_000_000, undefined, 50000, 1200, false, formatSize, formatNum))
    expect(html).toContain('1000000000 bytes')
    expect(html).toContain('50000 files')
    expect(html).toContain('1200 folders')
  })

  it('uses provided formatSize function', () => {
    const customFormat = (bytes: number): string => `${(bytes / 1024).toFixed(1)} KB`
    const html = tooltipHtml(buildDirSizeTooltip(2048, undefined, 3, 1, false, customFormat, formatNum))
    expect(html).toContain('2.0 KB')
  })

  it('always shows both sizes when physical is available', () => {
    const result = buildDirSizeTooltip(1000000, 1000005, 10, 3, false, formatSize, formatNum)
    const html = tooltipHtml(result)
    expect(html).toContain('Content:')
    expect(html).toContain('1000000 bytes')
    expect(html).toContain('On disk:')
    expect(html).toContain('1000005 bytes')
  })

  it('shows both sizes when physical differs significantly', () => {
    const result = buildDirSizeTooltip(1000000, 800000, 10, 3, false, formatSize, formatNum)
    const html = tooltipHtml(result)
    expect(html).toContain('Content:')
    expect(html).toContain('1000000 bytes')
    expect(html).toContain('On disk:')
    expect(html).toContain('800000 bytes')
    expect(html).toContain('<br>')
  })

  it('shows single size when physical is unavailable', () => {
    const html = tooltipHtml(buildDirSizeTooltip(1000000, undefined, 10, 3, false, formatSize, formatNum))
    expect(html).toContain('1000000 bytes')
    expect(html).not.toContain('Content:')
    expect(html).not.toContain('On disk:')
  })

  it('includes colored triad spans in HTML output', () => {
    const html = tooltipHtml(buildDirSizeTooltip(1234567, undefined, 5, 2, false, formatSize, formatNum))
    expect(html).toContain('class="size-')
  })

  // Honest-size state lines: the tooltip gains a one-line label per state.
  it('returns the unknown tooltip when incomplete and size is 0 (the — state)', () => {
    // No size breakdown — there's nothing known. complete=false, size=0.
    const result = buildDirSizeTooltip(0, undefined, 0, 0, false, formatSize, formatNum, false, false)
    expect(tooltipHtml(result)).toBe("Size unknown: this folder hasn't been scanned yet.")
  })

  it('appends the lower-bound line when incomplete and size > 0 (the ≥ state)', () => {
    const html = tooltipHtml(buildDirSizeTooltip(1234, undefined, 10, 3, false, formatSize, formatNum, false, false))
    expect(html).toContain('1234 bytes')
    expect(html).toContain('At least this much')
  })

  it('appends the stale line when complete but stale', () => {
    const html = tooltipHtml(buildDirSizeTooltip(1234, undefined, 10, 3, false, formatSize, formatNum, true, true))
    expect(html).toContain('1234 bytes')
    expect(html).toContain('from an earlier scan')
    // Stale and lower-bound are mutually exclusive content states.
    expect(html).not.toContain('At least this much')
  })

  it('shows a normal breakdown with no state line when complete and fresh', () => {
    const html = tooltipHtml(buildDirSizeTooltip(1234, undefined, 10, 3, false, formatSize, formatNum, true, false))
    expect(html).toContain('1234 bytes')
    expect(html).not.toContain('At least this much')
    expect(html).not.toContain('from an earlier scan')
  })

  it('treats absent complete/stale as exact + fresh (pre-honest-sizes callers)', () => {
    const html = tooltipHtml(buildDirSizeTooltip(1234, undefined, 10, 3, false, formatSize, formatNum))
    expect(html).toContain('1234 bytes')
    expect(html).not.toContain('At least this much')
  })
})

// ============================================================================
// getDisplaySize
// ============================================================================

describe('getDisplaySize', () => {
  it('returns logical in logical mode', () => {
    expect(getDisplaySize(100, 200, 'logical')).toBe(100)
  })

  it('returns physical in physical mode', () => {
    expect(getDisplaySize(100, 200, 'physical')).toBe(200)
  })

  it('returns min in smart mode when both available', () => {
    expect(getDisplaySize(100, 200, 'smart')).toBe(100)
    expect(getDisplaySize(200, 100, 'smart')).toBe(100)
  })

  it('returns logical when physical is undefined in smart mode', () => {
    expect(getDisplaySize(100, undefined, 'smart')).toBe(100)
  })

  it('returns physical when logical is undefined in smart mode', () => {
    expect(getDisplaySize(undefined, 200, 'smart')).toBe(200)
  })

  it('falls back to logical when physical is undefined in physical mode', () => {
    expect(getDisplaySize(100, undefined, 'physical')).toBe(100)
  })

  it('returns undefined when both are undefined', () => {
    expect(getDisplaySize(undefined, undefined, 'smart')).toBeUndefined()
    expect(getDisplaySize(undefined, undefined, 'logical')).toBeUndefined()
    expect(getDisplaySize(undefined, undefined, 'physical')).toBeUndefined()
  })
})

// ============================================================================
// hasSizeMismatch
// ============================================================================

describe('hasSizeMismatch', () => {
  it('returns false when logical is undefined', () => {
    expect(hasSizeMismatch(undefined, 500_000_000)).toBe(false)
  })

  it('returns false when physical is undefined', () => {
    expect(hasSizeMismatch(500_000_000, undefined)).toBe(false)
  })

  it('returns false when both are undefined', () => {
    expect(hasSizeMismatch(undefined, undefined)).toBe(false)
  })

  it('returns false when logical is zero', () => {
    expect(hasSizeMismatch(0, 500_000_000)).toBe(false)
  })

  it('returns false when physical is zero', () => {
    expect(hasSizeMismatch(500_000_000, 0)).toBe(false)
  })

  it('returns false when sizes are equal', () => {
    expect(hasSizeMismatch(1_000_000_000, 1_000_000_000)).toBe(false)
  })

  it('returns true when both thresholds are met (50% and 200 MB)', () => {
    // 600 MB logical, 300 MB physical → diff = 300 MB (>200 MB), 300/300 = 100% (>50%)
    expect(hasSizeMismatch(600_000_000, 300_000_000)).toBe(true)
    expect(hasSizeMismatch(300_000_000, 600_000_000)).toBe(true)
  })

  it('returns false when only percentage threshold is met but not absolute', () => {
    // 300 MB logical, 100 MB physical → diff = 200 MB, 200/100 = 200% (>50%)
    // BUT diff = 200 MB which is exactly 200_000_000, need >= so this is true
    // Use smaller values: 100 MB vs 50 MB → diff = 50 MB (<200 MB), 50/50 = 100% (>50%)
    expect(hasSizeMismatch(100_000_000, 50_000_000)).toBe(false)
  })

  it('returns false when only absolute threshold is met but not percentage', () => {
    // 1 GB logical, 800 MB physical → diff = 200 MB (>=200 MB), but 200/800 = 25% (<50%)
    expect(hasSizeMismatch(1_000_000_000, 800_000_000)).toBe(false)
  })

  it('returns true at exact boundary (200 MB diff, 50% relative)', () => {
    // 600 MB logical, 400 MB physical → diff = 200 MB (>=200 MB), 200/400 = 50% (>=50%)
    expect(hasSizeMismatch(600_000_000, 400_000_000)).toBe(true)
  })

  it('returns false just under the absolute boundary', () => {
    // 599_999_999 logical, 399_999_999 physical → diff = 200_000_000, 200M/400M = 50%
    // Actually that's at boundary. Use: diff = 199_999_999
    // 599_999_999 logical, 400_000_000 physical → diff = 199_999_999 (<200 MB)
    expect(hasSizeMismatch(599_999_999, 400_000_000)).toBe(false)
  })
})

// ============================================================================
// buildFileSizeTooltip
// ============================================================================

describe('buildFileSizeTooltip', () => {
  it('returns empty string when both sizes undefined', () => {
    expect(buildFileSizeTooltip(undefined, undefined, formatSize)).toBe('')
  })

  it('returns HTML tooltip when only logical is available', () => {
    const result = buildFileSizeTooltip(1024, undefined, formatSize)
    const html = tooltipHtml(result)
    expect(html).toContain('1024 bytes')
    expect(html).toContain('class="size-')
  })

  it('returns HTML tooltip when only physical is available', () => {
    const result = buildFileSizeTooltip(undefined, 2048, formatSize)
    const html = tooltipHtml(result)
    expect(html).toContain('2048 bytes')
  })

  it('always shows both sizes when both are available', () => {
    const result = buildFileSizeTooltip(1000000, 800000, formatSize)
    const html = tooltipHtml(result)
    expect(html).toContain('Content:')
    expect(html).toContain('1000000 bytes')
    expect(html).toContain('On disk:')
    expect(html).toContain('800000 bytes')
    expect(html).toContain('<br>')
  })

  it('shows both sizes even when sizes are similar', () => {
    const html = tooltipHtml(buildFileSizeTooltip(1000000, 1000005, formatSize))
    expect(html).toContain('Content:')
    expect(html).toContain('1000000 bytes')
    expect(html).toContain('On disk:')
    expect(html).toContain('1000005 bytes')
  })

  it('shows both sizes even when sizes are equal', () => {
    const html = tooltipHtml(buildFileSizeTooltip(500, 500, formatSize))
    expect(html).toContain('Content:')
    expect(html).toContain('On disk:')
  })
})
