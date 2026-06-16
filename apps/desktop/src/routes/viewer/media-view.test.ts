import { afterEach, describe, it, expect } from 'vitest'

import {
  mediaUrl,
  isMediaKind,
  mediaKindLabel,
  viewAsMediaLabel,
  formatMediaDimensions,
  clampZoom,
  nextClickZoom,
  MEDIA_MIN_ZOOM,
  MEDIA_MAX_ZOOM,
} from './media-view'
import { _setLocaleForTests } from '$lib/intl/locale'

describe('mediaUrl', () => {
  it('builds the cmdr-media URL from a token', () => {
    expect(mediaUrl('abc123')).toBe('cmdr-media://localhost/abc123')
  })

  it('encodes a token so it can never break out of the path', () => {
    // A token is CSPRNG hex from the backend, but encode defensively anyway.
    expect(mediaUrl('a/b?c#d')).toBe('cmdr-media://localhost/a%2Fb%3Fc%23d')
  })
})

describe('isMediaKind', () => {
  it('treats image and pdf as media, text as not', () => {
    expect(isMediaKind('image')).toBe(true)
    expect(isMediaKind('pdf')).toBe(true)
    expect(isMediaKind('text')).toBe(false)
  })
})

describe('mediaKindLabel', () => {
  it('maps each kind to its sentence-case label', () => {
    expect(mediaKindLabel('text')).toBe('Text')
    expect(mediaKindLabel('image')).toBe('Image')
    expect(mediaKindLabel('pdf')).toBe('PDF')
  })
})

describe('viewAsMediaLabel', () => {
  it('builds the reverse-switch label: sentence case, but PDF stays uppercase', () => {
    expect(viewAsMediaLabel('image')).toBe('View as image')
    expect(viewAsMediaLabel('pdf')).toBe('View as PDF')
  })
})

describe('formatMediaDimensions', () => {
  afterEach(() => {
    _setLocaleForTests(null)
  })

  it('formats dimensions with a multiplication sign and locale thousands separators', () => {
    _setLocaleForTests('en-US')
    expect(formatMediaDimensions({ width: 1920, height: 1080 })).toBe('1,920 × 1,080')
    _setLocaleForTests('de-DE')
    expect(formatMediaDimensions({ width: 1920, height: 1080 })).toBe('1.920 × 1.080')
  })

  it('returns null when dimensions are absent', () => {
    expect(formatMediaDimensions(null)).toBe(null)
  })
})

describe('clampZoom', () => {
  it('keeps a value inside the allowed range', () => {
    expect(clampZoom(1)).toBe(1)
  })

  it('clamps below the minimum and above the maximum', () => {
    expect(clampZoom(MEDIA_MIN_ZOOM - 5)).toBe(MEDIA_MIN_ZOOM)
    expect(clampZoom(MEDIA_MAX_ZOOM + 5)).toBe(MEDIA_MAX_ZOOM)
  })
})

describe('nextClickZoom', () => {
  // Clicking the image toggles between fit-to-window and 100%.
  it('goes from fit to 100% (1)', () => {
    expect(nextClickZoom('fit')).toEqual({ mode: 'actual', zoom: 1 })
  })

  it('goes from 100% (or any explicit zoom) back to fit', () => {
    expect(nextClickZoom('actual')).toEqual({ mode: 'fit', zoom: null })
  })
})
