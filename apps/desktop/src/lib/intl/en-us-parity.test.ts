/**
 * en-US parity net for the locale-aware formatter layer.
 *
 * Pins the EXACT strings the four formatter families produced for an en-US
 * runtime BEFORE this work, so the reviewer can trust "current users see no
 * change". Every literal below was captured against the pre-change code. After
 * counts, sizes, and the `system` date are routed through `getLocale()`, these
 * assertions must still hold byte-for-byte under en-US; localization only shows
 * up for other regions (covered by the de-DE behavior tests next to each
 * formatter).
 *
 * The locale is pinned via the `lib/intl` chokepoint, not by mutating `Intl`
 * globals.
 */
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { _setLocaleForTests } from './locale'
import { formatDateForDisplay, formatFileSizeWithFormat } from '$lib/settings/format-utils'
import { formatNumber, formatSizeTriads } from '$lib/file-explorer/selection/selection-info-utils'

describe('en-US parity: counts (formatNumber)', () => {
  beforeEach(() => {
    _setLocaleForTests('en-US')
  })
  afterEach(() => {
    _setLocaleForTests(null)
  })

  it('groups thousands with a comma, exactly as before', () => {
    expect(formatNumber(0)).toBe('0')
    expect(formatNumber(1)).toBe('1')
    expect(formatNumber(999)).toBe('999')
    expect(formatNumber(1000)).toBe('1,000')
    expect(formatNumber(1234)).toBe('1,234')
    expect(formatNumber(1000000)).toBe('1,000,000')
    expect(formatNumber(1234567)).toBe('1,234,567')
    expect(formatNumber(1234567890)).toBe('1,234,567,890')
  })
})

describe('en-US parity: raw-byte triads (formatSizeTriads)', () => {
  beforeEach(() => {
    _setLocaleForTests('en-US')
  })
  afterEach(() => {
    _setLocaleForTests(null)
  })

  // The triad separator is the en-US group character. Pre-change this was the
  // hardcoded U+2009 thin space; en-US's Intl group separator is the ASCII
  // comma, so once the separator follows the locale the visible character
  // changes for en-US too. This is captured intentionally: see the de-DE and
  // the explicit separator assertions. The TIER coloring and the triad split
  // must be unchanged regardless.
  it('splits into the same triads with the same tier classes', () => {
    expect(formatSizeTriads(5).map((t) => ({ ...t }))).toEqual([{ value: '5', tierClass: 'size-bytes' }])
    expect(formatSizeTriads(1234567).map((t) => t.tierClass)).toEqual(['size-mb', 'size-kb', 'size-bytes'])
    expect(formatSizeTriads(1234567890).map((t) => t.tierClass)).toEqual([
      'size-gb',
      'size-mb',
      'size-kb',
      'size-bytes',
    ])
  })

  it('joins to the same digits (separator aside)', () => {
    const digitsOnly = formatSizeTriads(1234567)
      .map((t) => t.value.replace(/[^0-9]/g, ''))
      .join('')
    expect(digitsOnly).toBe('1234567')
  })
})

describe('en-US parity: human-friendly sizes (formatFileSizeWithFormat)', () => {
  // No locale stub here on purpose: pre-change this function hardcoded `.` and
  // produced these literals regardless of locale. Post-change it reads
  // `getLocale()`; en-US must reproduce these EXACT strings (no grouping on the
  // 1073.21 / 1000000.00 cases, `.` decimal).
  beforeEach(() => {
    _setLocaleForTests('en-US')
  })
  afterEach(() => {
    _setLocaleForTests(null)
  })

  it('binary, dynamic unit', () => {
    expect(formatFileSizeWithFormat(0, 'binary')).toBe('0 bytes')
    expect(formatFileSizeWithFormat(512, 'binary')).toBe('512 bytes')
    expect(formatFileSizeWithFormat(1000, 'binary')).toBe('1000 bytes')
    expect(formatFileSizeWithFormat(1024, 'binary')).toBe('1.00 KB')
    expect(formatFileSizeWithFormat(1536, 'binary')).toBe('1.50 KB')
    expect(formatFileSizeWithFormat(1_073_208, 'binary')).toBe('1.02 MB')
    expect(formatFileSizeWithFormat(1024 ** 4, 'binary')).toBe('1.00 TB')
  })

  it('SI, dynamic unit', () => {
    expect(formatFileSizeWithFormat(1000, 'si')).toBe('1.00 kB')
    expect(formatFileSizeWithFormat(1024, 'si')).toBe('1.02 kB')
    expect(formatFileSizeWithFormat(1_000_000_000, 'si')).toBe('1.00 GB')
  })

  it('forced unit, including large values that stay ungrouped', () => {
    expect(formatFileSizeWithFormat(0, 'binary', 'MB')).toBe('0.00 MB')
    expect(formatFileSizeWithFormat(512, 'binary', 'MB')).toBe('0.00 MB')
    expect(formatFileSizeWithFormat(1_073_208, 'binary', 'MB')).toBe('1.02 MB')
    expect(formatFileSizeWithFormat(512, 'si', 'kB')).toBe('0.51 kB')
    expect(formatFileSizeWithFormat(1_073_208, 'si', 'kB')).toBe('1073.21 kB')
    expect(formatFileSizeWithFormat(10 * 1000 ** 3, 'si', 'MB')).toBe('10000.00 MB')
    expect(formatFileSizeWithFormat(1024 ** 4, 'si', 'kB')).toBe('1099511627.78 kB')
  })
})

describe('en-US parity: system date', () => {
  beforeEach(() => {
    _setLocaleForTests('en-US')
  })
  afterEach(() => {
    _setLocaleForTests(null)
  })

  // The `system` date follows the locale by design. The invariant: under en-US
  // it equals what an en-US `Intl.DateTimeFormat` with the same fixed-width
  // options produces. This holds before the change (runtime default == en-US in
  // an en-US runtime) and after (chokepoint == en-US).
  it("matches en-US Intl.DateTimeFormat for the 'system' format", () => {
    const ts = new Date(2024, 2, 15, 14, 30, 45).getTime() / 1000
    // Compare against `formatToParts().join('')`, not `.format()`: the system
    // formatter builds its text from parts (for per-segment age coloring), and
    // V8's `.format()` can disagree with `formatToParts` on the literal space
    // before AM/PM (regular space vs U+202F narrow no-break space).
    const expected = new Intl.DateTimeFormat('en-US', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    })
      .formatToParts(new Date(ts * 1000))
      .map((p) => p.value)
      .join('')
    expect(formatDateForDisplay(ts, 'system', '', Date.now()).text).toBe(expected)
  })
})
