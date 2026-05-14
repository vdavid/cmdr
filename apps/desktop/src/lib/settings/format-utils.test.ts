import { describe, it, expect } from 'vitest'
import { formatDateForDisplay, formatFileSizeWithFormat, joinSegments, type DateSegment } from './format-utils'

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
      expect(d.parts.left).toEqual([])
      expect(d.parts.right).toBeNull()
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
  it('splits ISO into year/month/day on the left, hour/minute on the right with literals between', () => {
    const d = formatDateForDisplay(timestamp, 'iso', '', NOW_MS)
    expect(d.parts.left.map((s) => s.text)).toEqual(['2024', '-', '03', '-', '15'])
    expect(d.parts.right?.map((s) => s.text)).toEqual(['14', ':', '30'])
    // Literals never carry an age class.
    for (const lit of ['-', ':']) {
      for (const seg of d.parts.left.filter((s) => s.text === lit)) expect(seg.ageClass).toBeNull()
    }
  })

  it('joins back to the plain string via joinSegments', () => {
    const d = formatDateForDisplay(timestamp, 'iso', '', NOW_MS)
    const joined =
      d.parts.right === null
        ? joinSegments(d.parts.left)
        : `${joinSegments(d.parts.left)} ${joinSegments(d.parts.right)}`
    expect(joined).toBe(d.text)
  })
})

describe('formatDateForDisplay: segments (short)', () => {
  it('omits year and includes day + time segments', () => {
    const d = formatDateForDisplay(timestamp, 'short', '', NOW_MS)
    expect(d.parts.left.map((s) => s.text)).toEqual(['03', '/', '15'])
    expect(d.parts.right?.map((s) => s.text)).toEqual(['14', ':', '30'])
  })
})

describe('formatDateForDisplay: segments (custom)', () => {
  it('finds tokens in any order in custom formats', () => {
    const d = formatDateForDisplay(timestamp, 'custom', 'DD/MM/YYYY | HH:mm', NOW_MS)
    expect(d.parts.left.map((s) => s.text)).toEqual(['15', '/', '03', '/', '2024'])
    expect(d.parts.right?.map((s) => s.text)).toEqual(['14', ':', '30'])
  })

  it('handles repeated tokens: each occurrence becomes its own segment', () => {
    const d = formatDateForDisplay(timestamp, 'custom', 'YYYY YYYY', NOW_MS)
    expect(d.parts.left.map((s) => s.text)).toEqual(['2024', ' ', '2024'])
    // Both year segments share the same tier (whatever year tier the timestamp produces).
    expect(d.parts.left[0].ageClass).toBe(d.parts.left[2].ageClass)
  })

  it('handles a custom format with `|` and trimmed whitespace', () => {
    const d = formatDateForDisplay(timestamp, 'custom', 'YYYY/MM/DD | HH:mm:ss', NOW_MS)
    expect(joinSegments(d.parts.left)).toBe('2024/03/15')
    expect(d.parts.right && joinSegments(d.parts.right)).toBe('14:30:45')
  })

  it('treats a degenerate `format |` (empty right) as no split', () => {
    const d = formatDateForDisplay(timestamp, 'custom', 'YYYY-MM-DD |', NOW_MS)
    expect(joinSegments(d.parts.left)).toBe('2024-03-15')
    expect(d.parts.right).toBeNull()
  })
})

describe('formatDateForDisplay: segments (system)', () => {
  it('uses Intl.formatToParts and classifies each part structurally', () => {
    const d = formatDateForDisplay(timestamp, 'system', '', NOW_MS)
    // The locale shape varies (en-US may emit a 2-digit year, sv-SE 4-digit);
    // what matters is that the year segment carries the year tier (fresh under
    // our anchor) and the joined text round-trips. We locate the year part by
    // matching the year value the formatter actually emitted.
    expect(joinSegments(d.parts.left)).toBe(d.text)
    const yearSeg = d.parts.left.find((s) => s.ageClass === 'age-fresh' && /\d{2,4}/.test(s.text))
    expect(yearSeg).toBeDefined()
  })
})

describe('formatDateForDisplay: per-component ageClass', () => {
  it('colors year, month, day, time as fresh when the file is "today" relative to now', () => {
    // The timestamp is 2024-03-15 14:30:45 local; NOW_MS is 2024-03-16 14:30:45.
    // Year matches (fresh), month matches (fresh), day differs by one (recent).
    const d = formatDateForDisplay(timestamp, 'iso', '', NOW_MS)
    expect(find(d.parts.left, '2024')?.ageClass).toBe('age-fresh')
    expect(find(d.parts.left, '03')?.ageClass).toBe('age-fresh')
    expect(find(d.parts.left, '15')?.ageClass).toBe('age-recent')
    // Day differs → time gets null (only colored when same date as now).
    expect(find(d.parts.right ?? [], '14')?.ageClass).toBeNull()
    expect(find(d.parts.right ?? [], '30')?.ageClass).toBeNull()
  })

  it('drops month/day/time coloring when the year differs from now', () => {
    const d = formatDateForDisplay(timestamp, 'iso', '', FAR_NOW_MS)
    // 2024 vs 2030 → 6 years back → age-old for year, null for month/day/time.
    expect(find(d.parts.left, '2024')?.ageClass).toBe('age-old')
    expect(find(d.parts.left, '03')?.ageClass).toBeNull()
    expect(find(d.parts.left, '15')?.ageClass).toBeNull()
    expect(find(d.parts.right ?? [], '14')?.ageClass).toBeNull()
  })

  it('colors time when timestamp is the same date as now', () => {
    // Build a "now" on the same date as `fixedDate` (14:30:45) but ~1.5 hours
    // later (16:15:00) → floor(distance in hours) = 1 → age-recent for the
    // HH/mm/ss segments.
    const sameDayNowMs = new Date(2024, 2, 15, 16, 15, 0).getTime()
    const d = formatDateForDisplay(timestamp, 'iso', '', sameDayNowMs)
    expect(find(d.parts.right ?? [], '14')?.ageClass).toBe('age-recent')
  })

  it('returns no segments for null/zero timestamps', () => {
    const d = formatDateForDisplay(null, 'iso', '', NOW_MS)
    expect(d.parts.left).toEqual([])
    expect(d.parts.right).toBeNull()
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
})
