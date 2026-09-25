/** The two fixed-width views over the file's original bytes. */
export type RawViewMode = 'binary' | 'hex'

export const RAW_ROW_HEIGHT = 20
export const MAX_RAW_SCROLL_HEIGHT = 8_000_000

export function bytesPerRawRow(mode: RawViewMode): number {
  return mode === 'hex' ? 16 : 64
}

export function rawRowCount(totalBytes: number, mode: RawViewMode): number {
  return Math.ceil(totalBytes / bytesPerRawRow(mode))
}

/** One byte remains one display character, even when it is a newline or invalid UTF-8. */
export function byteCharacters(bytes: readonly number[]): string {
  return bytes
    .map((byte) =>
      (byte >= 0x20 && byte <= 0x7e) || (byte >= 0xa1 && byte !== 0xad) ? String.fromCharCode(byte) : '·',
    )
    .join('')
}

export interface RawRow {
  offset: string
  hex: string
  characters: string
}

export function formatRawRow(
  bytes: readonly number[],
  byteOffset: number,
  totalBytes: number,
  mode: RawViewMode,
): RawRow {
  const width = Math.max(8, Math.ceil(Math.log2(Math.max(1, totalBytes)) / 4))
  return {
    offset: byteOffset.toString(16).toUpperCase().padStart(width, '0'),
    hex:
      mode === 'hex'
        ? bytes
            .map((byte) => byte.toString(16).toUpperCase().padStart(2, '0'))
            .join(' ')
            .padEnd(47, ' ')
        : '',
    characters: byteCharacters(bytes),
  }
}

/** Maps the bounded DOM scrollbar onto an arbitrarily large byte range. */
export function firstRawRow(scrollTop: number, viewportHeight: number, totalRows: number): number {
  if (totalRows === 0) return 0
  const naturalHeight = totalRows * RAW_ROW_HEIGHT
  const scrollHeight = Math.min(naturalHeight, MAX_RAW_SCROLL_HEIGHT)
  const maxScroll = Math.max(0, scrollHeight - viewportHeight)
  const maxFirst = Math.max(0, totalRows - Math.ceil(viewportHeight / RAW_ROW_HEIGHT))
  if (naturalHeight <= MAX_RAW_SCROLL_HEIGHT) return Math.min(maxFirst, Math.floor(scrollTop / RAW_ROW_HEIGHT))
  return maxScroll === 0 ? 0 : Math.min(maxFirst, Math.floor((scrollTop / maxScroll) * maxFirst))
}

export function scrollTopForRawRow(row: number, viewportHeight: number, totalRows: number): number {
  const naturalHeight = totalRows * RAW_ROW_HEIGHT
  const scrollHeight = Math.min(naturalHeight, MAX_RAW_SCROLL_HEIGHT)
  const maxScroll = Math.max(0, scrollHeight - viewportHeight)
  const maxFirst = Math.max(0, totalRows - Math.ceil(viewportHeight / RAW_ROW_HEIGHT))
  if (naturalHeight <= MAX_RAW_SCROLL_HEIGHT) return Math.min(maxScroll, row * RAW_ROW_HEIGHT)
  return maxFirst === 0 ? 0 : (Math.min(row, maxFirst) / maxFirst) * maxScroll
}
