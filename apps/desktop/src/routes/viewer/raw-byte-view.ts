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

/** A fetched window of original bytes, starting at a whole row. */
export interface RawChunk {
  startRow: number
  bytes: number[]
}

/** The rows `chunk` covers, each labeled with its own byte offset. */
export function rawChunkRows({
  chunk,
  mode,
  totalBytes,
}: {
  chunk: RawChunk
  mode: RawViewMode
  totalBytes: number
}): RawRow[] {
  const perRow = bytesPerRawRow(mode)
  const rows: RawRow[] = []
  for (let start = 0; start < chunk.bytes.length; start += perRow) {
    const offset = (chunk.startRow + start / perRow) * perRow
    rows.push(formatRawRow(chunk.bytes.slice(start, start + perRow), offset, totalBytes, mode))
  }
  return rows
}

/**
 * Past `MAX_RAW_SCROLL_HEIGHT` one scrollbar pixel spans many rows, so the view steps
 * the top row itself for wheel and keys, and the scrollbar only serves big jumps.
 */
export function isRawScrollScaled(totalRows: number): boolean {
  return totalRows * RAW_ROW_HEIGHT > MAX_RAW_SCROLL_HEIGHT
}

/** Whole rows a wheel event moves; the sub-row pixel remainder carries to the next event. */
export function wheelRowStep({
  deltaY,
  deltaMode,
  viewportRows,
  carry,
}: {
  deltaY: number
  deltaMode: number
  viewportRows: number
  carry: number
}): { rows: number; carry: number } {
  if (deltaMode === WheelEvent.DOM_DELTA_LINE) return { rows: Math.trunc(deltaY), carry: 0 }
  if (deltaMode === WheelEvent.DOM_DELTA_PAGE) return { rows: Math.trunc(deltaY * viewportRows), carry: 0 }
  const pixels = carry + deltaY
  const rows = Math.trunc(pixels / RAW_ROW_HEIGHT)
  return { rows, carry: pixels - rows * RAW_ROW_HEIGHT }
}

export function clampRawTopRow(row: number, viewportRows: number, totalRows: number): number {
  return Math.max(0, Math.min(row, totalRows - viewportRows))
}

/** The top row after a navigation key, or `null` when the key doesn't navigate. */
export function rawKeyTopRow({
  key,
  topRow,
  viewportRows,
  totalRows,
}: {
  key: string
  topRow: number
  viewportRows: number
  totalRows: number
}): number | null {
  const page = Math.max(1, viewportRows - 1)
  const next: Record<string, number> = {
    ArrowDown: topRow + 1,
    ArrowUp: topRow - 1,
    PageDown: topRow + page,
    PageUp: topRow - page,
    Home: 0,
    End: totalRows,
  }
  return key in next ? clampRawTopRow(next[key], viewportRows, totalRows) : null
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
