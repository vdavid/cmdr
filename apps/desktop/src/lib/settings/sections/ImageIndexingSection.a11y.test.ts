/**
 * Tier 3 a11y + composition tests for `ImageIndexingSection.svelte` (the `Indexing › Image
 * indexing` subsection). This file OWNS the section's own contract: the master toggle + the on-device
 * privacy note always render, and the bespoke slider / network-volume controls reveal only
 * once `mediaIndex.enabled` is on. The composed children (`MediaIndexScope`,
 * `MediaIndexChosenFolders`, `MediaIndexImportanceSlider`, `MediaIndexNetworkVolumes`,
 * `MediaIndexReclaim`) have their own dedicated tests; here they mount under the same
 * deterministic IPC/prefs mocks purely to prove the gating. The mocked scope is the
 * automatic one, so the slider (a gated child this file asserts on) renders at all.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import type { CoveredCount, MediaIndexVolumeState } from '$lib/ipc/bindings'
import { expectNoA11yViolations } from '$lib/test-a11y'

const settingValues: Record<string, unknown> = {
  'mediaIndex.enabled': false,
  'mediaIndex.importanceThreshold': 0,
  'mediaIndex.scope': 'importance',
  'mediaIndex.alwaysIndexFolders': [],
}

vi.mock('$lib/settings', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getSetting: (id: string) => settingValues[id],
  setSetting: vi.fn(),
  onSpecificSettingChange: () => () => {},
}))

const coveredCount = vi.fn<(threshold: number, ids: string[]) => Promise<CoveredCount>>()
const volumeState = vi.fn<(volumeId: string) => Promise<MediaIndexVolumeState>>()
vi.mock('$lib/tauri-commands', () => ({
  mediaIndexCoveredCount: (t: number, ids: string[]) => coveredCount(t, ids),
  mediaIndexVolumeState: (v: string) => volumeState(v),
  mediaIndexFolderCoverage: () => Promise.resolve([]),
  mediaIndexClipModelStatus: () =>
    Promise.resolve({ supported: false, installed: false, configured: false, downloadBytes: 0 }),
  mediaIndexDownloadClipModel: () => Promise.resolve(),
  mediaIndexDeleteClipModel: () => Promise.resolve(),
}))

vi.mock('$lib/media-index/enabled-volumes', () => ({
  getEnabledMediaIndexVolumeIds: () => ['root'],
}))

vi.mock('$lib/media-index/always-index-folders', () => ({
  getChosenFolders: () => [],
  isFolderChosen: () => false,
  setFolderChosen: vi.fn(),
}))

vi.mock('$lib/media-index/network-volume-prefs', () => ({
  isNetworkVolumeOptedIn: () => false,
  isVolumeAlwaysIndexed: () => false,
  setNetworkVolumeOptedIn: vi.fn(),
  setVolumeAlwaysIndexed: vi.fn(),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [],
}))

vi.mock('$lib/indexing', () => ({
  ROOT_VOLUME_ID: 'root',
  getEnrichingVolumes: () => [],
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

const { default: ImageIndexingSection } = await import('./ImageIndexingSection.svelte')

function vstate(overrides: Partial<MediaIndexVolumeState> = {}): MediaIndexVolumeState {
  return {
    enabled: true,
    indexing: true,
    enrichedCount: 120,
    qualifyingCount: 500,
    networkOptIn: false,
    alwaysIndexed: false,
    paused: false,
    waitingForImportance: false,
    coveredQualifyingCount: null,
    keptCount: null,
    ...overrides,
  }
}

async function mountAndSettle(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(ImageIndexingSection, { target, props: { searchQuery: '' } })
  flushSync()
  // Let any child onMount IPC (covered-count + volume-state) resolve.
  await vi.advanceTimersByTimeAsync(300)
  await tick()
  // axe schedules via real setTimeout; leave fake timers before the a11y audit runs.
  vi.useRealTimers()
  return target
}

describe('ImageIndexingSection', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    settingValues['mediaIndex.enabled'] = false
    coveredCount.mockResolvedValue({ folders: 120, images: 3900, pending: false })
    volumeState.mockResolvedValue(vstate())
  })

  afterEach(() => {
    vi.useRealTimers()
    document.body.innerHTML = ''
    vi.clearAllMocks()
  })

  it('always shows the master toggle and the on-device privacy note, no slider when off', async () => {
    const target = await mountAndSettle()
    expect(target.querySelector('[aria-label="Index image contents"]')).not.toBeNull()
    // The privacy note is the section's own copy: on-device, no provider, no API key.
    expect(target.textContent).toContain('Vision framework')
    // Off ⇒ the refining controls are hidden.
    expect(target.querySelector('.mi-slider-thumb')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('reveals the slider and network-volume controls once image indexing is on', async () => {
    settingValues['mediaIndex.enabled'] = true
    const target = await mountAndSettle()
    // The composed slider + per-network-volume list mount under the live master toggle.
    expect(target.querySelector('.mi-slider-thumb')).not.toBeNull()
    expect(target.querySelector('.net-vols')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})
