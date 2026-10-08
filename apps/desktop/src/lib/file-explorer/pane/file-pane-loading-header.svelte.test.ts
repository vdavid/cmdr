/**
 * The column header stays put while a folder loads.
 *
 * A load that outlasts the 100 ms grace period swaps the rows for the loading
 * view. It used to swap the WHOLE list, header included, so entering a folder on
 * a slow volume (a GCS bucket, a busy share) made Name / Ext / Size / Modified
 * blink out for a split second in both view modes.
 */
import { describe, it, expect, vi } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'
import FilePane from './FilePane.svelte'
import { LISTING_LOADING_DELAY_MS } from './listing-presentation.svelte'
import { waitForUpdates, useMountTarget, type ListenRecorder } from './integration-test-utils'
import type { FilePaneAPI } from './types'

// ============================================================================
// Mock setup (must be in each test file: Vitest hoists vi.mock calls)
// ============================================================================

const events = vi.hoisted(() => ({ recorder: null as ListenRecorder | null }))
/** Each load's start; a test decides per load whether it ever answers. */
const listDirectoryStartMock = vi.hoisted(() => vi.fn())

vi.mock('$lib/tauri-commands', async (importOriginal) => {
  const { createListenRecorder } = await import('./integration-test-utils')
  const recorder = createListenRecorder()
  events.recorder = recorder
  return {
    ...(await importOriginal<typeof import('$lib/tauri-commands')>()),
    listDirectoryStart: listDirectoryStartMock,
    cancelListing: vi.fn().mockResolvedValue(undefined),
    listDirectoryEnd: vi.fn().mockResolvedValue(undefined),
    onListingGone: vi.fn(() => () => {}),
    keepListingsAlive: vi.fn().mockResolvedValue([]),
    getFileRange: vi.fn().mockResolvedValue([]),
    getFileAt: vi.fn().mockResolvedValue(null),
    findFileIndex: vi.fn().mockResolvedValue(null),
    getTotalCount: vi.fn().mockResolvedValue(0),
    getListingStats: vi.fn().mockResolvedValue({ data: null, timedOut: false }),
    setListingIncludeHidden: vi
      .fn()
      .mockResolvedValue({ sequence: 0, totalCount: 0, newCursorIndex: null, newSelectedIndices: null }),
    getSyncStatus: vi.fn().mockResolvedValue({ data: {}, timedOut: false }),
    onMediaEnrichProgress: vi.fn().mockResolvedValue(() => {}),
    onMediaEnrichTerminal: vi.fn().mockResolvedValue(() => {}),
    listen: recorder.listen,
    ...recorder.listingWrappers,
    showFileContextMenu: vi.fn().mockResolvedValue(undefined),
    updateMenuContext: vi.fn().mockResolvedValue(undefined),
    updateServicesSelection: vi.fn().mockResolvedValue(undefined),
    updateSelectSameKindMenu: vi.fn().mockResolvedValue(undefined),
    listVolumes: vi.fn().mockResolvedValue({
      data: [{ id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }],
      timedOut: false,
    }),
    resolvePathVolume: vi.fn().mockResolvedValue({
      volume: { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
      timedOut: false,
    }),
    getDefaultVolumeId: vi.fn().mockResolvedValue('root'),
    getVolumeSpace: vi.fn().mockResolvedValue({ data: null, timedOut: false }),
    refreshListing: vi.fn().mockResolvedValue({ data: null, timedOut: false }),
    getIcons: vi.fn().mockResolvedValue({ data: {}, timedOut: false }),
    refreshDirectoryIcons: vi.fn().mockResolvedValue({ data: {}, timedOut: false }),
    DEFAULT_VOLUME_ID: 'root',
    listNetworkHosts: vi.fn().mockResolvedValue([]),
    setServersViewShown: vi.fn().mockResolvedValue(undefined),
    getNetworkDiscoveryState: vi.fn().mockResolvedValue('idle'),
    resolveNetworkHost: vi.fn().mockResolvedValue(null),
    listMtpDevices: vi.fn().mockResolvedValue([]),
    onMtpDeviceConnected: vi.fn().mockResolvedValue(() => {}),
    onMtpDeviceDisconnected: vi.fn().mockResolvedValue(() => {}),
    onVolumeSpaceChanged: vi.fn().mockResolvedValue(() => {}),
    onWriteSourceItemDone: vi.fn().mockResolvedValue(() => {}),
    onDirectoryDiff: vi.fn().mockResolvedValue(() => {}),
    onDirectoryDeleted: vi.fn().mockResolvedValue(() => {}),
    onListingRespelled: vi.fn().mockResolvedValue(() => {}),
    onMtpExclusiveAccessError: vi.fn().mockResolvedValue(() => {}),
    onMtpPermissionError: vi.fn().mockResolvedValue(() => {}),
    onVolumeContextAction: vi.fn().mockResolvedValue(() => {}),
    notifyDialogOpened: vi.fn().mockResolvedValue(undefined),
    notifyDialogClosed: vi.fn().mockResolvedValue(undefined),
    watchVolumeSpace: vi.fn().mockResolvedValue(undefined),
    unwatchVolumeSpace: vi.fn().mockResolvedValue(undefined),
  }
})

vi.mock('$lib/icon-cache', async (importOriginal) => {
  const { writable } = await import('svelte/store')
  return {
    ...(await importOriginal<typeof import('$lib/icon-cache')>()),
    getCachedIcon: vi.fn().mockReturnValue('/icons/file.png'),
    getCachedCustomFolderIcon: () => undefined,
    iconCacheVersion: writable(0),
    prefetchIcons: vi.fn().mockResolvedValue(undefined),
    prefetchCustomFolderIcons: vi.fn().mockResolvedValue(undefined),
    evictPerPathIconsForDir: vi.fn(),
  }
})

vi.mock('$lib/settings/reactive-settings.svelte', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/settings/reactive-settings.svelte')>()),
  getRowHeight: vi.fn().mockReturnValue(24),
  getUseAppIconsForDocuments: vi.fn().mockReturnValue(true),
  getNetworkEnabled: vi.fn().mockReturnValue(true),
  getMediaIndexEnabled: vi.fn().mockReturnValue(false),
  getMediaIndexShowFileStatusIcons: vi.fn().mockReturnValue(false),
  getShowVirtualGitPortal: vi.fn().mockReturnValue(true),
}))

vi.mock('$lib/drag-drop', () => ({ startDragTracking: vi.fn() }))

vi.mock('$lib/stores/volume-store.svelte', () => ({
  getVolumes: vi
    .fn()
    .mockReturnValue([{ id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false }]),
  getVolumesTimedOut: vi.fn().mockReturnValue(false),
  isVolumesRefreshing: vi.fn().mockReturnValue(false),
  isVolumeRetryFailed: vi.fn().mockReturnValue(false),
  requestVolumeRefresh: vi.fn(),
  initVolumeStore: vi.fn().mockResolvedValue(undefined),
  cleanupVolumeStore: vi.fn(),
}))

// ============================================================================

/** A start that never answers: the pane stays in its loading state for as long as the test looks. */
function hangEveryLoad(): void {
  listDirectoryStartMock.mockImplementation(() => new Promise(() => {}))
}

/** The listing id the pane minted for its most recent load (the start's sixth argument). */
function lastListingId(): string {
  const calls = listDirectoryStartMock.mock.calls
  return calls[calls.length - 1][5] as string
}

/** Whether the column header is on screen, by its Name column (the label also carries the sort caret). */
function showsHeader(target: HTMLElement): boolean {
  return [...target.querySelectorAll('.header-row .sortable-header')].some((el) =>
    el.textContent.trim().startsWith('Name'),
  )
}

describe('FilePane column header during a load', () => {
  const { getTarget } = useMountTarget()

  for (const viewMode of ['full', 'brief'] as const) {
    it(`keeps the ${viewMode} header while a folder outlasts the grace period`, async () => {
      // The first folder lands, so there are settled rows; the second one hangs.
      listDirectoryStartMock.mockImplementation((_v, _p, _h, _sb, _so, listingId: string) =>
        Promise.resolve({ listingId, status: { status: 'loading' } }),
      )
      const props = $state({
        initialPath: '/first',
        volumeId: 'root',
        volumePath: '/',
        isFocused: true,
        showHiddenFiles: true,
        viewMode,
      })
      const component = mount(FilePane, { target: getTarget(), props })
      const pane = component as unknown as FilePaneAPI
      await waitForUpdates()
      events.recorder?.fireListingEvent('listing-complete', {
        listingId: lastListingId(),
        totalCount: 3,
        volumeRoot: '/',
      })
      await waitForUpdates()
      expect(pane.isLoading()).toBe(false)
      expect(showsHeader(getTarget())).toBe(true)

      hangEveryLoad()
      props.initialPath = '/slow'
      flushSync()
      await waitForUpdates(LISTING_LOADING_DELAY_MS + 50)

      expect(pane.isLoading()).toBe(true)
      expect(getTarget().querySelector('.loading-container'), 'the loading view shows').toBeTruthy()
      expect(showsHeader(getTarget()), 'the header stays').toBe(true)
      await unmount(component)
    })
  }

  it('shows the header from the very first load, with no settled rows yet', async () => {
    hangEveryLoad()
    const component = mount(FilePane, {
      target: getTarget(),
      props: {
        initialPath: '/slow',
        volumeId: 'root',
        volumePath: '/',
        isFocused: true,
        showHiddenFiles: true,
        viewMode: 'full',
      },
    })
    const pane = component as unknown as FilePaneAPI
    await waitForUpdates(LISTING_LOADING_DELAY_MS + 50)

    expect(pane.isLoading()).toBe(true)
    expect(getTarget().querySelector('.loading-container')).toBeTruthy()
    expect(showsHeader(getTarget())).toBe(true)
    // No settled rows means nothing to call empty: the loading view says what's happening.
    expect(getTarget().querySelector('.empty-folder-message, .empty-folder-overlay')).toBeNull()
    await unmount(component)
  })
})
