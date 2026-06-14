/**
 * Tests for `createViewerMedia`: how it absorbs a `viewer_open` result into the
 * media session state, derives `isMedia` / `mediaSrc`, and orchestrates the
 * "View as text" override (reset media state first, then ask the page to re-open
 * as text; no-op for a text session).
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
  it('starts as a text session with no media', () => {
    const media = createViewerMedia({ reopenAsText: vi.fn() })
    expect(media.kind).toBe('text')
    expect(media.isMedia).toBe(false)
    expect(media.mediaSrc).toBe('')
    expect(media.mediaDimensions).toBeNull()
  })

  it('absorbs an image open result into media state and derives the src', () => {
    const media = createViewerMedia({ reopenAsText: vi.fn() })
    media.setFromOpenResult(
      openResult({ kind: 'image', mediaToken: 'abc123', mediaDimensions: { width: 800, height: 600 } }),
    )
    expect(media.kind).toBe('image')
    expect(media.isMedia).toBe(true)
    expect(media.mediaSrc).toBe('cmdr-media://localhost/abc123')
    expect(media.mediaDimensions).toEqual({ width: 800, height: 600 })
  })

  it('absorbs a pdf open result (no dimensions)', () => {
    const media = createViewerMedia({ reopenAsText: vi.fn() })
    media.setFromOpenResult(openResult({ kind: 'pdf', mediaToken: 'deadbeef', mediaDimensions: null }))
    expect(media.kind).toBe('pdf')
    expect(media.isMedia).toBe(true)
    expect(media.mediaSrc).toBe('cmdr-media://localhost/deadbeef')
    expect(media.mediaDimensions).toBeNull()
  })

  it('reset() returns to the text shape', () => {
    const media = createViewerMedia({ reopenAsText: vi.fn() })
    media.setFromOpenResult(openResult({ kind: 'image', mediaToken: 'abc123' }))
    media.reset()
    expect(media.kind).toBe('text')
    expect(media.isMedia).toBe(false)
    expect(media.mediaSrc).toBe('')
  })

  it('viewAsText resets media state BEFORE re-opening, so a slow re-open never renders a dangling image', async () => {
    let kindAtReopen: string | null = null
    let srcAtReopen: string | null = null
    const media = createViewerMedia({
      reopenAsText: () => {
        // The page opens the fresh text session here; the media state must
        // already be reset so a re-open failure can't leave a stale image up.
        kindAtReopen = media.kind
        srcAtReopen = media.mediaSrc
        return Promise.resolve()
      },
    })
    media.setFromOpenResult(openResult({ kind: 'image', mediaToken: 'abc123' }))
    await media.viewAsText()
    expect(kindAtReopen).toBe('text')
    expect(srcAtReopen).toBe('')
  })

  it('viewAsText is a no-op for a text session (nothing to switch to)', async () => {
    const reopenAsText = vi.fn(() => Promise.resolve())
    const media = createViewerMedia({ reopenAsText })
    await media.viewAsText()
    expect(reopenAsText).not.toHaveBeenCalled()
  })
})
