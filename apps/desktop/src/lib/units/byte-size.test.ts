/**
 * Unit math for byte sizes and rates. The date half of these tests lives in
 * `lib/settings/format-utils.test.ts`.
 */
import { afterEach, describe, it, expect } from 'vitest'
import {
  formatFileSizeWithFormat,
  formatDriveFigure,
  formatTieredSize,
  formatRoundSize,
  fixedUnitFor,
  dynamicTierIndex,
  unitLabel,
} from './byte-size'
import { _setCatalogForTests } from '$lib/intl/messages.svelte'
import driveFigureCases from './drive-figure-cases.json'
import type { FileSizeFormat } from '$lib/settings/types'
import { _setLocaleForTests } from '$lib/intl/locale'

describe('formatFileSizeWithFormat', () => {
  describe('binary (base 1024)', () => {
    it('formats 0 bytes', () => {
      expect(formatFileSizeWithFormat(0, 'binary')).toBe('0 bytes')
    })

    it('formats bytes below 1 KiB', () => {
      expect(formatFileSizeWithFormat(512, 'binary')).toBe('512 bytes')
    })

    it('formats exactly 1 KiB', () => {
      expect(formatFileSizeWithFormat(1024, 'binary')).toBe('1.00 KiB')
    })

    it('formats megabytes', () => {
      expect(formatFileSizeWithFormat(1024 * 1024, 'binary')).toBe('1.00 MiB')
    })

    it('formats gigabytes', () => {
      expect(formatFileSizeWithFormat(1024 ** 3, 'binary')).toBe('1.00 GiB')
    })

    it('formats terabytes', () => {
      expect(formatFileSizeWithFormat(1024 ** 4, 'binary')).toBe('1.00 TiB')
    })

    it('formats petabytes', () => {
      expect(formatFileSizeWithFormat(1024 ** 5, 'binary')).toBe('1.00 PiB')
    })

    it('caps at PiB for very large values', () => {
      const result = formatFileSizeWithFormat(1024 ** 6, 'binary')
      expect(result).toBe('1024.00 PiB')
    })

    it('formats fractional KiB values', () => {
      expect(formatFileSizeWithFormat(1536, 'binary')).toBe('1.50 KiB')
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
    it("'kB' under binary renders the IEC 'KiB' with 1024-based math", () => {
      expect(formatFileSizeWithFormat(2048, 'binary', 'kB')).toBe('2.00 KiB')
    })

    it("'kB' under SI renders 'kB' lowercase k with 1000-based math", () => {
      expect(formatFileSizeWithFormat(2000, 'si', 'kB')).toBe('2.00 kB')
    })

    it("'MB' under binary on 1 MiB returns '1.00 MiB'", () => {
      expect(formatFileSizeWithFormat(1024 ** 2, 'binary', 'MB')).toBe('1.00 MiB')
    })

    it("'GB' under SI on 2 GB returns '2.00 GB'", () => {
      expect(formatFileSizeWithFormat(2 * 1000 ** 3, 'si', 'GB')).toBe('2.00 GB')
    })

    it("forced kB on a sub-KiB value renders fractional ('0.50 KiB' binary)", () => {
      expect(formatFileSizeWithFormat(512, 'binary', 'kB')).toBe('0.50 KiB')
    })

    it("forced MB doesn't roll over to GB even on 10+ GB inputs", () => {
      const tenGB = 10 * 1000 ** 3
      expect(formatFileSizeWithFormat(tenGB, 'si', 'MB')).toBe('10000.00 MB')
    })
  })

  describe('rounded (the LIVE readout)', () => {
    // The transfer dialog's two numbers and its percentage have to agree. With
    // whole units only, a 1.7 GB / 2.4 GB copy read "2 GB / 2 GB (70%)": two
    // identical numbers beside a percentage that contradicts them, on every
    // transfer in the 1-10 GB range. So a single-digit value keeps one decimal.
    it('keeps a tenth on a single-digit value, so two different sizes read differently', () => {
      const gb = 1000 ** 3
      expect(formatFileSizeWithFormat(1.7 * gb, 'si', undefined, true)).toBe('1.7 GB')
      expect(formatFileSizeWithFormat(2.4 * gb, 'si', undefined, true)).toBe('2.4 GB')
    })

    it('shows the tenth even when it is zero, so the number never gains and loses a digit', () => {
      expect(formatFileSizeWithFormat(2 * 1000 ** 3, 'si', undefined, true)).toBe('2.0 GB')
    })

    it('drops to whole units from 10 up, where a tenth is noise a live number flickers on', () => {
      const gb = 1000 ** 3
      expect(formatFileSizeWithFormat(24.4 * gb, 'si', undefined, true)).toBe('24 GB')
      expect(formatFileSizeWithFormat(250 * 1000 ** 2, 'si', undefined, true)).toBe('250 MB')
    })

    it('rounds up ACROSS the ten boundary rather than printing "10.0"', () => {
      // 9.97 rounds to 10.0, which then wants the whole-unit form: the digit
      // count is decided on the value as it will be shown, not before.
      expect(formatFileSizeWithFormat(9.97 * 1000 ** 3, 'si', undefined, true)).toBe('10 GB')
    })

    it('leaves the bytes tier a bare integer, where a tenth of a byte means nothing', () => {
      expect(formatFileSizeWithFormat(512, 'binary', undefined, true)).toBe('512 bytes')
    })

    it('is never wider than the readout column budgets for', () => {
      // The readout's `--spacing-readout-amount` is 16ch, sized for
      // "999 GB / 999 GB". A single-digit value spends its saved digits on the
      // decimal instead, so the pair can't outgrow that.
      const widest = formatFileSizeWithFormat(999 * 1000 ** 3, 'si', undefined, true)
      expect(widest).toBe('999 GB')
      expect(formatFileSizeWithFormat(9.9 * 1000 ** 3, 'si', undefined, true).length).toBeLessThanOrEqual(widest.length)
    })
  })
})

describe('formatFileSizeWithFormat: locale-aware decimal', () => {
  afterEach(() => {
    _setLocaleForTests(null)
  })

  it('de-DE uses a comma decimal in the dynamic path', () => {
    _setLocaleForTests('de-DE')
    expect(formatFileSizeWithFormat(1024, 'binary')).toBe('1,00 KiB')
    expect(formatFileSizeWithFormat(1536, 'binary')).toBe('1,50 KiB')
    expect(formatFileSizeWithFormat(1024, 'si')).toBe('1,02 kB')
  })

  it('de-DE uses a comma decimal in the forced-unit path', () => {
    _setLocaleForTests('de-DE')
    expect(formatFileSizeWithFormat(1_073_208, 'binary', 'MB')).toBe('1,02 MiB')
    expect(formatFileSizeWithFormat(512, 'si', 'kB')).toBe('0,51 kB')
  })

  it('keeps the value and unit separated by a plain ASCII space (never NNBSP)', () => {
    _setLocaleForTests('de-DE')
    const out = formatFileSizeWithFormat(1024, 'binary')
    // Exactly one separator, and it's a regular space (U+0020): the
    // colorizeSizeString last-space parse and the size-tier coloring depend on
    // it. Intl's `style: 'unit'` would inject a NNBSP here, which is why we
    // compose the string ourselves.
    expect(out.split(' ')).toHaveLength(2)
    // No exotic space code points: U+00A0 NBSP, U+202F NNBSP, U+2009 thin space.
    expect(out).not.toMatch(/[\u00a0\u202f\u2009]/)
  })

  it('does NOT group the integer part of large forced-unit values', () => {
    // Pre-change `toFixed(2)` never grouped; en-US parity depends on this.
    _setLocaleForTests('de-DE')
    // de-DE groups with `.` and decimals with `,`; the integer part must stay
    // ungrouped so the value reads "10000,00", not "10.000,00".
    expect(formatFileSizeWithFormat(10 * 1000 ** 3, 'si', 'MB')).toBe('10000,00 MB')
  })

  it('does NOT group the bytes-as-integer dynamic value', () => {
    _setLocaleForTests('de-DE')
    // 1000 bytes in binary stays sub-base, rendered as a bare integer; no grouping.
    expect(formatFileSizeWithFormat(1000, 'binary')).toBe('1000 Bytes')
  })
})

describe('unitLabel', () => {
  // Base 1024 is IEC 80000-13 (KiB, MiB, GiB); base 1000 is SI (kB, MB, GB). The symbol
  // has to name the base the number was divided by, at every tier.
  it('names the IEC binary symbol under binary', () => {
    expect(unitLabel('kB', 'binary')).toBe('KiB')
    expect(unitLabel('MB', 'binary')).toBe('MiB')
    expect(unitLabel('GB', 'binary')).toBe('GiB')
  })

  it('names the SI decimal symbol under SI, with a lowercase k', () => {
    expect(unitLabel('kB', 'si')).toBe('kB')
    expect(unitLabel('MB', 'si')).toBe('MB')
    expect(unitLabel('GB', 'si')).toBe('GB')
  })
})

describe('formatRoundSize', () => {
  it('writes a whole preset value with no fraction digits', () => {
    expect(formatRoundSize(100 * 1024 ** 2, 'binary')).toBe('100 MiB')
    expect(formatRoundSize(3 * 1024 ** 3, 'binary')).toBe('3 GiB')
    expect(formatRoundSize(250_000_000, 'si')).toBe('250 MB')
  })

  it('keeps the fraction digits a value actually has', () => {
    expect(formatRoundSize(1.5 * 1024 ** 3, 'binary')).toBe('1.5 GiB')
    expect(formatRoundSize(1_250_000, 'si')).toBe('1.25 MB')
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

describe('formatDriveFigure', () => {
  afterEach(() => {
    _setLocaleForTests('en-US')
  })

  // The same table the backend's emit gate is tested against (`space_poller/readout.rs`), so the
  // figure on screen and the rule deciding when it changed can't drift apart.
  it.each(driveFigureCases.cases)('$name reads $text', ({ bytes, driveBytes, format, text }) => {
    _setLocaleForTests('en-US')
    expect(formatDriveFigure(bytes, driveBytes, format as FileSizeFormat)).toBe(text)
  })

  it('writes the decimals the way the locale does', () => {
    _setLocaleForTests('de-DE')
    expect(formatDriveFigure(5_000_000_000, 16_000_000_000, 'binary')).toBe('4,7 GiB')
  })
})

describe('size units come from the catalog, in the UI language', () => {
  // French writes "Mo" / "Go" (octets), so the unit is copy a translator owns,
  // not a code literal. The stand-in catalog is what a French translator would write.
  const frenchUnits = {
    'common.sizeUnit.byte': '{count, plural, one {octet} other {octets}}',
    'common.sizeUnit.kibibyte': 'Kio',
    'common.sizeUnit.mebibyte': 'Mio',
    'common.sizeUnit.gibibyte': 'Gio',
    'common.sizeUnit.tebibyte': 'Tio',
    'common.sizeUnit.pebibyte': 'Pio',
    'common.sizeUnit.kilobyte': 'ko',
    'common.sizeUnit.megabyte': 'Mo',
    'common.sizeUnit.gigabyte': 'Go',
    'common.sizeUnit.terabyte': 'To',
    'common.sizeUnit.petabyte': 'Po',
  }

  afterEach(() => {
    _setCatalogForTests('fr', null)
    _setLocaleForTests(null)
  })

  it('words every tier in the UI language', () => {
    _setCatalogForTests('fr', frenchUnits)
    _setLocaleForTests('fr-FR')
    expect(formatFileSizeWithFormat(512, 'binary')).toBe('512 octets')
    expect(formatFileSizeWithFormat(1536, 'binary')).toBe('1,50 Kio')
    expect(formatFileSizeWithFormat(1500, 'si')).toBe('1,50 ko')
    expect(formatFileSizeWithFormat(1024 ** 2, 'binary')).toBe('1,00 Mio')
    expect(formatFileSizeWithFormat(1000 ** 2, 'si')).toBe('1,00 Mo')
    expect(formatFileSizeWithFormat(1024 ** 3, 'binary', 'GB')).toBe('1,00 Gio')
    expect(formatFileSizeWithFormat(1024 ** 4, 'binary')).toBe('1,00 Tio')
    expect(formatFileSizeWithFormat(1000 ** 4, 'si')).toBe('1,00 To')
    expect(formatFileSizeWithFormat(1024 ** 5, 'binary')).toBe('1,00 Pio')
    expect(formatFileSizeWithFormat(1000 ** 5, 'si')).toBe('1,00 Po')
    expect(unitLabel('kB', 'si')).toBe('ko')
    expect(formatDriveFigure(261 * 1000 ** 3, 1000 ** 4, 'si')).toBe('261 Go')
  })

  it('picks the byte word by the plural rules of the UI language', () => {
    expect(formatFileSizeWithFormat(1, 'binary')).toBe('1 byte')
    expect(formatFileSizeWithFormat(2, 'binary')).toBe('2 bytes')
    // French counts zero as singular.
    _setCatalogForTests('fr', frenchUnits)
    _setLocaleForTests('fr-FR')
    expect(formatFileSizeWithFormat(0, 'binary')).toBe('0 octet')
  })

  it('hands back the size-tier with the text, so coloring never parses a unit word', () => {
    _setCatalogForTests('fr', frenchUnits)
    _setLocaleForTests('fr-FR')
    expect(formatTieredSize(3 * 1024 ** 2, 'binary')).toEqual({ text: '3,00 Mio', tier: 2 })
    // A forced unit keeps the magnitude's tier, the same color dynamic mode would give.
    expect(formatTieredSize(349, 'binary', 'MB')).toEqual({ text: '0,00 Mio', tier: 0 })
    expect(formatTieredSize(1024 ** 5, 'binary').tier).toBe(4)
  })
})
