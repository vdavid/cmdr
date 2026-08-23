/**
 * Tier 3 a11y + behavior tests for the image-index components in this directory:
 * the `Indexing › Image indexing` section and the pieces it composes.
 *
 * They share one file because `svelte-tests` charges per test FILE, not per test
 * (`docs/testing.md` § "What a test actually costs"), and this family pulls the
 * heaviest import graph in the directory. They also share one mock surface: the
 * same `$lib/settings`, `$lib/tauri-commands`, volume-store, media-index prefs,
 * and logger stubs, with the per-component differences routed through mutable
 * implementations that each block installs in its own `beforeEach`. Nothing is
 * merged into a single shared value: every block gets exactly the stubs it was
 * written against.
 *
 * The plainer settings sections live in `sections.a11y.test.ts`; the CLIP model and
 * chosen-folder cards in `media-index-model-and-folders.a11y.test.ts` (they need none
 * of this harness, and one merged file would clear the 800-line `file-length` mark).
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import type { CoveredCount, MediaIndexVolumeState, ReclaimPreview } from '$lib/ipc/bindings'
import type { VolumeEnrichActivity } from '$lib/indexing/media-enrich-state.svelte'
import type { VolumeInfo } from '$lib/file-explorer/types'
import { expectNoA11yViolations } from '$lib/test-a11y'

/**
 * Every stub a block may need to re-point. Each `describe` sets what it cares
 * about in `beforeEach`; the file-level `beforeEach` re-arms the neutral
 * defaults first, so no block can inherit another's setup.
 */
const stubs = vi.hoisted(() => ({
  // `null` means "use the real `$lib/settings` export": `MediaIndexProgressSummary`
  // reads settings for real, and only the blocks whose own file stubbed these
  // install a replacement.
  getSetting: null as ((id: string) => unknown) | null,
  settingDefinition: null as ((id: string) => unknown) | null,
  setSetting: vi.fn<(id: string, value: unknown) => void>(),
  coveredCount: vi.fn<(threshold: number, volumeIds: string[]) => Promise<unknown>>(),
  volumeState: vi.fn<(volumeId: string) => Promise<unknown>>(),
  reclaimPreview: vi.fn<(threshold: number, volumeIds: string[]) => Promise<unknown>>(),
  pruneBelowThreshold: vi.fn<(...args: unknown[]) => unknown>(),
  folderCoverage: vi.fn<() => Promise<unknown>>(),
  maxParallelism: vi.fn<() => Promise<number>>(),
  volumes: [] as VolumeInfo[],
  enrichingVolumes: [] as VolumeEnrichActivity[],
  enabledVolumeIds: ['root'],
  networkOptedIn: false,
  volumeAlwaysIndexed: false,
  networkOptInVolumes: [] as string[],
  setNetworkVolumeOptedIn: vi.fn<(id: string, on: boolean) => Promise<void>>(),
  setVolumeAlwaysIndexed: vi.fn<(id: string, on: boolean) => Promise<void>>(),
}))

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  const realGetSetting = actual.getSetting as (id: string) => unknown
  const realGetSettingDefinition = actual.getSettingDefinition as (id: string) => unknown
  return {
    ...actual,
    getSetting: (id: string) => (stubs.getSetting ? stubs.getSetting(id) : realGetSetting(id)),
    setSetting: (id: string, value: unknown) => {
      stubs.setSetting(id, value)
    },
    getSettingDefinition: (id: string) =>
      stubs.settingDefinition ? stubs.settingDefinition(id) : realGetSettingDefinition(id),
    onSpecificSettingChange: () => () => {},
  }
})

vi.mock('$lib/tauri-commands', () => ({
  mediaIndexCoveredCount: (t: number, ids: string[]) => stubs.coveredCount(t, ids),
  mediaIndexVolumeState: (v: string) => stubs.volumeState(v),
  mediaIndexReclaimPreview: (t: number, ids: string[]) => stubs.reclaimPreview(t, ids),
  mediaIndexPruneBelowThreshold: (...args: unknown[]) => stubs.pruneBelowThreshold(...args),
  mediaIndexFolderCoverage: () => stubs.folderCoverage(),
  getMediaIndexMaxParallelism: () => stubs.maxParallelism(),
}))

vi.mock('$lib/media-index/enabled-volumes', () => ({
  getEnabledMediaIndexVolumeIds: () => stubs.enabledVolumeIds,
}))

vi.mock('$lib/media-index/network-volume-prefs', () => ({
  isNetworkVolumeOptedIn: () => stubs.networkOptedIn,
  isVolumeAlwaysIndexed: () => stubs.volumeAlwaysIndexed,
  setNetworkVolumeOptedIn: (id: string, on: boolean) => stubs.setNetworkVolumeOptedIn(id, on),
  setVolumeAlwaysIndexed: (id: string, on: boolean) => stubs.setVolumeAlwaysIndexed(id, on),
  getNetworkOptInVolumes: () => stubs.networkOptInVolumes,
}))

vi.mock('$lib/stores/volume-store.svelte', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getVolumes: () => stubs.volumes,
}))

vi.mock('$lib/indexing', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  ROOT_VOLUME_ID: 'root',
  getEnrichingVolumes: () => stubs.enrichingVolumes,
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import ImageIndexingSection from './ImageIndexingSection.svelte'
import MediaIndexImportanceSlider from './MediaIndexImportanceSlider.svelte'
import MediaIndexNetworkVolumes from './MediaIndexNetworkVolumes.svelte'
import MediaIndexProgressSummary from './MediaIndexProgressSummary.svelte'
import MediaIndexReclaim from './MediaIndexReclaim.svelte'
import MediaIndexScope from './MediaIndexScope.svelte'

/** The default `MediaIndexVolumeState`, with the un-scored fallback counts. */
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
    // Default to the un-scored fallback (whole-drive count path); the covered/kept tests
    // below opt into the threshold-aware split explicitly.
    coveredQualifyingCount: null,
    keptCount: null,
    ...overrides,
  }
}

/** A fresh container, appended to the document and ready to mount into. */
function container(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  return target
}

beforeEach(() => {
  stubs.getSetting = null
  stubs.settingDefinition = null
  stubs.volumes = []
  stubs.enrichingVolumes = []
  stubs.enabledVolumeIds = ['root']
  stubs.networkOptedIn = false
  stubs.volumeAlwaysIndexed = false
  stubs.networkOptInVolumes = []
  stubs.folderCoverage.mockResolvedValue([])
  stubs.maxParallelism.mockResolvedValue(8)
  // Inert defaults for the children a block composes but doesn't assert on
  // (the slider and the scope each host a `MediaIndexReclaim`). Zeros keep the
  // reclaim offer below its floor, so it renders nothing and the audited DOM is
  // the block's own component, exactly as in each component's own file.
  stubs.coveredCount.mockResolvedValue({ folders: 0, images: 0, pending: false })
  stubs.volumeState.mockResolvedValue(null)
  stubs.reclaimPreview.mockResolvedValue({
    totalStored: 0,
    coveredStored: 0,
    doomedCount: 0,
    estimatedBytes: 0,
    pending: false,
  })
  stubs.setNetworkVolumeOptedIn.mockResolvedValue()
  stubs.setVolumeAlwaysIndexed.mockResolvedValue()
})

afterEach(() => {
  vi.useRealTimers()
  document.body.innerHTML = ''
  vi.clearAllMocks()
})

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
describe('ImageIndexingSection', () => {
  const settingValues: Record<string, unknown> = {}

  async function mountAndSettle(): Promise<HTMLElement> {
    const target = container()
    mount(ImageIndexingSection, { target, props: { searchQuery: '' } })
    flushSync()
    // Let any child onMount IPC (covered-count + volume-state) resolve.
    await vi.advanceTimersByTimeAsync(300)
    await tick()
    // axe schedules via real setTimeout; leave fake timers before the a11y audit runs.
    vi.useRealTimers()
    return target
  }

  beforeEach(() => {
    vi.useFakeTimers()
    settingValues['mediaIndex.enabled'] = false
    settingValues['mediaIndex.importanceThreshold'] = 0
    settingValues['mediaIndex.scope'] = 'importance'
    settingValues['mediaIndex.alwaysIndexFolders'] = []
    stubs.getSetting = (id: string) => settingValues[id]
    stubs.coveredCount.mockResolvedValue({ folders: 120, images: 3900, pending: false } satisfies CoveredCount)
    stubs.volumeState.mockResolvedValue(vstate())
  })

  it('always shows the master toggle and the on-device privacy note, no slider when off', async () => {
    const target = await mountAndSettle()
    expect(target.querySelector('[aria-label="Index image contents"]')).not.toBeNull()
    // The privacy note is the section's own copy: on-device, no provider, no API key.
    expect(target.textContent).toContain('Vision framework')
    // Off ⇒ the refining controls are hidden.
    expect(target.querySelector('[data-test="media-importance-threshold"]')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('reveals the slider and network-volume controls once image indexing is on', async () => {
    settingValues['mediaIndex.enabled'] = true
    const target = await mountAndSettle()
    // The composed slider + per-network-volume list mount under the live master toggle.
    expect(target.querySelector('[data-test="media-importance-threshold"]')).not.toBeNull()
    expect(target.querySelector('.net-vols')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y + behavior tests for `MediaIndexImportanceSlider.svelte` (the image-index
 * "how much to index" slider).
 *
 * Covers the default bucket label (threshold 0.0 ⇒ the broadest "everywhere" bucket), the
 * live covered-count preview, the always-skipped floor line, and the per-volume local
 * progress line — driving each off mocked IPC so the render is deterministic. The slider's
 * persist + live-apply path is covered by the `media-index-slider` E2E spec (Ark UI's
 * pointer/keyboard drag isn't reliably drivable in jsdom).
 */
describe('MediaIndexImportanceSlider', () => {
  async function mountAndSettle(): Promise<HTMLElement> {
    const target = container()
    mount(MediaIndexImportanceSlider, { target, props: {} })
    flushSync()
    // Let the onMount IPC (covered-count + volume-state) resolve.
    await vi.advanceTimersByTimeAsync(300)
    await tick()
    vi.useRealTimers()
    return target
  }

  beforeEach(() => {
    vi.useFakeTimers()
    // threshold 0.0 → broadest bucket by default; every other key reads 0 too, as
    // this component's own file mocked `getSetting` to a flat 0.
    stubs.getSetting = () => 0
    stubs.coveredCount.mockResolvedValue({ folders: 120, images: 3900, pending: false } satisfies CoveredCount)
    stubs.volumeState.mockResolvedValue(vstate())
  })

  it('defaults to the broadest bucket and shows the live covered-count preview', async () => {
    const target = await mountAndSettle()
    // The primary label reflects the default (threshold 0.0 = the rightmost "everywhere" bucket).
    expect(target.querySelector('.mi-slider .sl-value-above')?.textContent ?? '').toContain('Everywhere')
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

  it('voices the drive-scan wait when the drive genuinely is still scanning', async () => {
    stubs.volumeState.mockResolvedValue(vstate({ enrichedCount: 0, qualifyingCount: null }))
    stubs.coveredCount.mockResolvedValue({ folders: 0, images: 0, pending: true } satisfies CoveredCount)
    const target = await mountAndSettle()
    // No qualifying total AND the covered count reports a volume not ready ⇒ the drive
    // index is genuinely still scanning, so the preview says exactly that — the "I flipped
    // the switch and nothing happened" answer — instead of a generic counting line.
    expect(target.querySelector('.mi-preview')?.textContent ?? '').toContain('drive scan is still running')
    // One honest line, not two: the per-volume progress line stays out.
    expect(target.querySelector('.mi-progress-line')).toBeNull()
  })

  it('does not claim a drive scan when the backend simply has no count cached yet', async () => {
    // `mediaIndexVolumeState` is a poll: it reads the backend's coverage counts and never
    // builds them (a cold build is a whole-index walk). So `qualifyingCount: null` also
    // covers "nobody has counted yet" on a fully-scanned drive — claiming a drive scan
    // there would be a lie. The covered count came back resolved, so the drive IS ready.
    stubs.volumeState.mockResolvedValue(vstate({ enrichedCount: 0, qualifyingCount: null }))
    stubs.coveredCount.mockResolvedValue({ folders: 12, images: 340, pending: false } satisfies CoveredCount)
    const target = await mountAndSettle()
    const preview = target.querySelector('.mi-preview')?.textContent ?? ''
    expect(preview).not.toContain('drive scan is still running')
    expect(preview).toContain('340')
  })

  it('caveats the preview when an enabled volume is still scanning', async () => {
    stubs.coveredCount.mockResolvedValue({ folders: 12, images: 3400, pending: true } satisfies CoveredCount)
    const target = await mountAndSettle()
    expect(target.querySelector('.mi-preview-pending')).not.toBeNull()
  })

  it('says "nothing matches" only when the count is a settled zero', async () => {
    stubs.coveredCount.mockResolvedValue({ folders: 0, images: 0, pending: false } satisfies CoveredCount)
    const target = await mountAndSettle()
    const preview = target.querySelector('.mi-preview')?.textContent ?? ''
    expect(preview.toLowerCase()).toContain('nothing')
  })

  it('shows the threshold-aware covered progress and the quiet kept-rows line', async () => {
    // 1,000 stored, 50 outside coverage (kept), 900 qualifying in covered folders ⇒
    // 950 indexed inside coverage caps to the 900 covered total (done), and the 50 kept
    // rows show as a quiet still-searchable line (below the reclaim-offer floor).
    stubs.volumeState.mockResolvedValue(
      vstate({ enrichedCount: 1000, keptCount: 50, coveredQualifyingCount: 900, qualifyingCount: 2000 }),
    )
    const target = await mountAndSettle()
    const line = target.querySelector('.mi-progress-line')?.textContent ?? ''
    expect(line).toContain('900')
    expect(line.toLowerCase()).toContain('covered')
    const kept = target.querySelector('.mi-kept')?.textContent ?? ''
    expect(kept).toContain('50')
    expect(kept.toLowerCase()).toContain('searchable')
    await expectNoA11yViolations(target)
  })

  it('hides the kept line when the fuller reclaim offer would show instead (one narrative)', async () => {
    // 50,000 kept of 100,000 stored ⇒ over the reclaim floor ⇒ the reclaim component owns
    // the narrative, so the quiet kept line stays hidden (never two sentences in tension).
    stubs.volumeState.mockResolvedValue(
      vstate({ enrichedCount: 100_000, keptCount: 50_000, coveredQualifyingCount: 60_000 }),
    )
    const target = await mountAndSettle()
    expect(target.querySelector('.mi-kept')).toBeNull()
  })

  it('shows a done line once every qualifying image is indexed', async () => {
    stubs.volumeState.mockResolvedValue(vstate({ indexing: false, enrichedCount: 500, qualifyingCount: 500 }))
    const target = await mountAndSettle()
    expect(target.querySelector('.mi-progress-line')?.textContent ?? '').toContain('500')
  })

  it('moving the slider commits the new threshold and re-queries the preview', async () => {
    const target = await mountAndSettle()
    const thumb = target.querySelector('[data-test="media-importance-threshold"]') as HTMLElement
    thumb.focus()
    // ArrowLeft moves one bucket toward "most-used only" (from threshold 0.0 → 0.2).
    thumb.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true, cancelable: true }))
    await vi.waitFor(() => {
      expect(stubs.setSetting).toHaveBeenCalledWith('mediaIndex.importanceThreshold', 0.2)
    })
    // The debounced preview re-runs at the new threshold.
    await vi.waitFor(() => {
      expect(stubs.coveredCount).toHaveBeenCalledWith(0.2, expect.arrayContaining(['root']))
    })
  })
})

/**
 * Tests for `MediaIndexNetworkVolumes.svelte` (the per-network-volume opt-in +
 * "always index" controls in the Image search settings card). Mounts the component
 * with a stubbed network volume and mocked IPC/prefs, asserting the opt-in wiring and
 * running an axe tier-3 audit. All external deps are mocked so the render is
 * deterministic.
 */
describe('MediaIndexNetworkVolumes', () => {
  function makeState(overrides: Partial<MediaIndexVolumeState> = {}): MediaIndexVolumeState {
    return {
      enabled: true,
      indexing: false,
      enrichedCount: 0,
      qualifyingCount: null,
      networkOptIn: stubs.networkOptedIn,
      alwaysIndexed: false,
      paused: false,
      waitingForImportance: false,
      coveredQualifyingCount: null,
      keptCount: null,
      ...overrides,
    }
  }

  async function mountAndSettle(): Promise<HTMLElement> {
    const target = container()
    mount(MediaIndexNetworkVolumes, { target, props: {} })
    flushSync()
    await vi.advanceTimersByTimeAsync(50)
    await tick()
    vi.useRealTimers()
    return target
  }

  beforeEach(() => {
    vi.useFakeTimers()
    // Same as `MediaIndexChosenFolders`: its own file left `$lib/settings` with
    // `onSpecificSettingChange` alone, so nothing here reads a setting.
    stubs.getSetting = () => undefined
    stubs.volumes = [
      { id: 'smb-naspi', name: 'naspi', path: '/Volumes/naspi', category: 'network', isEjectable: true },
      { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
    ]
    stubs.networkOptedIn = false
    stubs.volumeState.mockResolvedValue(makeState())
    stubs.setNetworkVolumeOptedIn.mockResolvedValue()
    stubs.setVolumeAlwaysIndexed.mockResolvedValue()
  })

  it('lists only network volumes (not local ones)', async () => {
    const target = await mountAndSettle()
    const names = [...target.querySelectorAll('.net-name')].map((n) => (n.textContent || '').trim())
    expect(names).toEqual(['naspi'])
  })

  it('toggling the opt-in switch calls the persist+apply helper', async () => {
    const target = await mountAndSettle()
    // The hooks sit on the hidden input (the primitive forwards `data-*` there); the
    // styled track is `aria-hidden` decoration.
    const input = target.querySelector('input[data-test="media-net-optin"][data-volume-id="smb-naspi"]')
    expect(input).not.toBeNull()
    ;(input as HTMLElement).click()
    await tick()
    expect(stubs.setNetworkVolumeOptedIn).toHaveBeenCalledWith('smb-naspi', true)
  })

  it('the opted-out list has no a11y violations', async () => {
    const target = await mountAndSettle()
    await expectNoA11yViolations(target)
  })

  it('the opted-in list (with always-index row + status) has no a11y violations', async () => {
    stubs.networkOptedIn = true
    stubs.volumeState.mockResolvedValue(makeState({ networkOptIn: true, enrichedCount: 12 }))
    const target = await mountAndSettle()
    expect(target.querySelector('.net-status')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `MediaIndexProgressSummary.svelte`: the live per-volume
 * image-indexing progress summary shown in the "Enable indexing" settings card. It wraps
 * the shared `IndexingEnrichRow`, so the only local logic is the enriching-volumes gate
 * (renders nothing when idle) and the drive-name resolution. `getEnrichingVolumes` and the
 * volume store are mocked; `tString` resolves the real `en` catalog.
 */
describe('MediaIndexProgressSummary a11y', () => {
  function activity(overrides: Partial<VolumeEnrichActivity> = {}): VolumeEnrichActivity {
    return {
      volumeId: 'root',
      done: 1_200,
      total: 5_000,
      bytesDone: 2_000_000,
      bytesTotal: 9_000_000,
      paused: null,
      startedAt: Date.now() - 4000,
      ...overrides,
    }
  }

  async function mountSummary(): Promise<HTMLElement> {
    const target = container()
    mount(MediaIndexProgressSummary, { target, props: {} })
    await tick()
    return target
  }

  beforeEach(() => {
    stubs.volumes = [{ id: 'root', name: 'Macintosh HD' } as VolumeInfo]
  })

  it('renders nothing while no volume is enriching', async () => {
    stubs.enrichingVolumes = []
    const target = await mountSummary()
    expect(target.querySelector('.mi-progress')).toBeNull()
    target.remove()
  })

  it('a single enriching drive (images + bytes bars) has no violations', async () => {
    stubs.enrichingVolumes = [activity()]
    const target = await mountSummary()
    expect(target.querySelector('.mi-progress')).not.toBeNull()
    expect(target.querySelectorAll('[role="progressbar"]').length).toBe(2)
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('multiple enriching drives have no violations', async () => {
    stubs.enrichingVolumes = [activity(), activity({ volumeId: 'smb-nas', done: 40, total: 900 })]
    const target = await mountSummary()
    expect(target.querySelectorAll('.enrich-row').length).toBeGreaterThanOrEqual(2)
    await expectNoA11yViolations(target)
    target.remove()
  })
})

/**
 * Tier 3 a11y + visibility tests for `MediaIndexReclaim.svelte` (the "delete the extra
 * indexed entries" line + button under the image-index slider).
 *
 * The line renders only once counts settle AND the leftover clears the `shouldOfferReclaim`
 * floor, so each state is driven off a mocked reclaim-preview. The prune round-trip
 * (confirm → prune → toast) is covered by the reclaim E2E; here we pin that the offered
 * state is accessible and the blocked / below-floor states render nothing.
 */
describe('MediaIndexReclaim', () => {
  function preview(overrides: Partial<ReclaimPreview> = {}): ReclaimPreview {
    return {
      totalStored: 200_000,
      coveredStored: 150,
      doomedCount: 199_850,
      estimatedBytes: 1_900_000_000,
      pending: false,
      ...overrides,
    }
  }

  async function mountReclaim(props: Record<string, unknown>): Promise<HTMLElement> {
    const target = container()
    mount(MediaIndexReclaim, { target, props: { threshold: 0.0, blocked: false, ...props } })
    flushSync()
    await vi.waitFor(() => {
      // Let the effect-driven preview fetch resolve.
      expect(stubs.reclaimPreview).toHaveBeenCalled()
    })
    await tick()
    return target
  }

  beforeEach(() => {
    stubs.reclaimPreview.mockResolvedValue(preview())
  })

  it('offers the reclaim line + button and is accessible when leftover is large', async () => {
    const target = await mountReclaim({})
    const line = target.querySelector('.mi-reclaim-line')?.textContent ?? ''
    expect(line).toContain('200,000')
    expect(line).toContain('199,850')
    expect(target.querySelector('button')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('renders nothing while blocked (waiting on importance / a scan)', async () => {
    const target = container()
    mount(MediaIndexReclaim, { target, props: { threshold: 0.0, blocked: true } })
    flushSync()
    await tick()
    expect(target.querySelector('.mi-reclaim')).toBeNull()
    expect(stubs.reclaimPreview).not.toHaveBeenCalled()
  })

  it('renders nothing when the leftover is below the offer floor', async () => {
    stubs.reclaimPreview.mockResolvedValue(preview({ totalStored: 1000, coveredStored: 990, doomedCount: 10 }))
    const target = await mountReclaim({})
    expect(target.querySelector('.mi-reclaim')).toBeNull()
  })

  it('renders nothing while the backend reports pending', async () => {
    stubs.reclaimPreview.mockResolvedValue(preview({ pending: true }))
    const target = await mountReclaim({})
    expect(target.querySelector('.mi-reclaim')).toBeNull()
  })
})

/**
 * Tier 3 a11y + behavior tests for `MediaIndexScope.svelte` (which folders image
 * indexing may cover).
 *
 * The load-bearing assertion is the slider's visibility: it exists only in the automatic
 * scope, because in the narrow one it has no effect at all and showing it would promise a
 * control that does nothing.
 */
describe('MediaIndexScope', () => {
  let scope = 'chosen'

  function mountScope(): HTMLElement {
    const target = container()
    mount(MediaIndexScope, { target, props: {} })
    flushSync()
    return target
  }

  beforeEach(() => {
    // This component's own file read `mediaIndex.scope` from a variable and every
    // other key as 0; the slider and reclaim children are real here (these tests
    // assert WHETHER they render, not what — each has its own suite), so the IPC and
    // stores they reach for are stubbed to empty.
    scope = 'chosen'
    stubs.getSetting = (id: string) => (id === 'mediaIndex.scope' ? scope : 0)
    stubs.settingDefinition = () => ({
      label: 'Which folders to index',
      constraints: {
        options: [
          { value: 'chosen', label: 'Only folders I choose' },
          { value: 'importance', label: 'Automatically, by folder importance' },
        ],
      },
    })
    stubs.coveredCount.mockResolvedValue({ folders: 0, images: 0, pending: false } satisfies CoveredCount)
    stubs.volumeState.mockResolvedValue(null)
    stubs.reclaimPreview.mockResolvedValue({
      totalStored: 0,
      coveredStored: 0,
      doomedCount: 0,
      estimatedBytes: 0,
      pending: false,
    } satisfies ReclaimPreview)
  })

  it('offers both scopes and hides the slider in the narrow one', async () => {
    const target = mountScope()
    const text = target.textContent
    expect(text).toContain('Only folders I choose')
    expect(text).toContain('Automatically, by folder importance')
    expect(target.querySelector('.mi-slider')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('keeps the reclaim offer reachable in the narrow scope', async () => {
    // The reclaim offer normally rides inside the slider. Narrowing is exactly when there
    // are leftover rows to free, so losing the offer with the slider would strand the disk
    // space; this component hosts its own instance instead.
    const target = mountScope()
    await vi.waitFor(() => {
      expect(stubs.reclaimPreview).toHaveBeenCalled()
    })
    // And not twice: the automatic scope's instance lives inside the slider.
    expect(target.querySelectorAll('.mi-slider').length).toBe(0)
  })

  it('shows the importance slider in the automatic scope', async () => {
    scope = 'importance'
    const target = mountScope()
    expect(target.querySelector('.mi-slider')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('commits the picked scope', async () => {
    const target = mountScope()
    const automatic = target.querySelector('input[value="importance"]') as HTMLInputElement
    automatic.click()
    await vi.waitFor(() => {
      expect(stubs.setSetting).toHaveBeenCalledWith('mediaIndex.scope', 'importance')
    })
  })
})
