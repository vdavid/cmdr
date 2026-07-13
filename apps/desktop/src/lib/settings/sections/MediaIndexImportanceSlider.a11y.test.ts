/**
 * Tier 3 a11y + behavior tests for `MediaIndexImportanceSlider.svelte` (the M2 image-index
 * "how much to index" slider).
 *
 * Covers the default bucket label (threshold 0.0 ⇒ the broadest "everywhere" bucket), the
 * live covered-count preview, the always-skipped floor line, and the per-volume local
 * progress line — driving each off mocked IPC so the render is deterministic. The slider's
 * persist + live-apply path is covered by the `media-index-slider` E2E spec (Ark UI's
 * pointer/keyboard drag isn't reliably drivable in jsdom).
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import type { CoveredCount, MediaIndexVolumeState } from '$lib/ipc/bindings'
import { expectNoA11yViolations } from '$lib/test-a11y'

const coveredCount = vi.fn<(threshold: number, volumeIds: string[]) => Promise<CoveredCount>>()
const volumeState = vi.fn<(volumeId: string) => Promise<MediaIndexVolumeState>>()
const getSetting = vi.fn<(id: string) => unknown>()
const setSetting = vi.fn<(id: string, value: unknown) => void>()

vi.mock('$lib/settings', () => ({
  getSetting: (id: string) => getSetting(id),
  setSetting: (id: string, value: unknown) => {
    setSetting(id, value)
  },
  onSpecificSettingChange: () => () => {},
}))

vi.mock('$lib/tauri-commands', () => ({
  mediaIndexCoveredCount: (t: number, ids: string[]) => coveredCount(t, ids),
  mediaIndexVolumeState: (v: string) => volumeState(v),
}))

vi.mock('$lib/media-index/network-volume-prefs', () => ({
  getNetworkOptInVolumes: () => [],
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [],
}))

vi.mock('$lib/indexing', () => ({
  ROOT_VOLUME_ID: 'root',
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

const { default: MediaIndexImportanceSlider } = await import('./MediaIndexImportanceSlider.svelte')

function vstate(overrides: Partial<MediaIndexVolumeState> = {}): MediaIndexVolumeState {
  return {
    enabled: true,
    indexing: true,
    enrichedCount: 120,
    qualifyingCount: 500,
    networkOptIn: false,
    alwaysIndexed: false,
    paused: false,
    ...overrides,
  }
}

async function mountAndSettle(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(MediaIndexImportanceSlider, { target })
  flushSync()
  // Let the onMount IPC (covered-count + volume-state) resolve.
  await vi.advanceTimersByTimeAsync(300)
  await tick()
  vi.useRealTimers()
  return target
}

describe('MediaIndexImportanceSlider', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    getSetting.mockReturnValue(0) // threshold 0.0 → broadest bucket by default
    coveredCount.mockResolvedValue({ folders: 120, images: 3900, pending: false })
    volumeState.mockResolvedValue(vstate())
  })

  afterEach(() => {
    vi.useRealTimers()
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('defaults to the broadest bucket and shows the live covered-count preview', async () => {
    const target = await mountAndSettle()
    // The primary label reflects the default (threshold 0.0 = the rightmost "everywhere" bucket).
    expect(target.querySelector('.mi-slider-value')?.textContent ?? '').toContain('Everywhere')
    // The honest preview reads the mocked counts (thousands-separated).
    const preview = target.querySelector('.mi-preview')?.textContent ?? ''
    expect(preview).toContain('3,900')
    expect(preview).toContain('120')
    // The always-skipped floor line is present and legible.
    expect(target.querySelector('.mi-floor')?.textContent ?? '').toMatch(/node_modules/)
    await expectNoA11yViolations(target)
  })

  it('shows honest per-volume local progress ("N of M")', async () => {
    const target = await mountAndSettle()
    const line = target.querySelector('.mi-progress-line')?.textContent ?? ''
    expect(line).toContain('120')
    expect(line).toContain('500')
  })

  it('voices "still counting" when the qualifying total is unknown', async () => {
    volumeState.mockResolvedValue(vstate({ enrichedCount: 0, qualifyingCount: null }))
    const target = await mountAndSettle()
    // Counting (qualifyingCount null + nothing enriched) shows a line rather than a fabricated total.
    expect(target.querySelector('.mi-progress-line')?.textContent ?? '').not.toBe('')
  })

  it('caveats the preview when an enabled volume is still scanning', async () => {
    coveredCount.mockResolvedValue({ folders: 12, images: 3400, pending: true })
    const target = await mountAndSettle()
    expect(target.querySelector('.mi-preview-pending')).not.toBeNull()
  })

  it('says "nothing matches" only when the count is a settled zero', async () => {
    coveredCount.mockResolvedValue({ folders: 0, images: 0, pending: false })
    const target = await mountAndSettle()
    const preview = target.querySelector('.mi-preview')?.textContent ?? ''
    expect(preview.toLowerCase()).toContain('nothing')
  })

  it('shows a done line once every qualifying image is indexed', async () => {
    volumeState.mockResolvedValue(vstate({ indexing: false, enrichedCount: 500, qualifyingCount: 500 }))
    const target = await mountAndSettle()
    expect(target.querySelector('.mi-progress-line')?.textContent ?? '').toContain('500')
  })

  it('moving the slider commits the new threshold and re-queries the preview', async () => {
    const target = await mountAndSettle()
    const thumb = target.querySelector('.mi-slider-thumb') as HTMLElement
    thumb.focus()
    // ArrowLeft moves one bucket toward "most-used only" (from threshold 0.0 → 0.2).
    thumb.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true, cancelable: true }))
    await vi.waitFor(() => {
      expect(setSetting).toHaveBeenCalledWith('mediaIndex.importanceThreshold', 0.2)
    })
    // The debounced preview re-runs at the new threshold.
    await vi.waitFor(() => {
      expect(coveredCount).toHaveBeenCalledWith(0.2, expect.arrayContaining(['root']))
    })
  })
})
