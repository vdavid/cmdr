/**
 * Duration and file-rate formatting. Byte sizes and rates are in `byte-size.test.ts`.
 */
import { afterEach, describe, it, expect } from 'vitest'
import { formatDuration, formatFilesPerSecond, formatMilliseconds, seconds } from './duration'
import { _setLocaleForTests } from '$lib/intl/locale'

describe('formatDuration', () => {
  it('renders sub-minute durations in whole seconds', () => {
    expect(formatDuration(seconds(0))).toBe('0s')
    expect(formatDuration(seconds(45.4))).toBe('45s')
    expect(formatDuration(seconds(59))).toBe('59s')
  })

  it('renders minutes with a seconds tail, dropping a zero tail', () => {
    expect(formatDuration(seconds(60))).toBe('1m')
    expect(formatDuration(seconds(492))).toBe('8m 12s')
    expect(formatDuration(seconds(346))).toBe('5m 46s')
  })

  it('renders hours with a minutes tail, dropping a zero tail', () => {
    expect(formatDuration(seconds(3600))).toBe('1h')
    expect(formatDuration(seconds(3900))).toBe('1h 5m')
  })
})

describe('formatFilesPerSecond', () => {
  // It returns the NUMBER and the plural selector; the "files/s" marker is
  // user-facing copy and lives in the catalog (`fileOperations.shared.fileRate`),
  // the same split `fileOperations.shared.byteRate` makes for transfer speed.
  describe('rates below 3 (1 decimal)', () => {
    it('formats sub-1 rates with 1 decimal', () => {
      expect(formatFilesPerSecond(0.4)?.text).toBe('0.4')
    })

    it('formats 1.x rates with 1 decimal', () => {
      expect(formatFilesPerSecond(1.8)?.text).toBe('1.8')
    })

    it('formats 2.x rates with 1 decimal', () => {
      expect(formatFilesPerSecond(2.5)?.text).toBe('2.5')
    })

    it('rounds to 1 decimal', () => {
      expect(formatFilesPerSecond(0.44)?.text).toBe('0.4')
      expect(formatFilesPerSecond(0.45)?.text).toBe('0.5')
    })
  })

  describe('the plural selector', () => {
    it('drops the tenth at exactly one, so the digits and the noun can agree', () => {
      // CLDR reads a SHOWN "1.0" as `other` in en/de/nl/sv ("1.0 files"), while
      // the selector for the number 1 is `one` ("1 file"). Showing the tenth
      // here would print "1.0 file/s" — the one rate where the two disagree.
      expect(formatFilesPerSecond(1)).toEqual({ text: '1', value: 1 })
      expect(formatFilesPerSecond(0.97)).toEqual({ text: '1', value: 1 })
      expect(formatFilesPerSecond(1.04)).toEqual({ text: '1', value: 1 })
    })

    it('hands it the shown value, not the raw rate, so the words match the digits', () => {
      // 1.05 is shown as "1.1", and a locale that pluralizes on the decimal has
      // to select from what the reader can see.
      expect(formatFilesPerSecond(1.05)).toEqual({ text: '1.1', value: 1.1 })
      expect(formatFilesPerSecond(27.4)).toEqual({ text: '27', value: 27 })
    })
  })

  describe('rates at or above 3 (integer)', () => {
    it('rounds to integer at exactly 3', () => {
      expect(formatFilesPerSecond(3)?.text).toBe('3')
    })

    it('rounds 27.5 up to 28', () => {
      expect(formatFilesPerSecond(27.5)?.text).toBe('28')
    })

    it('groups a large rate for the locale rather than printing raw digits', () => {
      expect(formatFilesPerSecond(1500)?.text).toBe('1,500')
    })
  })

  describe('returns null when rate rounds to 0', () => {
    it('returns null for exactly 0', () => {
      expect(formatFilesPerSecond(0)).toBe(null)
    })

    it('returns null for rates < 0.05 (round to 0.0)', () => {
      expect(formatFilesPerSecond(0.04)).toBe(null)
      expect(formatFilesPerSecond(0.0001)).toBe(null)
    })

    it('returns "0.1" at the 0.05 boundary', () => {
      expect(formatFilesPerSecond(0.05)?.text).toBe('0.1')
    })
  })
})
describe('the locale owns the decimal mark, in every unit on the screen', () => {
  afterEach(() => {
    _setLocaleForTests(null)
  })

  it('gives a file rate the same decimal mark the size beside it uses', () => {
    // The defect: sizes went through Intl and read "250,00 MB" in Swedish while
    // the rate beside them was built with `toFixed`, which always emits an ASCII
    // dot, so one dialog showed two decimal conventions at once.
    _setLocaleForTests('sv-SE')
    expect(formatFilesPerSecond(2.3)?.text).toBe('2,3')
  })

  it('groups a big rate the way the locale groups', () => {
    _setLocaleForTests('de-DE')
    expect(formatFilesPerSecond(1500)?.text).toBe('1.500')
  })

  it('formats a sub-second duration through the locale too', () => {
    // Diagnostics-only, and fixed anyway: two identical bugs in one file is how
    // the next person finds the second one all over again.
    _setLocaleForTests('de-DE')
    expect(formatMilliseconds(1400)).toBe('1,4 s')
  })
})

describe('formatMilliseconds', () => {
  it('renders sub-second timings in whole milliseconds', () => {
    expect(formatMilliseconds(847)).toBe('847 ms')
  })

  it('renders a sub-minute timing in seconds, to a tenth', () => {
    expect(formatMilliseconds(1400)).toBe('1.4 s')
  })

  it('hands anything longer to formatDuration', () => {
    expect(formatMilliseconds(492_000)).toBe('8m 12s')
  })
})
