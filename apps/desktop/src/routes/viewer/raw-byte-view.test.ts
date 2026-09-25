import { describe, expect, it } from 'vitest'
import {
  byteCharacters,
  bytesPerRawRow,
  firstRawRow,
  formatRawRow,
  rawRowCount,
  scrollTopForRawRow,
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
})
