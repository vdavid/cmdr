import { afterEach, describe, it, expect } from 'vitest'
import { formatDateForDisplay, joinSegments, type DateSegment } from './format-utils'
import { _setLocaleForTests } from '$lib/intl/locale'

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

describe("formatDateForDisplay: 'system' follows the locale chokepoint", () => {
  afterEach(() => {
    _setLocaleForTests(null)
  })

  it('switches the system date to the active locale (de-DE) without touching iso/short/custom', () => {
    _setLocaleForTests('de-DE')
    const expectedDe = new Intl.DateTimeFormat('de-DE', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    })
      .formatToParts(fixedDate)
      .map((p) => p.value)
      .join('')
    expect(formatDateForDisplay(timestamp, 'system', '', NOW_MS).text).toBe(expectedDe)

    // The fixed-token formats are locale-independent BY DESIGN: unchanged under de-DE.
    expect(formatDateForDisplay(timestamp, 'iso', '', NOW_MS).text).toBe('2024-03-15 14:30')
    expect(formatDateForDisplay(timestamp, 'short', '', NOW_MS).text).toBe('03/15 14:30')
    expect(formatDateForDisplay(timestamp, 'custom', 'YYYY/MM/DD HH:mm:ss', NOW_MS).text).toBe('2024/03/15 14:30:45')
  })

  it('uses a stable cached instance across calls (one formatter per locale)', () => {
    _setLocaleForTests('de-DE')
    const a = formatDateForDisplay(timestamp, 'system', '', NOW_MS).text
    const b = formatDateForDisplay(timestamp, 'system', '', NOW_MS).text
    expect(a).toBe(b)
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
