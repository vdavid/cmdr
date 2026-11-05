import { describe, it, expect } from 'vitest'
import vectors from '../../../../../crates/cmdr-multi-rename/src/media_token_vectors.json'
import { MAX_COUNTER_DIGITS } from './counter-token'
import { MEDIA_FIELDS, mediaFieldOfName, mediaKind, usesMediaFields } from './media-token'

/** `[=tag.track:2]` → `=tag.track:2`: the kinds read what sits between the brackets. */
const inner = (token: string): string => token.slice(1, -1)

describe('media token, against the vectors shared with mask.rs', () => {
  it('uses the same width cap as the backend', () => {
    expect(MAX_COUNTER_DIGITS).toBe(vectors.maxDigits)
  })

  it('knows every field, its mask name, and its kind', () => {
    expect(Object.entries(MEDIA_FIELDS).map(([wire, { name, kind }]) => ({ name, wire, kind }))).toEqual(vectors.fields)
  })

  it.each(vectors.valid)('reads $token as the backend does', (vector) => {
    const token = mediaKind.parse(inner(vector.token))
    expect(token).not.toBeNull()
    if (token === null) return
    expect(MEDIA_FIELDS[token.field].name).toBe(vector.field)
    expect(token.fallback).toBe(vector.fallback)
    expect(token.format).toEqual(vector.format)
  })

  it.each(vectors.valid)('rewrites $token as $minimal', (vector) => {
    const token = mediaKind.parse(inner(vector.token))
    expect(token).not.toBeNull()
    if (token === null) return
    expect(`[${mediaKind.format(token)}]`).toBe(vector.minimal)
  })

  it.each(vectors.invalid)('rejects %s', (token) => {
    expect(mediaKind.parse(inner(token))).toBeNull()
  })
})

describe('media token', () => {
  it('only reads a media field', () => {
    expect(mediaKind.parse('N')).toBeNull()
    expect(mediaKind.parse('C10')).toBeNull()
    expect(mediaKind.parse('')).toBeNull()
  })

  it('finds a field by its mask name, exactly', () => {
    expect(mediaFieldOfName('tag.albumartist')).toBe('tagAlbumArtist')
    expect(mediaFieldOfName('Tag.albumartist')).toBeNull()
    expect(mediaFieldOfName('tagAlbumArtist')).toBeNull()
  })

  it('writes a token the editor built', () => {
    expect(mediaKind.format({ field: 'exifTaken', fallback: true, format: { kind: 'date', text: 'YMD' } })).toBe(
      '=exif.taken|modified:YMD',
    )
    expect(mediaKind.format({ field: 'tagTrack', fallback: false, format: { kind: 'digits', digits: 999 } })).toBe(
      '=tag.track:64',
    )
    expect(mediaKind.format({ field: 'tagTrack', fallback: false, format: { kind: 'digits', digits: 0 } })).toBe(
      '=tag.track:1',
    )
    expect(
      mediaKind.format({ field: 'tagArtist', fallback: false, format: { kind: 'range', range: { kind: 'all' } } }),
    ).toBe('=tag.artist')
  })

  it('leaves out what a field kind cannot take', () => {
    // `|modified` belongs to dates only, and each kind takes its own format.
    expect(mediaKind.format({ field: 'tagTrack', fallback: true, format: { kind: 'date', text: 'd' } })).toBe(
      '=tag.track',
    )
  })

  it('tells whether a mask uses any media field', () => {
    expect(usesMediaFields('[=exif.taken:d] [N]')).toBe(true)
    expect(usesMediaFields('[N]-[C:3]')).toBe(false)
    expect(usesMediaFields('[[=exif.taken]')).toBe(false)
    expect(usesMediaFields('[=exif.nope]')).toBe(false)
    expect(usesMediaFields('')).toBe(false)
  })
})
