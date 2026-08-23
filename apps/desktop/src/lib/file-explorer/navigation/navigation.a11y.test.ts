/**
 * Tier 3 a11y tests for the navigation strip: both drive badges and the volume
 * breadcrumb.
 *
 * One file per component would cost about three times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, fixtures, props,
 * and assertions.
 *
 * No stub here disagrees between blocks: the two `reactive-settings` sets overlap
 * only on `getFileSizeFormat`, and agree on it. Every `$lib/*` stub spreads the real
 * module first, so a block that never stubbed one still sees its un-stubbed exports.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushSync, tick } from 'svelte'
import type { Freshness, VolumeIndexStatus } from '$lib/ipc/bindings'
import type { MediaIndexVolumeState } from '$lib/tauri-commands'
import type { VolumeIndexActivity } from '$lib/indexing'
import type { VolumeEnrichActivity } from '$lib/indexing/media-enrich-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

// The drive badge reads its own volume's live activity + phase from `index-state`;
// the image dot reads the master toggle and this volume's enrichment activity. Mock
// all three so each visible state is deterministic.
let badgeActivity: VolumeIndexActivity | undefined
let masterEnabled = true
let enrichActivity: VolumeEnrichActivity | undefined

vi.mock('$lib/indexing', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getVolumeActivity: () => badgeActivity,
  getVolumeAggregation: () => undefined,
  getVolumePhase: () => undefined,
  placeholderActivity: (volumeId: string): VolumeIndexActivity => ({
    volumeId,
    phase: 'scanning',
    entriesScanned: 0,
    dirsFound: 0,
    bytesScanned: 0,
    scanStartedAt: 0,
    priorTotalEntries: null,
    priorScanDurationMs: null,
    volumeUsedBytes: null,
    replayEventsProcessed: 0,
    replayEstimatedTotal: 0,
    replayStartedAt: 0,
  }),
}))

vi.mock('$lib/indexing/media-enrich-state.svelte', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getVolumeEnrichActivity: () => enrichActivity,
}))

vi.mock('$lib/settings/reactive-settings.svelte', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getFileSizeFormat: () => 'binary',
  getMediaIndexEnabled: () => masterEnabled,
  formatFileSize: (n: number) => `${String(n)} B`,
  getFileSizeUnit: () => 'bytes',
  getNetworkEnabled: () => true,
  // VolumeBreadcrumb's `onMount` prefetches the generic folder icon with this flag.
  getUseAppIconsForDocuments: () => false,
}))

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  resolvePathVolume: vi.fn(() => Promise.resolve({ volume: { id: 'root', path: '/' } })),
  upgradeToSmbVolume: vi.fn(() => Promise.resolve({ status: 'success' })),
  removeFavorite: vi.fn(() => Promise.resolve()),
  renameFavorite: vi.fn(() => Promise.resolve()),
  reorderFavorites: vi.fn(() => Promise.resolve()),
  stripFavoritePrefix: (id: string) => (id.startsWith('fav-') ? id.slice(4) : id),
  showVolumeRowContextMenu: vi.fn(() => Promise.resolve()),
  onVolumeContextAction: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [
    { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
    { id: 'ext', name: 'External', path: '/Volumes/External', category: 'attached_volume', isEjectable: true },
  ],
  getVolumesTimedOut: () => false,
  isVolumesRefreshing: () => false,
  isVolumeRetryFailed: () => false,
  requestVolumeRefresh: vi.fn(),
}))

vi.mock('$lib/ui/toast', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  addToast: vi.fn(() => 'toast-id'),
  dismissToast: vi.fn(),
}))

// Stub the icon cache so the `onMount` prefetch doesn't reach into `$lib/tauri-commands`
// for a `getIcons` call. Same shape as `pane/volume-breadcrumb.test.ts`.
vi.mock('$lib/icon-cache', async (importOriginal) => {
  const { writable } = await import('svelte/store')
  return {
    ...(await importOriginal<Record<string, unknown>>()),
    getCachedIcon: vi.fn().mockReturnValue('/icons/dir.png'),
    iconCacheVersion: writable(0),
    prefetchIcons: vi.fn().mockResolvedValue(undefined),
  }
})

import DriveIndexBadge from './DriveIndexBadge.svelte'
import ImageIndexDriveBadge from './ImageIndexDriveBadge.svelte'
import VolumeBreadcrumb from './VolumeBreadcrumb.svelte'

// These components share one jsdom document, the badge menu portals out of its
// container, and axe resolves ARIA id references document-wide. Clearing between
// tests keeps each audit looking at its own container only.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier 3 a11y tests for `DriveIndexBadge.svelte`: the focusable, labeled status
 * dot and its open menu must have no axe violations, in each freshness state.
 * Mirrors the `IndexingStatusIndicator` block of `$lib/indexing/stateful.a11y.test.ts`.
 */
describe('DriveIndexBadge a11y', () => {
  function makeStatus(freshness: Freshness | null, enabled = freshness != null): VolumeIndexStatus {
    return {
      volumeId: 'smb-test',
      enabled,
      freshness,
      failure: null,
      scanCompletedAt: freshness === 'fresh' ? 1_750_000_000 : null,
      scanDurationMs: freshness === 'fresh' ? 134_000 : null,
      coalescedSignalsSinceSweep: 0,
      unreadableLocations: 0,
      unreadableRetried: false,
      nextSweepDueAt: null,
    }
  }

  async function mountBadge(status: VolumeIndexStatus) {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DriveIndexBadge, {
      target,
      props: { volumeId: status.volumeId, status, driveName: 'Backups', onAction: () => {} },
    })
    await tick()
    return target
  }

  beforeEach(() => {
    badgeActivity = undefined
  })

  it('the gray (disabled) dot has no violations', async () => {
    const target = await mountBadge(makeStatus(null, false))
    expect(target.querySelector('.drive-index-badge')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the blue (scanning) dot has no violations', async () => {
    const target = await mountBadge(makeStatus('scanning'))
    await expectNoA11yViolations(target)
  })

  it('the scanning dot with the rich DOM status body has no violations', async () => {
    badgeActivity = {
      volumeId: 'smb-test',
      phase: 'scanning',
      entriesScanned: 42_000,
      dirsFound: 1_200,
      bytesScanned: 1_000_000,
      scanStartedAt: Date.now() - 4000,
      priorTotalEntries: 100_000, // calibrated → renders the progress bar too
      priorScanDurationMs: 120_000,
      volumeUsedBytes: null,
      replayEventsProcessed: 0,
      replayEstimatedTotal: 0,
      replayStartedAt: 0,
    }
    const target = await mountBadge(makeStatus('scanning'))
    expect(target.querySelector('.scan-tooltip-body')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the green (fresh) dot has no violations', async () => {
    const target = await mountBadge(makeStatus('fresh'))
    await expectNoA11yViolations(target)
  })

  it('the yellow (stale) dot has no violations', async () => {
    const target = await mountBadge(makeStatus('stale'))
    await expectNoA11yViolations(target)
  })

  it('the open menu has no violations', async () => {
    const target = await mountBadge(makeStatus('stale'))
    const badge = target.querySelector<HTMLButtonElement>('.drive-index-badge')
    expect(badge).not.toBeNull()
    badge?.click()
    flushSync()
    expect(target.querySelector('.drive-index-menu')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `ImageIndexDriveBadge.svelte`: the labeled, non-focusable
 * image-index status dot must have no axe violations in each state (off / indexing /
 * done), and must render nothing when the drive has no qualifying images.
 * Mirrors the `DriveIndexBadge` block above.
 */
describe('ImageIndexDriveBadge a11y', () => {
  /** A complete `MediaIndexVolumeState` with `enabled` and count overrides. */
  function makeState(overrides: Partial<MediaIndexVolumeState> = {}): MediaIndexVolumeState {
    return {
      enabled: true,
      indexing: false,
      enrichedCount: 0,
      qualifyingCount: 50,
      networkOptIn: false,
      alwaysIndexed: false,
      paused: false,
      waitingForImportance: false,
      coveredQualifyingCount: 50,
      keptCount: null,
      ...overrides,
    }
  }

  async function mountBadge(volumeState: MediaIndexVolumeState) {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ImageIndexDriveBadge, {
      target,
      props: { volumeId: 'vol-test', volumeState },
    })
    await tick()
    return target
  }

  beforeEach(() => {
    masterEnabled = true
    enrichActivity = undefined
  })

  it('the gray (off) dot has no violations', async () => {
    masterEnabled = false
    const target = await mountBadge(makeState())
    expect(target.querySelector('.image-index-drive-badge-off')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the yellow (indexing) dot has no violations', async () => {
    const target = await mountBadge(makeState({ enrichedCount: 12 }))
    expect(target.querySelector('.image-index-drive-badge-indexing')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the green (done) dot has no violations', async () => {
    const target = await mountBadge(makeState({ enrichedCount: 50 }))
    expect(target.querySelector('.image-index-drive-badge-done')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('renders nothing when the drive has no qualifying images', async () => {
    const target = await mountBadge(makeState({ qualifyingCount: 0, coveredQualifyingCount: 0 }))
    expect(target.querySelector('.image-index-drive-badge')).toBeNull()
  })
})

/**
 * Tier 3 a11y tests for `VolumeBreadcrumb.svelte`.
 *
 * The volume selector breadcrumb + dropdown. Only the closed state is
 * audited here; the open dropdown uses lots of CSS positioning that
 * axe doesn't reason about correctly in jsdom. Volume-store and Tauri
 * IPC are stubbed.
 */
describe('VolumeBreadcrumb a11y', () => {
  it('closed breadcrumb (local volume) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(VolumeBreadcrumb, {
      target,
      props: {
        volumeId: 'root',
        currentPath: '/Users/test',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('closed breadcrumb (network virtual volume) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(VolumeBreadcrumb, {
      target,
      props: {
        volumeId: 'network',
        currentPath: 'smb://',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
