/**
 * Tier 3 a11y tests for `ImageSearchResults.svelte` (the "text in images" OCR grid).
 *
 * Covers the coverage-honesty notices (indexing off, still indexing, not indexed yet, a
 * genuine no-match) and the populated thumbnail grid with highlighted snippets. The IPC
 * commands are mocked so the component drives each state deterministically; timers are
 * faked to fire the debounced fetch.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import type { MediaIndexVolumeState, OcrHit } from '$lib/ipc/bindings'
import { expectNoA11yViolations } from '$lib/test-a11y'

const searchOcr = vi.fn<(volumeId: string, query: string, limit: number | null) => Promise<OcrHit[]>>()
const volumeState = vi.fn<(volumeId: string) => Promise<MediaIndexVolumeState>>()
const thumbnailToken = vi.fn<(path: string) => Promise<string | null>>()
const dropTokens = vi.fn<(tokens: string[]) => Promise<void>>()

vi.mock('$lib/tauri-commands', () => ({
  mediaIndexSearchOcr: (v: string, q: string, l: number | null) => searchOcr(v, q, l),
  mediaIndexVolumeState: (v: string) => volumeState(v),
  mediaIndexThumbnailToken: (p: string) => thumbnailToken(p),
  mediaIndexDropThumbnailTokens: (t: string[]) => dropTokens(t),
}))

// The viewer's `mediaUrl`; a plain string is all the grid needs for render + axe.
vi.mock('../../routes/viewer/media-view', () => ({
  mediaUrl: (token: string) => `cmdr-media://localhost/${token}`,
}))

// Imported AFTER the mocks so the component picks them up.
const { default: ImageSearchResults } = await import('./ImageSearchResults.svelte')

function state(overrides: Partial<MediaIndexVolumeState> = {}): MediaIndexVolumeState {
  return {
    enabled: true,
    indexing: false,
    enrichedCount: 5,
    qualifyingCount: null,
    networkOptIn: false,
    alwaysIndexed: false,
    paused: false,
    ...overrides,
  }
}

async function mountAndSettle(props: Record<string, unknown> = {}): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(ImageSearchResults, {
    target,
    props: { query: 'invoice', volumeId: 'root', active: true, onOpen: () => {}, ...props },
  })
  flushSync()
  // Fire the 300 ms debounce and let the awaited IPC mocks resolve.
  await vi.advanceTimersByTimeAsync(400)
  await tick()
  // axe.run relies on real timers internally; leaving fake timers on hangs it.
  vi.useRealTimers()
  return target
}

describe('ImageSearchResults a11y', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    searchOcr.mockResolvedValue([])
    volumeState.mockResolvedValue(state())
    thumbnailToken.mockResolvedValue('tok123')
    dropTokens.mockResolvedValue()
  })

  afterEach(() => {
    vi.useRealTimers()
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('the "indexing off" notice has no a11y violations', async () => {
    volumeState.mockResolvedValue(state({ enabled: false }))
    const target = await mountAndSettle()
    expect(target.querySelector('.ir-notice')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the "still indexing" notice has no a11y violations', async () => {
    volumeState.mockResolvedValue(state({ indexing: true }))
    const target = await mountAndSettle()
    expect(target.querySelector('.ir-notice-indexing')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the "not indexed yet" notice has no a11y violations', async () => {
    volumeState.mockResolvedValue(state({ enrichedCount: 0 }))
    const target = await mountAndSettle()
    expect(target.querySelector('.ir-notice')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('a genuine no-match has no a11y violations', async () => {
    const target = await mountAndSettle()
    expect(target.querySelector('.ir-empty')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the network "not opted in" notice has no a11y violations', async () => {
    volumeState.mockResolvedValue(state({ networkOptIn: false }))
    const target = await mountAndSettle({ isNetwork: true })
    expect(target.querySelector('.ir-notice')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the network "disconnected / paused" notice has no a11y violations', async () => {
    volumeState.mockResolvedValue(state({ networkOptIn: true, paused: true }))
    const target = await mountAndSettle({ isNetwork: true, mountRoot: '/Volumes/naspi' })
    expect(target.querySelector('.ir-notice')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the populated grid with highlighted snippets has no a11y violations', async () => {
    searchOcr.mockResolvedValue([
      { path: '/photos/receipt.png', snippet: 'total [invoice] amount' },
      { path: '/photos/scan.jpg', snippet: 'an [invoice] copy' },
    ] satisfies OcrHit[])
    const target = await mountAndSettle()
    expect(target.querySelectorAll('.ir-tile').length).toBe(2)
    expect(target.querySelector('.ir-snippet mark')?.textContent).toBe('invoice')
    await expectNoA11yViolations(target)
  })
})
