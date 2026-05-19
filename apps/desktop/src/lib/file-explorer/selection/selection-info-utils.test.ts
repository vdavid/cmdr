/**
 * Tests for selection-info-utils.ts
 */
import { describe, it, expect } from 'vitest'
import {
  formatSizeTriads,
  formatSizeForDisplay,
  tierClassForUnit,
  formatDate,
  buildDateTooltip,
  getSizeDisplay,
  getDateDisplay,
  isBrokenSymlink,
  isPermissionDenied,
  sizeTierClasses,
  formatNumber,
  calculatePercentage,
} from './selection-info-utils'
import { formatDateForDisplay } from '$lib/settings/format-utils'
import type { FileEntry } from '../types'

// Helper to create a basic file entry
function createFileEntry(overrides: Partial<FileEntry> = {}): FileEntry {
  return {
    name: 'test.txt',
    path: '/test/test.txt',
    isDirectory: false,
    isSymlink: false,
    permissions: 0o644,
    owner: 'user',
    group: 'staff',
    iconId: 'file',
    extendedMetadataLoaded: true,
    ...overrides,
  }
}

describe('formatSizeTriads', () => {
  it('formats single digit', () => {
    const result = formatSizeTriads(5)
    expect(result).toHaveLength(1)
    expect(result[0].value).toBe('5')
    expect(result[0].tierClass).toBe('size-bytes')
  })

  it('formats two digits', () => {
    const result = formatSizeTriads(42)
    expect(result).toHaveLength(1)
    expect(result[0].value).toBe('42')
    expect(result[0].tierClass).toBe('size-bytes')
  })

  it('formats three digits (no separator needed)', () => {
    const result = formatSizeTriads(999)
    expect(result).toHaveLength(1)
    expect(result[0].value).toBe('999')
    expect(result[0].tierClass).toBe('size-bytes')
  })

  it('formats four digits (KB range)', () => {
    const result = formatSizeTriads(1234)
    expect(result).toHaveLength(2)
    expect(result[0].value).toBe('1\u2009') // with thin space separator
    expect(result[0].tierClass).toBe('size-kb')
    expect(result[1].value).toBe('234')
    expect(result[1].tierClass).toBe('size-bytes')
  })

  it('formats six digits', () => {
    const result = formatSizeTriads(123456)
    expect(result).toHaveLength(2)
    expect(result[0].value).toBe('123\u2009')
    expect(result[0].tierClass).toBe('size-kb')
    expect(result[1].value).toBe('456')
    expect(result[1].tierClass).toBe('size-bytes')
  })

  it('formats seven digits (MB range)', () => {
    const result = formatSizeTriads(1234567)
    expect(result).toHaveLength(3)
    expect(result[0].value).toBe('1\u2009')
    expect(result[0].tierClass).toBe('size-mb')
    expect(result[1].value).toBe('234\u2009')
    expect(result[1].tierClass).toBe('size-kb')
    expect(result[2].value).toBe('567')
    expect(result[2].tierClass).toBe('size-bytes')
  })

  it('formats ten digits (GB range)', () => {
    const result = formatSizeTriads(1234567890)
    expect(result).toHaveLength(4)
    expect(result[0].tierClass).toBe('size-gb')
    expect(result[1].tierClass).toBe('size-mb')
    expect(result[2].tierClass).toBe('size-kb')
    expect(result[3].tierClass).toBe('size-bytes')
  })

  it('formats thirteen digits (TB range)', () => {
    const result = formatSizeTriads(1234567890123)
    expect(result).toHaveLength(5)
    expect(result[0].tierClass).toBe('size-tb')
    expect(result[1].tierClass).toBe('size-gb')
  })

  it('caps at TB tier for very large numbers', () => {
    const result = formatSizeTriads(1234567890123456)
    expect(result).toHaveLength(6)
    // Both highest tiers should be size-tb (capped)
    expect(result[0].tierClass).toBe('size-tb')
    expect(result[1].tierClass).toBe('size-tb')
  })

  it('handles zero', () => {
    const result = formatSizeTriads(0)
    expect(result).toHaveLength(1)
    expect(result[0].value).toBe('0')
    expect(result[0].tierClass).toBe('size-bytes')
  })
})

describe('formatDate', () => {
  it('returns empty string for undefined', () => {
    expect(formatDate(undefined)).toBe('')
  })

  it('formats Unix timestamp correctly', () => {
    // Jan 15, 2024 12:30:45 UTC
    const timestamp = 1705322445
    const result = formatDate(timestamp)
    // Result depends on local timezone, but format should be YYYY-MM-DD HH:MM:SS
    expect(result).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/)
  })

  it('formats epoch timestamp', () => {
    const result = formatDate(0)
    expect(result).toMatch(/^1970-01-01/)
  })

  it('pads single digit months and days', () => {
    // Timestamp for a date with single digit month/day
    const timestamp = 1609459200 // Jan 1, 2021 00:00:00 UTC
    const result = formatDate(timestamp)
    expect(result).toContain('-01-')
  })
})

describe('buildDateTooltip', () => {
  // Stub formatter for tests: ISO with a year prefix so the year span is detectable,
  // a deterministic "now" so age tiers are predictable.
  const NOW_MS = Date.parse('2026-05-11T12:00:00Z')
  const fmt = (ts: number | null | undefined) => formatDateForDisplay(ts, 'iso', 'YYYY-MM-DD | HH:mm', NOW_MS)

  it('returns empty html when no dates are set', () => {
    const entry = createFileEntry()
    expect(buildDateTooltip(entry, fmt).html).toBe('')
  })

  it('includes labeled lines for each known date', () => {
    const entry = createFileEntry({
      createdAt: 1705322445,
      openedAt: 1705322445,
      addedAt: 1705322445,
      modifiedAt: 1705322445,
    })
    const html = buildDateTooltip(entry, fmt).html
    expect(html).toContain('Created:')
    expect(html).toContain('Last opened:')
    expect(html).toContain('Last moved ("added"):')
    expect(html).toContain('Last modified:')
    expect(html).toContain('<br>')
  })

  it('wraps colored segments in their age-tier spans', () => {
    // Today (same year/month/day as NOW_MS): year/month/day all fresh.
    const sameDay = Date.parse('2026-05-11T08:00:00Z') / 1000
    // Five years ago: year tier is age-old, no month/day/time coloring.
    const old = Date.parse('2021-05-11T08:00:00Z') / 1000
    const entry = createFileEntry({ createdAt: old, modifiedAt: sameDay })
    const html = buildDateTooltip(entry, fmt).html
    expect(html).toContain('class="age-old"')
    expect(html).toContain('class="age-fresh"')
  })
})

describe('getSizeDisplay', () => {
  it('returns null for null entry', () => {
    expect(getSizeDisplay(null, false, false)).toBeNull()
  })

  it('returns null for broken symlink', () => {
    const entry = createFileEntry({ size: 1000 })
    expect(getSizeDisplay(entry, true, false)).toBeNull()
  })

  it('returns null for permission denied', () => {
    const entry = createFileEntry({ size: 1000 })
    expect(getSizeDisplay(entry, false, true)).toBeNull()
  })

  it('returns DIR for directory', () => {
    const entry = createFileEntry({ isDirectory: true })
    expect(getSizeDisplay(entry, false, false)).toBe('DIR')
  })

  it('returns null for file with undefined size', () => {
    const entry = createFileEntry({ size: undefined })
    expect(getSizeDisplay(entry, false, false)).toBeNull()
  })

  it('returns formatted triads for file with size', () => {
    const entry = createFileEntry({ size: 1234567 })
    const result = getSizeDisplay(entry, false, false)
    expect(result).not.toBe('DIR')
    expect(result).not.toBeNull()
    expect(Array.isArray(result)).toBe(true)
  })
})

describe('getDateDisplay', () => {
  it('returns empty string for null entry', () => {
    expect(getDateDisplay(null, false, false)).toBe('')
  })

  it('returns broken symlink message', () => {
    const entry = createFileEntry()
    expect(getDateDisplay(entry, true, false)).toBe('(broken symlink)')
  })

  it('returns permission denied message', () => {
    const entry = createFileEntry()
    expect(getDateDisplay(entry, false, true)).toBe('(permission denied)')
  })

  it('returns formatted date for regular file', () => {
    const entry = createFileEntry({ modifiedAt: 1705322445 })
    const result = getDateDisplay(entry, false, false)
    expect(result).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/)
  })

  it('uses currentDirModifiedAt for parent entry', () => {
    const entry = createFileEntry({ name: '..', modifiedAt: 1000 })
    const result = getDateDisplay(entry, false, false, 2000)
    // Should use currentDirModifiedAt (2000) instead of entry.modifiedAt (1000)
    const resultWithoutOverride = getDateDisplay(entry, false, false)
    expect(result).not.toBe(resultWithoutOverride)
  })

  it('returns empty string when no modified time and not parent', () => {
    const entry = createFileEntry({ modifiedAt: undefined })
    expect(getDateDisplay(entry, false, false)).toBe('')
  })
})

describe('isBrokenSymlink', () => {
  it('returns false for null', () => {
    expect(isBrokenSymlink(null)).toBe(false)
  })

  it('returns false for regular file', () => {
    const entry = createFileEntry()
    expect(isBrokenSymlink(entry)).toBe(false)
  })

  it('returns false for valid symlink', () => {
    const entry = createFileEntry({ isSymlink: true, iconId: 'symlink' })
    expect(isBrokenSymlink(entry)).toBe(false)
  })

  it('returns true for broken symlink', () => {
    const entry = createFileEntry({ isSymlink: true, iconId: 'symlink-broken' })
    expect(isBrokenSymlink(entry)).toBe(true)
  })
})

describe('isPermissionDenied', () => {
  it('returns false for null', () => {
    expect(isPermissionDenied(null)).toBe(false)
  })

  it('returns false for regular file with permissions', () => {
    const entry = createFileEntry({ permissions: 0o644, size: 100 })
    expect(isPermissionDenied(entry)).toBe(false)
  })

  it('returns false for symlink even with no permissions', () => {
    const entry = createFileEntry({ isSymlink: true, permissions: 0, size: undefined })
    expect(isPermissionDenied(entry)).toBe(false)
  })

  it('returns true for non-symlink with no permissions and no size', () => {
    const entry = createFileEntry({ permissions: 0, size: undefined })
    expect(isPermissionDenied(entry)).toBe(true)
  })

  it('returns false for file with no permissions but has size', () => {
    const entry = createFileEntry({ permissions: 0, size: 100 })
    expect(isPermissionDenied(entry)).toBe(false)
  })
})

describe('sizeTierClasses', () => {
  it('has correct classes in order', () => {
    expect(sizeTierClasses).toEqual(['size-bytes', 'size-kb', 'size-mb', 'size-gb', 'size-tb'])
  })
})

describe('tierClassForUnit', () => {
  it('maps bytes to size-bytes', () => {
    expect(tierClassForUnit('bytes')).toBe('size-bytes')
  })

  it('maps KB and kB to size-kb', () => {
    expect(tierClassForUnit('KB')).toBe('size-kb')
    expect(tierClassForUnit('kB')).toBe('size-kb')
  })

  it('maps MB to size-mb', () => {
    expect(tierClassForUnit('MB')).toBe('size-mb')
  })

  it('maps GB to size-gb', () => {
    expect(tierClassForUnit('GB')).toBe('size-gb')
  })

  it('maps TB and PB to size-tb (capped)', () => {
    expect(tierClassForUnit('TB')).toBe('size-tb')
    expect(tierClassForUnit('PB')).toBe('size-tb')
  })
})

describe('formatSizeForDisplay', () => {
  describe('raw-bytes mode (humanFriendly: false)', () => {
    it('delegates to formatSizeTriads for small values', () => {
      const result = formatSizeForDisplay(512, { humanFriendly: false, format: 'binary' })
      expect(result).toEqual(formatSizeTriads(512))
    })

    it('delegates to formatSizeTriads for large values', () => {
      const result = formatSizeForDisplay(1_073_208, { humanFriendly: false, format: 'binary' })
      expect(result).toEqual(formatSizeTriads(1_073_208))
      // Sanity-check: matches user's example "1 073 208" (with thin spaces)
      expect(result.map((t) => t.value).join('')).toBe('1 073 208')
    })

    it('ignores the format option in raw-bytes mode', () => {
      const binary = formatSizeForDisplay(1024, { humanFriendly: false, format: 'binary' })
      const si = formatSizeForDisplay(1024, { humanFriendly: false, format: 'si' })
      expect(binary).toEqual(si)
    })
  })

  describe('human-friendly mode (humanFriendly: true)', () => {
    it('returns one element with size-bytes for sub-KB binary values', () => {
      const result = formatSizeForDisplay(512, { humanFriendly: true, format: 'binary' })
      expect(result).toHaveLength(1)
      expect(result[0]).toEqual({ value: '512 bytes', tierClass: 'size-bytes' })
    })

    it('returns size-kb for binary 1024', () => {
      const result = formatSizeForDisplay(1024, { humanFriendly: true, format: 'binary' })
      expect(result).toEqual([{ value: '1.00 KB', tierClass: 'size-kb' }])
    })

    it('returns size-mb for ~1 MB (matches feature spec example "1.02 MB")', () => {
      const result = formatSizeForDisplay(1_073_208, { humanFriendly: true, format: 'binary' })
      expect(result).toEqual([{ value: '1.02 MB', tierClass: 'size-mb' }])
    })

    it('returns size-gb for ~1 GB binary', () => {
      const result = formatSizeForDisplay(1024 ** 3, { humanFriendly: true, format: 'binary' })
      expect(result).toEqual([{ value: '1.00 GB', tierClass: 'size-gb' }])
    })

    it('returns size-tb for TB and beyond', () => {
      const tb = formatSizeForDisplay(1024 ** 4, { humanFriendly: true, format: 'binary' })
      const pb = formatSizeForDisplay(1024 ** 5, { humanFriendly: true, format: 'binary' })
      expect(tb[0].tierClass).toBe('size-tb')
      expect(pb[0].tierClass).toBe('size-tb')
    })

    it('boundary: SI 1000 is 1.00 kB (size-kb tier)', () => {
      const result = formatSizeForDisplay(1000, { humanFriendly: true, format: 'si' })
      expect(result).toEqual([{ value: '1.00 kB', tierClass: 'size-kb' }])
    })

    it('boundary: binary 1023 is still bytes', () => {
      const result = formatSizeForDisplay(1023, { humanFriendly: true, format: 'binary' })
      expect(result).toEqual([{ value: '1023 bytes', tierClass: 'size-bytes' }])
    })

    it('SI 1024 is 1.02 kB', () => {
      const result = formatSizeForDisplay(1024, { humanFriendly: true, format: 'si' })
      expect(result).toEqual([{ value: '1.02 kB', tierClass: 'size-kb' }])
    })
  })
})

// ============================================================================
// Selection summary utility tests
// ============================================================================

describe('formatNumber', () => {
  it('formats small numbers without separators', () => {
    expect(formatNumber(0)).toBe('0')
    expect(formatNumber(1)).toBe('1')
    expect(formatNumber(999)).toBe('999')
  })

  it('formats thousands with comma separator', () => {
    expect(formatNumber(1000)).toBe('1,000')
    expect(formatNumber(1234)).toBe('1,234')
    expect(formatNumber(9999)).toBe('9,999')
  })

  it('formats millions with multiple comma separators', () => {
    expect(formatNumber(1000000)).toBe('1,000,000')
    expect(formatNumber(1234567)).toBe('1,234,567')
  })

  it('formats large numbers correctly', () => {
    expect(formatNumber(1234567890)).toBe('1,234,567,890')
  })
})

describe('calculatePercentage', () => {
  it('returns 0 when total is 0', () => {
    expect(calculatePercentage(0, 0)).toBe(0)
    expect(calculatePercentage(100, 0)).toBe(0)
  })

  it('returns 0 for 0 of n', () => {
    expect(calculatePercentage(0, 100)).toBe(0)
    expect(calculatePercentage(0, 1000)).toBe(0)
  })

  it('returns 100 for n of n', () => {
    expect(calculatePercentage(100, 100)).toBe(100)
    expect(calculatePercentage(1, 1)).toBe(100)
  })

  it('calculates correct percentage', () => {
    expect(calculatePercentage(50, 100)).toBe(50)
    expect(calculatePercentage(25, 100)).toBe(25)
    expect(calculatePercentage(1, 4)).toBe(25)
    expect(calculatePercentage(1, 3)).toBe(33) // rounds down
    expect(calculatePercentage(2, 3)).toBe(67) // rounds up
  })

  it('rounds to nearest integer', () => {
    expect(calculatePercentage(1, 6)).toBe(17) // 16.67 -> 17
    expect(calculatePercentage(1, 7)).toBe(14) // 14.29 -> 14
    expect(calculatePercentage(5, 6)).toBe(83) // 83.33 -> 83
  })
})
