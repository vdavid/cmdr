/**
 * Tier 3 a11y tests for the navigation strip: both drive badges, the volume breadcrumb, and
 * the two menus that hang off it.
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
import ServersPinHintToastContent from './ServersPinHintToastContent.svelte'

// The drive badge reads its own volume's live activity + phase from `index-state`;
// the image dot reads the master toggle and this volume's enrichment activity. Mock
// all three so each visible state is deterministic.
let badgeActivity: VolumeIndexActivity | undefined
let masterEnabled = true
/** Whether the switcher's favorites section opens unfolded, as the setting remembers it. */
let switcherFavoritesExpanded = false
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
  // `volume-capabilities` reads it to classify a `.git`-portal path, which the favorites
  // menu's add row asks about.
  getShowVirtualGitPortal: () => false,
  getSwitcherFavoritesExpanded: () => switcherFavoritesExpanded,
}))

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  resolvePathVolume: vi.fn(() => Promise.resolve({ volume: { id: 'root', path: '/' } })),
  upgradeToSmbVolume: vi.fn(() => Promise.resolve({ status: 'success' })),
  removeFavorite: vi.fn(() => Promise.resolve()),
  renameFavorite: vi.fn(() => Promise.resolve()),
  reorderFavorites: vi.fn(() => Promise.resolve()),
  stripFavoritePrefix: (id: string) => (id.startsWith('fav-') ? id.slice(4) : id),
  onVolumeContextAction: vi.fn(() => Promise.resolve(() => {})),
  // The switcher fetches disk space on open; nothing to show keeps the rows plain.
  getVolumeSpace: vi.fn(() => Promise.resolve(null)),
  addFavorite: vi.fn(() => Promise.resolve()),
  trackEvent: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: () => [
    // Two favorites, so the menu renders its number column and its reorderable section.
    { id: 'fav-1', name: 'Documents', path: '/Users/test/Documents', category: 'favorite', isEjectable: false },
    { id: 'fav-2', name: 'Downloads', path: '/Users/test/Downloads', category: 'favorite', isEjectable: false },
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

import ConnectionDot from './ConnectionDot.svelte'
import DetachButton from './DetachButton.svelte'
import DriveIndexBadge from './DriveIndexBadge.svelte'
import ImageIndexDriveBadge from './ImageIndexDriveBadge.svelte'
import UsbSpeedDot from './UsbSpeedDot.svelte'
import FavoritesMenu from './FavoritesMenu.svelte'
import VolumeBreadcrumb from './VolumeBreadcrumb.svelte'
import VolumeChooserMenu from './VolumeChooserMenu.svelte'
import FavoriteRowLabel from './FavoriteRowLabel.svelte'
import FavoriteShortcutField from './FavoriteShortcutField.svelte'
import VolumeSpaceLine from './VolumeSpaceLine.svelte'
import { SvelteMap, SvelteSet } from 'svelte/reactivity'
import type { SpaceInfo } from '$lib/tauri-commands'
import type { VolumeInfo } from '../types'
import type { FavoritesMenuController } from './favorites-menu.svelte'
import type { VolumeSpaceManager } from './volume-space-manager.svelte'
import type { DriveBadges } from './drive-badges.svelte'

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
      liveWatch: true,
    }
  }

  async function mountBadge(status: VolumeIndexStatus) {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(DriveIndexBadge, {
      target,
      props: { volumeId: status.volumeId, status, driveName: 'Backups', answers: true, onAction: () => {} },
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
    // The body mounts only while the tooltip is open.
    target.querySelector('.drive-index-badge')?.dispatchEvent(new MouseEvent('mouseenter'))
    flushSync()
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

  // The menu is the house `Menu`, which portals its surface to the body, so axe is pointed at
  // the document rather than the mount target: scanning the target alone would find the dot
  // and miss the whole menu.
  it('the open menu has no violations', async () => {
    const target = await mountBadge(makeStatus('stale'))
    const badge = target.querySelector<HTMLButtonElement>('.drive-index-badge')
    expect(badge).not.toBeNull()
    badge?.click()
    flushSync()
    await tick()
    await tick()
    expect(document.querySelector('[data-menu]')).not.toBeNull()
    await expectNoA11yViolations(document.body)
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
        paneId: 'left' as const,
        volumeId: 'root',
        currentPath: '/Users/test',
        containingVolumeId: 'root',
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
        paneId: 'left' as const,
        volumeId: 'network',
        currentPath: 'smb://',
        containingVolumeId: null,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/** Tier 3 a11y for `ServersPinHintToastContent.svelte`. */
describe('ServersPinHintToastContent a11y', () => {
  it('has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ServersPinHintToastContent, { target, props: { toastId: 'pin-hint' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y for the three row/chip pieces the chip and the switcher share, and for the
 * switcher's list itself. The list is the house `Menu`, so its surface rules are audited in
 * `$lib/ui/overlays.a11y.test.ts`; what's audited here is what THIS consumer puts in a row.
 */
describe('switcher row pieces a11y', () => {
  /** A container in the document, since axe resolves ARIA references document-wide. */
  function hostElement(): HTMLDivElement {
    const target = document.createElement('div')
    document.body.appendChild(target)
    return target
  }

  it('the connection dot has no violations, in every state', async () => {
    for (const state of [
      'direct',
      'os_mount',
      'disconnected',
      'needs_sign_in',
      'needs_host_key_approval',
      'saved',
    ] as const) {
      document.body.innerHTML = ''
      const target = hostElement()
      mount(ConnectionDot, { target, props: { state } })
      await tick()
      expect(target.querySelector(`.smb-indicator-${state}`)).not.toBeNull()
      await expectNoA11yViolations(target)
    }
  })

  it('the USB-speed dot has no violations', async () => {
    const target = hostElement()
    mount(UsbSpeedDot, { target, props: { speed: 'super' } })
    await tick()
    expect(target.querySelector('.usb-speed-indicator-super')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the detach button has no violations, idle and while ejecting', async () => {
    const target = hostElement()
    mount(DetachButton, {
      target,
      props: { label: 'Eject Backup', icon: 'eject', disabled: false, ejecting: false, onclick: () => {} },
    })
    await tick()
    expect(target.querySelector('button')?.getAttribute('aria-label')).toBe('Eject Backup')
    await expectNoA11yViolations(target)

    document.body.innerHTML = ''
    const ejecting = hostElement()
    mount(DetachButton, {
      target: ejecting,
      props: { label: 'Ejecting Backup…', icon: 'eject', disabled: true, ejecting: true, onclick: () => {} },
    })
    await tick()
    await expectNoA11yViolations(ejecting)
  })
})

/**
 * Tier 3 a11y for `VolumeChooserMenu.svelte`, OPEN: the rows it builds carry a checkmark, an
 * icon, a label, and a trailing cluster, and the menu portals out of its container — so the
 * audit looks at the whole document.
 */
describe('VolumeChooserMenu a11y', () => {
  /** The index dots are the chip's to own, so the list takes them as a prop; nothing to show. */
  const noBadges: DriveBadges = {
    statusFor: () => undefined,
    imageStateFor: () => undefined,
    fetchForRows: () => {},
    runAction: () => {},
    destroy: () => {},
  }

  // Folded, the favorites section is one disclosure row (`aria-expanded`); unfolded, its
  // favorite rows sit in the same group under it, with their own submenus.
  it.each([
    ['folded', false],
    ['unfolded', true],
  ])('the open list has no a11y violations, favorites section %s', async (_label, expanded) => {
    switcherFavoritesExpanded = expanded
    const target = document.createElement('div')
    document.body.appendChild(target)
    const anchor = document.createElement('span')
    document.body.appendChild(anchor)
    const instance = mount(VolumeChooserMenu, {
      target,
      props: {
        paneId: 'left' as const,
        volumeId: 'root',
        currentPath: '/',
        containingVolumeId: 'root',
        badges: noBadges,
        getAnchor: () => anchor,
        getChipCluster: () => anchor,
        onSwitchPane: () => {},
        onShowFavorites: () => {},
        onOpenChange: () => {},
      },
    }) as unknown as { open: () => void }
    flushSync()
    instance.open()
    // The surface portals itself into `document.body`, which lands a beat after the open.
    await vi.waitFor(() => {
      expect(document.querySelector('[data-menu-row="root"]')).not.toBeNull()
    })
    expect(document.querySelector('[data-menu-row="fav-1"]') !== null).toBe(expanded)
    await expectNoA11yViolations(document.body)
    switcherFavoritesExpanded = false
  })
})

/**
 * Tier 3 a11y for `FavoritesMenu.svelte`, OPEN. Its rows carry a leading number column the
 * switcher's don't, announced through `aria-keyshortcuts`, and the last row is deliberately
 * DISABLED here (the pane's folder is already a favorite), which is the state most likely to
 * announce as an unlabelled dead end. It portals out of its container, so the audit looks at
 * the whole document.
 */
describe('FavoritesMenu a11y', () => {
  it('the open menu has no a11y violations, disabled add row included', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const anchor = document.createElement('span')
    document.body.appendChild(anchor)
    const instance = mount(FavoritesMenu, {
      target,
      props: {
        paneId: 'left' as const,
        volumeId: 'root',
        // The store's first favorite points here, so the `0` row renders disabled with its
        // reason as the tooltip.
        currentPath: '/Users/test/Documents',
        getAnchor: () => anchor,
        getChipCluster: () => anchor,
        onShowVolumes: () => {},
        onOpenChange: () => {},
      },
    }) as unknown as { open: (trigger: 'command') => void }
    flushSync()
    instance.open('command')
    // The surface portals itself into `document.body`, which lands a beat after the open.
    await vi.waitFor(() => {
      expect(document.querySelector('[data-menu-row="favorites:add"]')).not.toBeNull()
    })
    expect(document.querySelector('[data-menu-row="favorites:add"][data-disabled]')).not.toBeNull()
    await expectNoA11yViolations(document.body)
  })
})

/**
 * Tier 3 a11y for the pieces a favorite or volume row carries inside both menus: the favorite's
 * label (plain, quiet, and as the rename field), its shortcut capture, and a volume's disk-space
 * line in each of its states. The menus' own blocks above mount them folded away, so each state
 * is audited here directly.
 */
describe('favorite and volume row pieces a11y', () => {
  const favorite: VolumeInfo = {
    id: 'fav-1',
    name: 'Documents',
    path: '/Users/test/Documents',
    category: 'favorite',
    isEjectable: false,
  }

  /** Only what the two row pieces read off the controller; the rest isn't reached. */
  function stubFavorites(state: {
    renaming?: boolean
    editingShortcut?: boolean
    dimmed?: boolean
  }): FavoritesMenuController {
    return {
      renamingFavoriteId: state.renaming ? favorite.id : null,
      renameDraft: favorite.name,
      editingShortcutId: state.editingShortcut ? favorite.id : null,
      isDimmed: () => state.dimmed ?? false,
      handleRenameKeyDown: () => {},
      commitRename: () => Promise.resolve(),
      handleShortcutKeyDown: () => {},
      cancelShortcutEdit: () => {},
    } as unknown as FavoritesMenuController
  }

  function hostElement(): HTMLDivElement {
    const target = document.createElement('div')
    document.body.appendChild(target)
    return target
  }

  it.each<[string, { renaming?: boolean; dimmed?: boolean }]>([
    ['ready', {}],
    ['quiet (a pick connects first)', { dimmed: true }],
    ['renaming', { renaming: true }],
  ])('the favorite label has no violations, %s', async (_label, state) => {
    const target = hostElement()
    mount(FavoriteRowLabel, {
      target,
      props: { favorites: stubFavorites(state), volume: favorite, label: 'Documents' },
    })
    await tick()
    expect(target.querySelector(state.renaming ? 'input' : '.favorite-label')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  it('the shortcut capture has no violations while setting a shortcut', async () => {
    const target = hostElement()
    mount(FavoriteShortcutField, {
      target,
      props: { favorites: stubFavorites({ editingShortcut: true }), volume: favorite },
    })
    await tick()
    expect(target.querySelector('.favorite-shortcut-input')).not.toBeNull()
    await expectNoA11yViolations(target)
  })

  function stubSpaceManager(state: { space?: SpaceInfo; retrying?: boolean; timedOut?: boolean }): VolumeSpaceManager {
    const volumeSpaceMap = new SvelteMap<string, SpaceInfo>()
    if (state.space) volumeSpaceMap.set('root', state.space)
    return {
      volumeSpaceMap,
      spaceTimedOutSet: new SvelteSet(state.timedOut ? ['root'] : []),
      spaceRetryingSet: new SvelteSet(state.retrying ? ['root'] : []),
      spaceRetryFailedSet: new SvelteSet<string>(),
      spaceRetryAttemptedSet: new SvelteSet<string>(),
      spaceAutoRetryingSet: new SvelteSet<string>(),
      retryVolumeSpace: () => {},
    } as unknown as VolumeSpaceManager
  }

  it.each([
    ['bounded', { space: { kind: 'bounded', totalBytes: 1000, availableBytes: 400, usedBytes: 600 } as SpaceInfo }],
    ['unbounded', { space: { kind: 'unbounded', usedBytes: 600 } as SpaceInfo }],
    ['retrying', { retrying: true }],
    ['timed out', { timedOut: true }],
  ])('the disk-space line has no violations, %s', async (_label, state) => {
    const target = hostElement()
    const volume: VolumeInfo = {
      id: 'root',
      name: 'Macintosh HD',
      path: '/',
      category: 'main_volume',
      isEjectable: false,
    }
    mount(VolumeSpaceLine, { target, props: { volume, spaceManager: stubSpaceManager(state) } })
    await tick()
    expect(target.querySelector('.volume-space-info')).not.toBeNull()
    await expectNoA11yViolations(target)
  })
})
