import { describe, expect, it } from 'vitest'
import {
  byteCharacters,
  bytesPerRawRow,
  firstRawRow,
  formatRawRow,
  isRawScrollScaled,
  rawChunkRows,
  rawKeyTopRow,
  rawRowCount,
  scrollTopForRawRow,
  wheelRowStep,
} from './raw-byte-view'

describe('raw byte views', () => {
  it('keeps line breaks and invalid UTF-8 in their byte positions', () => {
    expect(byteCharacters([0, 0x41, 0x0a, 0x80, 0xff])).toBe('·A··ÿ')
    expect(formatRawRow([0, 0x41, 0x0a, 0xff], 16, 100, 'hex')).toEqual({
      offset: '00000010',
      hex: '00 41 0A FF'.padEnd(47, ' '),
      characters: '·A·ÿ',
    })
  })

  it('uses fixed byte widths in binary and hex modes', () => {
    expect(bytesPerRawRow('binary')).toBe(64)
    expect(bytesPerRawRow('hex')).toBe(16)
    expect(rawRowCount(65, 'binary')).toBe(2)
    expect(rawRowCount(65, 'hex')).toBe(5)
  })

  it('can reach the end of a very large file with a bounded scrollbar', () => {
    const rows = rawRowCount(50_000_000_000, 'hex')
    const top = scrollTopForRawRow(rows, 600, rows)
    expect(firstRawRow(top, 600, rows)).toBe(rows - 30)
  })

  it('knows when the scrollbar is scaled, so row stepping takes over', () => {
    expect(isRawScrollScaled(rawRowCount(1_000_000, 'hex'))).toBe(false)
    expect(isRawScrollScaled(rawRowCount(50_000_000_000, 'hex'))).toBe(true)
  })

  it('turns wheel deltas into whole rows, carrying the remainder between events', () => {
    // Pixel deltas (trackpads): 20 px is one row, and a 10 px nudge waits for the next one.
    expect(wheelRowStep({ deltaY: 45, deltaMode: 0, viewportRows: 30, carry: 0 })).toEqual({ rows: 2, carry: 5 })
    expect(wheelRowStep({ deltaY: 10, deltaMode: 0, viewportRows: 30, carry: 5 })).toEqual({ rows: 0, carry: 15 })
    expect(wheelRowStep({ deltaY: 5, deltaMode: 0, viewportRows: 30, carry: 15 })).toEqual({ rows: 1, carry: 0 })
    expect(wheelRowStep({ deltaY: -45, deltaMode: 0, viewportRows: 30, carry: 0 })).toEqual({ rows: -2, carry: -5 })
    // Line and page deltas (mouse wheels, some drivers).
    expect(wheelRowStep({ deltaY: 3, deltaMode: 1, viewportRows: 30, carry: 0 })).toEqual({ rows: 3, carry: 0 })
    expect(wheelRowStep({ deltaY: 1, deltaMode: 2, viewportRows: 30, carry: 0 })).toEqual({ rows: 30, carry: 0 })
  })

  it('moves the top row one row or one page per key, clamped to the file', () => {
    const total = 1_000_000_000
    expect(rawKeyTopRow({ key: 'ArrowDown', topRow: 500, viewportRows: 30, totalRows: total })).toBe(501)
    expect(rawKeyTopRow({ key: 'ArrowUp', topRow: 500, viewportRows: 30, totalRows: total })).toBe(499)
    expect(rawKeyTopRow({ key: 'PageDown', topRow: 500, viewportRows: 30, totalRows: total })).toBe(529)
    expect(rawKeyTopRow({ key: 'PageUp', topRow: 500, viewportRows: 30, totalRows: total })).toBe(471)
    expect(rawKeyTopRow({ key: 'Home', topRow: 500, viewportRows: 30, totalRows: total })).toBe(0)
    expect(rawKeyTopRow({ key: 'End', topRow: 500, viewportRows: 30, totalRows: total })).toBe(total - 30)
    expect(rawKeyTopRow({ key: 'ArrowUp', topRow: 0, viewportRows: 30, totalRows: total })).toBe(0)
    expect(rawKeyTopRow({ key: 'a', topRow: 500, viewportRows: 30, totalRows: total })).toBeNull()
  })

  it('formats the rows a fetched chunk covers, at their own byte offsets', () => {
    const bytes = Array.from({ length: 40 }, (_, i) => 0x41 + (i % 26))
    const rows = rawChunkRows({ chunk: { startRow: 10, bytes }, mode: 'hex', totalBytes: 1000 })
    expect(rows.map((row) => row.offset)).toEqual(['000000A0', '000000B0', '000000C0'])
    expect(rows[2].characters).toBe('GHIJKLMN')
  })
})
