import { describe, it, expect } from 'vitest'
import {
  formatDateForDisplay,
  formatFileSizeWithFormat,
  fixedUnitFor,
  dynamicTierIndex,
  joinSegments,
  unitLabel,
  type DateSegment,
} from './format-utils'

// Fixed timestamp: March 15, 2024 14:30:45 local (local date to avoid timezone flakiness).
const fixedDate = new Date(2024, 2, 15, 14, 30, 45)
const timestamp = fixedDate.getTime() / 1000

// Anchor "now" 1 day after fixedDate. With this anchor:
// - same year + month + day → time tier for HH/mm/ss
// - year tier → fresh
const NOW_MS = fixedDate.getTime() + 24 * 60 * 60 * 1000

// Anchor far enough in the future that the year jumps to "3+ ago" → age-old.
const FAR_NOW_MS = new Date(2030, 5, 1).getTime()

/** Convenience: find the first segment whose text equals `text`. */
function find(segments: DateSegment[], text: string): DateSegment | undefined {
  return segments.find((s) => s.text === text)
}

describe('formatDateForDisplay: text', () => {
  it('returns empty result for undefined/null/zero timestamps', () => {
    for (const t of [undefined, null, 0]) {
      const d = formatDateForDisplay(t, 'iso', '', NOW_MS)
      expect(d.text).toBe('')
      expect(d.segments).toEqual([])
    }
  })

  it('formats as ISO (YYYY-MM-DD HH:mm)', () => {
    expect(formatDateForDisplay(timestamp, 'iso', '', NOW_MS).text).toBe('2024-03-15 14:30')
  })

  it('formats as short (MM/DD HH:mm)', () => {
    expect(formatDateForDisplay(timestamp, 'short', '', NOW_MS).text).toBe('03/15 14:30')
  })

  it('formats with a custom format string', () => {
    expect(formatDateForDisplay(timestamp, 'custom', 'YYYY/MM/DD HH:mm:ss', NOW_MS).text).toBe('2024/03/15 14:30:45')
  })

  it('handles custom formats with partial tokens', () => {
    expect(formatDateForDisplay(timestamp, 'custom', 'YYYY-MM', NOW_MS).text).toBe('2024-03')
  })

  it('falls back to ISO for unknown format modes', () => {
    expect(formatDateForDisplay(timestamp, 'unknown' as never, '', NOW_MS).text).toBe('2024-03-15 14:30')
  })

  it('produces a non-empty system-locale text', () => {
    expect(formatDateForDisplay(timestamp, 'system', '', NOW_MS).text.length).toBeGreaterThan(0)
  })
})

describe('formatDateForDisplay: segments (iso)', () => {
  it('emits year/month/day/time as one segment list with literals between', () => {
    const d = formatDateForDisplay(timestamp, 'iso', '', NOW_MS)
    expect(d.segments.map((s) => s.text)).toEqual(['2024', '-', '03', '-', '15', ' ', '14', ':', '30']) // Literals never carry an age class.
    for (const lit of ['-', ':', ' ']) {
      for (const seg of d.segments.filter((s) => s.text === lit)) expect(seg.ageClass).toBeNull()
    }
  })

  it('joins back to the plain string via joinSegments', () => {
    const d = formatDateForDisplay(timestamp, 'iso', '', NOW_MS)
    expect(joinSegments(d.segments)).toBe(d.text)
  })
})

describe('formatDateForDisplay: segments (short)', () => {
  it('omits year and includes day + time segments in one list', () => {
    const d = formatDateForDisplay(timestamp, 'short', '', NOW_MS)
    expect(d.segments.map((s) => s.text)).toEqual(['03', '/', '15', ' ', '14', ':', '30'])
  })
})

describe('formatDateForDisplay: segments (custom)', () => {
  it('finds tokens in any order in custom formats', () => {
    const d = formatDateForDisplay(timestamp, 'custom', 'DD/MM/YYYY HH:mm', NOW_MS)
    expect(d.segments.map((s) => s.text)).toEqual(['15', '/', '03', '/', '2024', ' ', '14', ':', '30'])
  })

  it('handles repeated tokens: each occurrence becomes its own segment', () => {
    const d = formatDateForDisplay(timestamp, 'custom', 'YYYY YYYY', NOW_MS)
    expect(d.segments.map((s) => s.text)).toEqual(['2024', ' ', '2024'])
    // Both year segments share the same tier (whatever year tier the timestamp produces).
    expect(d.segments[0].ageClass).toBe(d.segments[2].ageClass)
  })

  it('renders the full custom format as one segment list', () => {
    const d = formatDateForDisplay(timestamp, 'custom', 'YYYY/MM/DD HH:mm:ss', NOW_MS)
    expect(joinSegments(d.segments)).toBe('2024/03/15 14:30:45')
  })
})

describe('formatDateForDisplay: segments (system)', () => {
  it('uses Intl.formatToParts and classifies each part structurally', () => {
    const d = formatDateForDisplay(timestamp, 'system', '', NOW_MS)
    // The locale shape varies (en-US may emit a 2-digit year, sv-SE 4-digit);
    // what matters is that the year segment carries the year tier (fresh under
    // our anchor) and the joined text round-trips. We locate the year part by
    // matching the year value the formatter actually emitted.
    expect(joinSegments(d.segments)).toBe(d.text)
    const yearSeg = d.segments.find((s) => s.ageClass === 'age-fresh' && /\d{2,4}/.test(s.text))
    expect(yearSeg).toBeDefined()
  })
})

describe('formatDateForDisplay: per-component ageClass', () => {
  it('colors year, month, day, time as fresh when the file is "today" relative to now', () => {
    // The timestamp is 2024-03-15 14:30:45 local; NOW_MS is 2024-03-16 14:30:45.
    // Year matches (fresh), month matches (fresh), day differs by one (recent).
    const d = formatDateForDisplay(timestamp, 'iso', '', NOW_MS)
    expect(find(d.segments, '2024')?.ageClass).toBe('age-fresh')
    expect(find(d.segments, '03')?.ageClass).toBe('age-fresh')
    expect(find(d.segments, '15')?.ageClass).toBe('age-recent')
    // Day differs → time gets null (only colored when same date as now).
    expect(find(d.segments, '14')?.ageClass).toBeNull()
    expect(find(d.segments, '30')?.ageClass).toBeNull()
  })

  it('drops month/day/time coloring when the year differs from now', () => {
    const d = formatDateForDisplay(timestamp, 'iso', '', FAR_NOW_MS)
    // 2024 vs 2030 → 6 years back → age-old for year, null for month/day/time.
    expect(find(d.segments, '2024')?.ageClass).toBe('age-old')
    expect(find(d.segments, '03')?.ageClass).toBeNull()
    expect(find(d.segments, '15')?.ageClass).toBeNull()
    expect(find(d.segments, '14')?.ageClass).toBeNull()
  })

  it('colors time when timestamp is the same date as now', () => {
    // Build a "now" on the same date as `fixedDate` (14:30:45) but ~1.5 hours
    // later (16:15:00) → floor(distance in hours) = 1 → age-recent for the
    // HH/mm/ss segments.
    const sameDayNowMs = new Date(2024, 2, 15, 16, 15, 0).getTime()
    const d = formatDateForDisplay(timestamp, 'iso', '', sameDayNowMs)
    expect(find(d.segments, '14')?.ageClass).toBe('age-recent')
  })

  it('returns no segments for null/zero timestamps', () => {
    const d = formatDateForDisplay(null, 'iso', '', NOW_MS)
    expect(d.segments).toEqual([])
  })
})

describe('formatFileSizeWithFormat', () => {
  describe('binary (base 1024)', () => {
    it('formats 0 bytes', () => {
      expect(formatFileSizeWithFormat(0, 'binary')).toBe('0 bytes')
    })

    it('formats bytes below 1 KB', () => {
      expect(formatFileSizeWithFormat(512, 'binary')).toBe('512 bytes')
    })

    it('formats exactly 1 KB', () => {
      expect(formatFileSizeWithFormat(1024, 'binary')).toBe('1.00 KB')
    })

    it('formats megabytes', () => {
      expect(formatFileSizeWithFormat(1024 * 1024, 'binary')).toBe('1.00 MB')
    })

    it('formats gigabytes', () => {
      expect(formatFileSizeWithFormat(1024 ** 3, 'binary')).toBe('1.00 GB')
    })

    it('formats terabytes', () => {
      expect(formatFileSizeWithFormat(1024 ** 4, 'binary')).toBe('1.00 TB')
    })

    it('formats petabytes', () => {
      expect(formatFileSizeWithFormat(1024 ** 5, 'binary')).toBe('1.00 PB')
    })

    it('caps at PB for very large values', () => {
      const result = formatFileSizeWithFormat(1024 ** 6, 'binary')
      expect(result).toBe('1024.00 PB')
    })

    it('formats fractional KB values', () => {
      expect(formatFileSizeWithFormat(1536, 'binary')).toBe('1.50 KB')
    })
  })

  describe('SI (base 1000)', () => {
    it('formats 0 bytes', () => {
      expect(formatFileSizeWithFormat(0, 'si')).toBe('0 bytes')
    })

    it('formats bytes below 1 kB', () => {
      expect(formatFileSizeWithFormat(999, 'si')).toBe('999 bytes')
    })

    it('formats exactly 1 kB', () => {
      expect(formatFileSizeWithFormat(1000, 'si')).toBe('1.00 kB')
    })

    it('formats megabytes', () => {
      expect(formatFileSizeWithFormat(1_000_000, 'si')).toBe('1.00 MB')
    })

    it('formats gigabytes', () => {
      expect(formatFileSizeWithFormat(1_000_000_000, 'si')).toBe('1.00 GB')
    })

    it('uses lowercase k for SI kilo', () => {
      expect(formatFileSizeWithFormat(1500, 'si')).toBe('1.50 kB')
    })
  })

  describe('boundary between binary and SI', () => {
    it('1024 bytes is 1.02 kB in SI', () => {
      expect(formatFileSizeWithFormat(1024, 'si')).toBe('1.02 kB')
    })

    it('1000 bytes is still bytes in binary', () => {
      expect(formatFileSizeWithFormat(1000, 'binary')).toBe('1000 bytes')
    })
  })

  describe('forced unit (kB / MB / GB)', () => {
    it("'kB' under binary renders 'KB' uppercase with 1024-based math", () => {
      expect(formatFileSizeWithFormat(2048, 'binary', 'kB')).toBe('2.00 KB')
    })

    it("'kB' under SI renders 'kB' lowercase k with 1000-based math", () => {
      expect(formatFileSizeWithFormat(2000, 'si', 'kB')).toBe('2.00 kB')
    })

    it("'MB' under binary on 1 MiB returns '1.00 MB'", () => {
      expect(formatFileSizeWithFormat(1024 ** 2, 'binary', 'MB')).toBe('1.00 MB')
    })

    it("'GB' under SI on 2 GB returns '2.00 GB'", () => {
      expect(formatFileSizeWithFormat(2 * 1000 ** 3, 'si', 'GB')).toBe('2.00 GB')
    })

    it("forced kB on a sub-KB value renders fractional ('0.50 KB' binary)", () => {
      expect(formatFileSizeWithFormat(512, 'binary', 'kB')).toBe('0.50 KB')
    })

    it("forced MB doesn't roll over to GB even on 10+ GB inputs", () => {
      const tenGB = 10 * 1000 ** 3
      expect(formatFileSizeWithFormat(tenGB, 'si', 'MB')).toBe('10000.00 MB')
    })
  })
})

describe('unitLabel', () => {
  it("'kB' becomes 'KB' under binary", () => {
    expect(unitLabel('kB', 'binary')).toBe('KB')
  })

  it("'kB' stays 'kB' under SI", () => {
    expect(unitLabel('kB', 'si')).toBe('kB')
  })

  it("'MB' is the same in binary and SI", () => {
    expect(unitLabel('MB', 'binary')).toBe('MB')
    expect(unitLabel('MB', 'si')).toBe('MB')
  })

  it("'GB' is the same in binary and SI", () => {
    expect(unitLabel('GB', 'binary')).toBe('GB')
    expect(unitLabel('GB', 'si')).toBe('GB')
  })
})

describe('dynamicTierIndex', () => {
  it('returns 0 (bytes) for sub-base values', () => {
    expect(dynamicTierIndex(0, 'binary')).toBe(0)
    expect(dynamicTierIndex(999, 'si')).toBe(0)
    expect(dynamicTierIndex(1023, 'binary')).toBe(0)
  })

  it('returns 1 (kB) for kilobyte range', () => {
    expect(dynamicTierIndex(1024, 'binary')).toBe(1)
    expect(dynamicTierIndex(1000, 'si')).toBe(1)
    expect(dynamicTierIndex(500_000, 'si')).toBe(1)
  })

  it('returns 2 (MB) for megabyte range', () => {
    expect(dynamicTierIndex(1024 ** 2, 'binary')).toBe(2)
    expect(dynamicTierIndex(5_000_000, 'si')).toBe(2)
  })

  it('returns 3 (GB) for gigabyte range', () => {
    expect(dynamicTierIndex(1024 ** 3, 'binary')).toBe(3)
    expect(dynamicTierIndex(10 * 1000 ** 3, 'si')).toBe(3)
  })

  it('caps at 4 (TB-tier) for TB and beyond', () => {
    expect(dynamicTierIndex(1024 ** 4, 'binary')).toBe(4)
    expect(dynamicTierIndex(1024 ** 5, 'binary')).toBe(4)
    expect(dynamicTierIndex(1024 ** 6, 'binary')).toBe(4)
  })

  it('respects the binary/SI base boundary (1000 bytes is sub-base in binary)', () => {
    expect(dynamicTierIndex(1000, 'binary')).toBe(0)
    expect(dynamicTierIndex(1000, 'si')).toBe(1)
  })
})

describe('fixedUnitFor', () => {
  it("returns null for 'dynamic'", () => {
    expect(fixedUnitFor('dynamic')).toBeNull()
  })

  it("returns null for 'bytes' (raw-byte path is not a forced unit)", () => {
    expect(fixedUnitFor('bytes')).toBeNull()
  })

  it('returns the same token for fixed unit values', () => {
    expect(fixedUnitFor('kB')).toBe('kB')
    expect(fixedUnitFor('MB')).toBe('MB')
    expect(fixedUnitFor('GB')).toBe('GB')
  })
})
