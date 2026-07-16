/**
 * The master "Index image contents" toggle (`mediaIndex.enabled`) gates the whole
 * "text in images" section: when off it's a complete no-op — renders nothing AND fires
 * no backend IPC (no `mediaIndexVolumeState`, no `mediaIndexSearchOcr`), even with a live
 * query. Flipping the setting live-hides / reveals the section with no restart.
 *
 * The IPC commands + `$lib/settings` are mocked so the component drives each state
 * deterministically; timers are faked to fire the 300 ms debounced fetch.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import type { MediaIndexVolumeState, OcrHit, SimilarImage } from '$lib/ipc/bindings'
import ImageSearchResults from './ImageSearchResults.svelte'

// Hoisted so the `vi.mock` factories (also hoisted) can close over these before the
// component module — and thus the mocked deps — evaluate.
const h = vi.hoisted(() => ({
  searchOcr: vi.fn<(volumeId: string, query: string, limit: number | null) => Promise<OcrHit[]>>(),
  searchSemantic:
    vi.fn<(volumeId: string, query: string, limit: number | null) => Promise<{ path: string; score: number }[]>>(),
  volumeState: vi.fn<(volumeId: string) => Promise<MediaIndexVolumeState>>(),
  thumbnailToken: vi.fn<(path: string) => Promise<string | null>>(),
  dropTokens: vi.fn<(tokens: string[]) => Promise<void>>(),
  findSimilar: vi.fn<(volumeId: string, sourcePath: string, limit: number | null) => Promise<SimilarImage[]>>(),
  // The master toggle plus a captured live-change callback, so a test can flip it at
  // runtime exactly as the settings store does (the component subscribes via
  // `onSpecificSettingChange`).
  settings: { masterEnabled: true, liveChange: null as null | ((id: string, value: unknown) => void) },
}))

vi.mock('$lib/tauri-commands', () => ({
  mediaIndexSearchOcr: (v: string, q: string, l: number | null) => h.searchOcr(v, q, l),
  mediaIndexSearchSemantic: (v: string, q: string, l: number | null) => h.searchSemantic(v, q, l),
  mediaIndexVolumeState: (v: string) => h.volumeState(v),
  mediaIndexThumbnailToken: (p: string) => h.thumbnailToken(p),
  mediaIndexDropThumbnailTokens: (t: string[]) => h.dropTokens(t),
  mediaIndexFindSimilar: (v: string, s: string, l: number | null) => h.findSimilar(v, s, l),
}))

vi.mock('$lib/settings', () => ({
  getSetting: (key: string) => (key === 'mediaIndex.enabled' ? h.settings.masterEnabled : undefined),
  onSpecificSettingChange: (_key: string, cb: (id: string, value: unknown) => void) => {
    h.settings.liveChange = cb
    return () => {
      h.settings.liveChange = null
    }
  },
}))

vi.mock('../../routes/viewer/media-view', () => ({
  mediaUrl: (token: string) => `cmdr-media://localhost/${token}`,
}))

function state(overrides: Partial<MediaIndexVolumeState> = {}): MediaIndexVolumeState {
  return {
    enabled: true,
    indexing: false,
    enrichedCount: 5,
    qualifyingCount: null,
    networkOptIn: false,
    alwaysIndexed: false,
    paused: false,
    waitingForImportance: false,
    coveredQualifyingCount: null,
    keptCount: null,
    ...overrides,
  }
}

/** Mount with a live query + active dialog. */
function mountGrid(props: Record<string, unknown> = {}): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(ImageSearchResults, {
    target,
    props: { query: 'invoice', volumeId: 'root', active: true, onOpen: () => {}, ...props },
  })
  flushSync()
  return target
}

/** Fire the 300 ms debounce and let the awaited IPC mocks resolve. */
async function settle(): Promise<void> {
  await vi.advanceTimersByTimeAsync(400)
  await tick()
}

describe('ImageSearchResults master-toggle gating', () => {
  beforeEach(() => {
    h.settings.masterEnabled = true
    h.settings.liveChange = null
    vi.useFakeTimers()
    h.searchOcr.mockResolvedValue([
      { path: '/photos/receipt.png', snippet: 'total [invoice] amount' },
    ] satisfies OcrHit[])
    // No CLIP model in these tests: semantic search returns nothing, so the grid runs
    // OCR-only (the degraded path).
    h.searchSemantic.mockResolvedValue([])
    h.volumeState.mockResolvedValue(state())
    h.thumbnailToken.mockResolvedValue('tok123')
    h.dropTokens.mockResolvedValue()
    h.findSimilar.mockResolvedValue([])
  })

  afterEach(() => {
    vi.useRealTimers()
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('feature OFF + a typed query renders no section and fires NO IPC', async () => {
    h.settings.masterEnabled = false
    const target = mountGrid()
    await settle()

    expect(target.querySelector('.image-results')).toBeNull()
    expect(h.searchOcr).not.toHaveBeenCalled()
    expect(h.volumeState).not.toHaveBeenCalled()
    expect(h.thumbnailToken).not.toHaveBeenCalled()
  })

  it('feature ON renders the section and runs the OCR search', async () => {
    const target = mountGrid()
    await settle()

    expect(target.querySelector('.image-results')).not.toBeNull()
    expect(h.searchOcr).toHaveBeenCalledWith('root', 'invoice', null)
    expect(h.volumeState).toHaveBeenCalledWith('root')
  })

  it('flipping the toggle off live-hides the section, releases tokens, and stops firing IPC', async () => {
    const target = mountGrid()
    await settle()
    expect(target.querySelector('.image-results')).not.toBeNull()
    expect(h.searchOcr).toHaveBeenCalledTimes(1)

    // Turn the master toggle off at runtime, exactly as the settings store would.
    expect(h.settings.liveChange).not.toBeNull()
    h.settings.liveChange?.('mediaIndex.enabled', false)
    flushSync()
    await settle()

    expect(target.querySelector('.image-results')).toBeNull()
    // The minted thumbnail token was released, not leaked.
    expect(h.dropTokens).toHaveBeenCalledWith(['tok123'])
    // No further OCR search fired after the flip.
    expect(h.searchOcr).toHaveBeenCalledTimes(1)

    // Turning it back on resumes: the section returns and a fresh search runs (no restart).
    h.settings.liveChange?.('mediaIndex.enabled', true)
    flushSync()
    await settle()
    expect(target.querySelector('.image-results')).not.toBeNull()
    expect(h.searchOcr).toHaveBeenCalledTimes(2)
  })
})
