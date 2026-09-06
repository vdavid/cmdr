/**
 * Two settings gate the "text in images" section, and either one off is a complete
 * no-op — renders nothing AND fires no backend IPC (no `mediaIndexVolumeState`, no
 * `mediaIndexSearchSemantic`, no `mediaIndexSearchOcr`), even with a live query:
 *   - `mediaIndex.enabled`, the master "Index image contents" toggle.
 *   - `mediaIndex.showInSearch`, "Show image results in Search" (off by default).
 * Flipping either live-hides / reveals the section with no restart.
 *
 * The IPC commands + `$lib/settings` are mocked so the component drives each state
 * deterministically; timers are faked to fire the 300 ms debounced fetch. The settings
 * fake is keyed BY SETTING ID on both halves: the component holds one subscription per
 * gate, so a single shared `liveChange` slot would let the second subscription overwrite
 * the first and a test would silently drive the wrong toggle.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import type { MediaIndexVolumeState, OcrHit, SimilarImage } from '$lib/ipc/bindings'
import ImageSearchResults from './ImageSearchResults.svelte'

// Hoisted so the `vi.mock` factories (also hoisted) can close over these before the
// component module — and thus the mocked deps — evaluate.
const h = vi.hoisted(() => {
  // What `getSetting` answers per id, plus the live-change callback the component
  // registered for each id, so a test can flip one at runtime exactly as the settings
  // store does (the component subscribes via `onSpecificSettingChange`). Annotated on a
  // local rather than inline: an `as` on the property is one eslint autofix away from
  // being dropped as redundant, which silently widens `getSetting` back to an unsafe any.
  const settingValues: Record<string, unknown> = {}
  const settingListeners = new Map<string, (value: unknown) => void>()
  return {
    searchOcr: vi.fn<(payload: { volumeId: string; query: string; limit: number | null }) => Promise<OcrHit[]>>(),
    searchSemantic:
      vi.fn<
        (payload: {
          volumeId: string
          query: string
          limit: number | null
        }) => Promise<{ path: string; score: number }[]>
      >(),
    volumeState: vi.fn<(volumeId: string) => Promise<MediaIndexVolumeState>>(),
    thumbnailToken: vi.fn<(path: string) => Promise<string | null>>(),
    dropTokens: vi.fn<(tokens: string[]) => Promise<void>>(),
    findSimilar:
      vi.fn<(payload: { volumeId: string; sourcePath: string; limit: number | null }) => Promise<SimilarImage[]>>(),
    settings: { values: settingValues, liveChange: settingListeners },
  }
})

vi.mock('$lib/tauri-commands', () => ({
  mediaIndexSearchOcr: (volumeId: string, query: string, limit: number | null) =>
    h.searchOcr({ volumeId, query, limit }),
  mediaIndexSearchSemantic: (volumeId: string, query: string, limit: number | null) =>
    h.searchSemantic({ volumeId, query, limit }),
  mediaIndexVolumeState: (v: string) => h.volumeState(v),
  mediaIndexThumbnailToken: (p: string) => h.thumbnailToken(p),
  mediaIndexDropThumbnailTokens: (t: string[]) => h.dropTokens(t),
  mediaIndexFindSimilar: (volumeId: string, sourcePath: string, limit: number | null) =>
    h.findSimilar({ volumeId, sourcePath, limit }),
}))

vi.mock('$lib/settings', () => ({
  getSetting: (key: string) => h.settings.values[key],
  onSpecificSettingChange: (key: string, cb: (value: unknown) => void) => {
    h.settings.liveChange.set(key, cb)
    return () => h.settings.liveChange.delete(key)
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

/** Both gates on, fake timers, and a one-hit OCR result: the state every test starts from. */
function seedDefaults(): void {
  h.settings.values = { 'mediaIndex.enabled': true, 'mediaIndex.showInSearch': true }
  h.settings.liveChange.clear()
  vi.useFakeTimers()
  h.searchOcr.mockResolvedValue([{ path: '/photos/receipt.png', snippet: 'total [invoice] amount' }] satisfies OcrHit[])
  // No CLIP model in these tests: semantic search returns nothing, so the grid runs
  // OCR-only (the degraded path).
  h.searchSemantic.mockResolvedValue([])
  h.volumeState.mockResolvedValue(state())
  h.thumbnailToken.mockResolvedValue('tok123')
  h.dropTokens.mockResolvedValue()
  h.findSimilar.mockResolvedValue([])
}

function resetAfterTest(): void {
  vi.useRealTimers()
  document.body.innerHTML = ''
  vi.clearAllMocks()
}

/** Assert the grid did no backend work at all — the whole point of an off gate. */
function expectNoIpc(): void {
  expect(h.volumeState).not.toHaveBeenCalled()
  expect(h.searchSemantic).not.toHaveBeenCalled()
  expect(h.searchOcr).not.toHaveBeenCalled()
  expect(h.thumbnailToken).not.toHaveBeenCalled()
}

describe('ImageSearchResults master-toggle gating', () => {
  beforeEach(seedDefaults)
  afterEach(resetAfterTest)

  it('feature OFF + a typed query renders no section and fires NO IPC', async () => {
    h.settings.values['mediaIndex.enabled'] = false
    const target = mountGrid()
    await settle()

    expect(target.querySelector('.image-results')).toBeNull()
    expectNoIpc()
  })

  it('feature ON renders the section and runs the OCR search', async () => {
    const target = mountGrid()
    await settle()

    expect(target.querySelector('.image-results')).not.toBeNull()
    expect(h.searchOcr).toHaveBeenCalledWith({ volumeId: 'root', query: 'invoice', limit: null })
    expect(h.volumeState).toHaveBeenCalledWith('root')
  })

  it('flipping the toggle off live-hides the section, releases tokens, and stops firing IPC', async () => {
    const target = mountGrid()
    await settle()
    expect(target.querySelector('.image-results')).not.toBeNull()
    expect(h.searchOcr).toHaveBeenCalledTimes(1)

    // Turn the master toggle off at runtime, exactly as the settings store would.
    const flipMaster = h.settings.liveChange.get('mediaIndex.enabled')
    expect(flipMaster).toBeDefined()
    flipMaster?.(false)
    flushSync()
    await settle()

    expect(target.querySelector('.image-results')).toBeNull()
    // The minted thumbnail token was released, not leaked.
    expect(h.dropTokens).toHaveBeenCalledWith(['tok123'])
    // No further OCR search fired after the flip.
    expect(h.searchOcr).toHaveBeenCalledTimes(1)

    // Turning it back on resumes: the section returns and a fresh search runs (no restart).
    flipMaster?.(true)
    flushSync()
    await settle()
    expect(target.querySelector('.image-results')).not.toBeNull()
    expect(h.searchOcr).toHaveBeenCalledTimes(2)
  })
})

describe('ImageSearchResults show-in-Search gating', () => {
  beforeEach(seedDefaults)
  afterEach(resetAfterTest)

  it('show-in-Search OFF with the master toggle ON renders no section and fires NO IPC', async () => {
    h.settings.values['mediaIndex.showInSearch'] = false
    const target = mountGrid()
    await settle()

    expect(target.querySelector('.image-results')).toBeNull()
    // The index is on and being maintained; the user just doesn't want the grid. That
    // has to cost nothing per keystroke, not merely hide the tiles after fetching them.
    expectNoIpc()
  })

  it('both gates ON renders the tiles', async () => {
    const target = mountGrid()
    await settle()

    expect(target.querySelector('.image-results')).not.toBeNull()
    expect(target.querySelectorAll('.ir-tile')).toHaveLength(1)
    expect(h.searchOcr).toHaveBeenCalledWith({ volumeId: 'root', query: 'invoice', limit: null })
  })

  it('flipping show-in-Search off live clears the grid, drops its tokens, and stops firing IPC', async () => {
    const target = mountGrid()
    await settle()
    expect(target.querySelector('.image-results')).not.toBeNull()
    expect(h.searchOcr).toHaveBeenCalledTimes(1)

    const flipShowInSearch = h.settings.liveChange.get('mediaIndex.showInSearch')
    expect(flipShowInSearch).toBeDefined()
    flipShowInSearch?.(false)
    flushSync()
    await settle()

    expect(target.querySelector('.image-results')).toBeNull()
    expect(h.dropTokens).toHaveBeenCalledWith(['tok123'])
    expect(h.searchOcr).toHaveBeenCalledTimes(1)

    // And back on, with no restart: the section returns and a fresh search runs.
    flipShowInSearch?.(true)
    flushSync()
    await settle()
    expect(target.querySelector('.image-results')).not.toBeNull()
    expect(h.searchOcr).toHaveBeenCalledTimes(2)
  })
})
