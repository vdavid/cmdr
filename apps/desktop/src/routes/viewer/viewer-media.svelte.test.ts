/**
 * Tests for `createViewerMedia`: how it absorbs a `viewer_open` result into the
 * media session state, derives `isMedia` / `mediaSrc`, orchestrates the
 * "View as text" override (reset media state first, then ask the page to re-open
 * as text; no-op for a text session), remembers the file's natural media kind
 * (`lastMediaKind`) across a switch to text, and offers the reverse "View as
 * image / PDF" switch back via `viewAsMedia`.
 */

import { describe, expect, it, vi } from 'vitest'

import { createViewerMedia } from './viewer-media.svelte'
import type { ViewerOpenResult } from '$lib/ipc/bindings'

function openResult(overrides: Partial<ViewerOpenResult>): ViewerOpenResult {
  return {
    sessionId: 'sess-1',
    fileName: 'file',
    totalBytes: 0,
    totalLines: 0,
    estimatedTotalLines: 0,
    backendType: 'fullLoad',
    capabilities: {
      supportsLineSeek: true,
      supportsByteSeek: true,
      supportsFractionSeek: true,
      knowsTotalLines: true,
    },
    initialLines: { lines: [], firstLineNumber: 0, byteOffset: 0, totalLines: 0, totalBytes: 0 },
    isIndexing: false,
    encoding: 'utf8',
    kind: 'text',
    mediaToken: null,
    mediaDimensions: null,
    ...overrides,
  }
}

describe('createViewerMedia', () => {
  function deps(overrides: Partial<Parameters<typeof createViewerMedia>[0]> = {}) {
    return { reopenAsText: vi.fn(() => Promise.resolve()), reopenNatural: vi.fn(() => Promise.resolve()), ...overrides }
  }

  it('starts as a text session with no media', () => {
    const media = createViewerMedia(deps())
    expect(media.kind).toBe('text')
    expect(media.isMedia).toBe(false)
    expect(media.mediaSrc).toBe('')
    expect(media.mediaDimensions).toBeNull()
    expect(media.lastMediaKind).toBeNull()
  })

  it('absorbs an image open result into media state and derives the src', () => {
    const media = createViewerMedia(deps())
    media.setFromOpenResult(
      openResult({ kind: 'image', mediaToken: 'abc123', mediaDimensions: { width: 800, height: 600 } }),
    )
    expect(media.kind).toBe('image')
    expect(media.isMedia).toBe(true)
    expect(media.mediaSrc).toBe('cmdr-media://localhost/abc123')
    expect(media.mediaDimensions).toEqual({ width: 800, height: 600 })
    expect(media.lastMediaKind).toBe('image')
  })

  it('absorbs a pdf open result (no dimensions)', () => {
    const media = createViewerMedia(deps())
    media.setFromOpenResult(openResult({ kind: 'pdf', mediaToken: 'deadbeef', mediaDimensions: null }))
    expect(media.kind).toBe('pdf')
    expect(media.isMedia).toBe(true)
    expect(media.mediaSrc).toBe('cmdr-media://localhost/deadbeef')
    expect(media.mediaDimensions).toBeNull()
    expect(media.lastMediaKind).toBe('pdf')
  })

  it('reset() returns to the text shape but PRESERVES lastMediaKind', () => {
    const media = createViewerMedia(deps())
    media.setFromOpenResult(openResult({ kind: 'image', mediaToken: 'abc123' }))
    media.reset()
    expect(media.kind).toBe('text')
    expect(media.isMedia).toBe(false)
    expect(media.mediaSrc).toBe('')
    // The remembered natural kind is what the text view offers switching back to.
    expect(media.lastMediaKind).toBe('image')
  })

  it('viewAsText resets media state BEFORE re-opening, so a slow re-open never renders a dangling image', async () => {
    let kindAtReopen: string | null = null
    let srcAtReopen: string | null = null
    const media = createViewerMedia(
      deps({
        reopenAsText: () => {
          // The page opens the fresh text session here; the media state must
          // already be reset so a re-open failure can't leave a stale image up.
          kindAtReopen = media.kind
          srcAtReopen = media.mediaSrc
          return Promise.resolve()
        },
      }),
    )
    media.setFromOpenResult(openResult({ kind: 'image', mediaToken: 'abc123' }))
    await media.viewAsText()
    expect(kindAtReopen).toBe('text')
    expect(srcAtReopen).toBe('')
  })

  it('viewAsText keeps lastMediaKind so the text view remembers the natural kind', async () => {
    const media = createViewerMedia(deps())
    media.setFromOpenResult(openResult({ kind: 'pdf', mediaToken: 'deadbeef' }))
    await media.viewAsText()
    expect(media.kind).toBe('text')
    expect(media.lastMediaKind).toBe('pdf')
  })

  it('viewAsText is a no-op for a text session (nothing to switch to)', async () => {
    const d = deps()
    const media = createViewerMedia(d)
    await media.viewAsText()
    expect(d.reopenAsText).not.toHaveBeenCalled()
  })

  it('viewAsMedia re-opens the file naturally once we are in text view of a media file', async () => {
    const d = deps()
    const media = createViewerMedia(d)
    // Open as image, then switch to text (preserving lastMediaKind), then switch back.
    media.setFromOpenResult(openResult({ kind: 'image', mediaToken: 'abc123' }))
    await media.viewAsText()
    expect(media.kind).toBe('text')

    await media.viewAsMedia()
    expect(d.reopenNatural).toHaveBeenCalledTimes(1)
  })

  it('viewAsMedia is a no-op when the current kind is media (nothing to switch to)', async () => {
    const d = deps()
    const media = createViewerMedia(d)
    media.setFromOpenResult(openResult({ kind: 'image', mediaToken: 'abc123' }))
    await media.viewAsMedia()
    expect(d.reopenNatural).not.toHaveBeenCalled()
  })

  it('viewAsMedia is a no-op for a genuine text file (no remembered media kind)', async () => {
    const d = deps()
    const media = createViewerMedia(d)
    // A plain text file: lastMediaKind never gets set.
    media.setFromOpenResult(openResult({ kind: 'text' }))
    await media.viewAsMedia()
    expect(d.reopenNatural).not.toHaveBeenCalled()
  })
})
