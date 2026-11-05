/**
 * The media field placeholder, `[=source.field|modified:format]`: a file's EXIF or tag value, like
 * `[=exif.taken:d]`, `[=exif.taken|modified:YMD_hms]`, `[=tag.track:2]`, or `[=tag.artist:1-10]`. The
 * format depends on the field's kind: a date takes the mask's date letters, a number a digit count, and
 * text a range. `|modified` (date fields only) falls back to the modified date. Reads exactly what
 * `media()` in `crates/cmdr-multi-rename/src/mask/media_token.rs` reads; both are tested against
 * `media_token_vectors.json`. Pure.
 */
import type { MediaField } from '#lib/ipc/bindings.js'
import { MAX_COUNTER_DIGITS } from './counter-token'
import { findTokens, type MaskTokenGrammar } from './mask-tokens'
import { formatRangeToken, parseRange, type Range } from './range-token'

/** What kind of value a field holds, which decides the format it takes. */
export type MediaFieldKind = 'date' | 'number' | 'text'

/** Every field by its IPC name: the name a mask writes, and its kind. In the picker's order. */
export const MEDIA_FIELDS: Record<MediaField, { name: string; kind: MediaFieldKind }> = {
  exifTaken: { name: 'exif.taken', kind: 'date' },
  exifCamera: { name: 'exif.camera', kind: 'text' },
  exifMake: { name: 'exif.make', kind: 'text' },
  exifModel: { name: 'exif.model', kind: 'text' },
  exifLens: { name: 'exif.lens', kind: 'text' },
  exifIso: { name: 'exif.iso', kind: 'number' },
  exifFocal: { name: 'exif.focal', kind: 'number' },
  exifWidth: { name: 'exif.width', kind: 'number' },
  exifHeight: { name: 'exif.height', kind: 'number' },
  exifOrientation: { name: 'exif.orientation', kind: 'number' },
  tagTitle: { name: 'tag.title', kind: 'text' },
  tagArtist: { name: 'tag.artist', kind: 'text' },
  tagAlbumArtist: { name: 'tag.albumartist', kind: 'text' },
  tagAlbum: { name: 'tag.album', kind: 'text' },
  tagTrack: { name: 'tag.track', kind: 'number' },
  tagDisc: { name: 'tag.disc', kind: 'number' },
  tagYear: { name: 'tag.year', kind: 'number' },
  tagGenre: { name: 'tag.genre', kind: 'text' },
  tagComment: { name: 'tag.comment', kind: 'text' },
}

const FIELD_OF_NAME = new Map<string, MediaField>(
  (Object.entries(MEDIA_FIELDS) as [MediaField, { name: string }][]).map(([field, { name }]) => [name, field]),
)

/** The field a mask's name spells, exactly (case matters), or `null`. */
export function mediaFieldOfName(name: string): MediaField | null {
  return FIELD_OF_NAME.get(name) ?? null
}

/** A field's format as typed after its `:`; `null` for none (a date as `[d] [t]`, the rest as is). */
export type MediaFormat =
  | { kind: 'date'; text: string }
  | { kind: 'digits'; digits: number }
  | { kind: 'range'; range: Range }
  | null

export interface MediaToken {
  field: MediaField
  /** `|modified`: a file without the date takes its modified date. Date fields only. */
  fallback: boolean
  format: MediaFormat
}

const FALLBACK = 'modified'
const U32_MAX = 2n ** 32n - 1n

/** `date_parts()` in `mask.rs`: `d`, `t`, or date letters with `-` `.` `_` or spaces between, starting with a letter. */
function isDateFormat(text: string): boolean {
  return text === 'd' || text === 't' || /^[YyMDhms][YyMDhms\-._ ]*$/.test(text)
}

/** A digit count, 1 to `MAX_COUNTER_DIGITS`, read as Rust's `u32::from_str` reads it. */
function parseDigits(text: string): number | null {
  if (!/^\+?\d+$/.test(text) || BigInt(text) > U32_MAX) return null
  const digits = Number(text)
  return digits >= 1 && digits <= MAX_COUNTER_DIGITS ? digits : null
}

function parseFormat(kind: MediaFieldKind, text: string): MediaFormat | undefined {
  switch (kind) {
    case 'date':
      return isDateFormat(text) ? { kind: 'date', text } : undefined
    case 'number': {
      const digits = parseDigits(text)
      return digits === null ? undefined : { kind: 'digits', digits }
    }
    case 'text': {
      const range = parseRange(text)
      return range === null ? undefined : { kind: 'range', range }
    }
  }
}

/** The token `[<inner>]` spells, or `null` when it isn't a media field. Mirrors `mask.rs` step for step. */
export function parseMediaToken(inner: string): MediaToken | null {
  if (!inner.startsWith('=')) return null
  const body = inner.slice(1)
  const colon = body.indexOf(':')
  const head = colon === -1 ? body : body.slice(0, colon)
  const bar = head.indexOf('|')
  const field = mediaFieldOfName(bar === -1 ? head : head.slice(0, bar))
  if (field === null) return null
  const { kind } = MEDIA_FIELDS[field]
  const fallback = bar !== -1
  if (fallback && (kind !== 'date' || head.slice(bar + 1) !== FALLBACK)) return null
  if (colon === -1) return { field, fallback, format: null }
  const text = body.slice(colon + 1)
  if (text === '') return null
  const format = parseFormat(kind, text)
  return format === undefined ? null : { field, fallback, format }
}

/** The format's text after the `:`, or `''` for none. Only what `kind` takes. */
function formatText(kind: MediaFieldKind, format: MediaFormat): string {
  if (format === null) return ''
  if (kind === 'date' && format.kind === 'date' && isDateFormat(format.text)) return format.text
  if (kind === 'number' && format.kind === 'digits') {
    return String(Math.min(MAX_COUNTER_DIGITS, Math.max(1, Math.trunc(format.digits))))
  }
  if (kind === 'text' && format.kind === 'range') {
    const { range } = format
    // The whole text is no format at all; `1-` reads the same.
    if (range.kind === 'all' || (range.kind === 'from' && range.at === 1)) return ''
    return formatRangeToken({ field: 'fullName', range })
  }
  return ''
}

/** The token's text without brackets, in its shortest form. Drops what the field's kind can't take. */
export function formatMediaToken({ field, fallback, format }: MediaToken): string {
  const { name, kind } = MEDIA_FIELDS[field]
  const bar = fallback && kind === 'date' ? `|${FALLBACK}` : ''
  const text = formatText(kind, format)
  return `=${name}${bar}${text === '' ? '' : `:${text}`}`
}

export const mediaKind: MaskTokenGrammar<MediaToken> = {
  id: 'media',
  parse: parseMediaToken,
  format: formatMediaToken,
}

/** Whether `mask` reads any file's EXIF or tags, so a rename must wait for them. */
export function usesMediaFields(mask: string): boolean {
  return findTokens(mask, [mediaKind]).length > 0
}
