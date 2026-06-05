/**
 * Transactional-navigation regression suite — the braid-layer seam.
 *
 * Pins the `navigate(intent)` transaction at the mounted-coordinator layer.
 * These tests mount the REAL `DualPaneExplorer` + `FilePane` and drive listing
 * events through the capture-and-replay `listen` recorder (`createListenRecorder`,
 * `integration-test-utils.ts`). The recorder is what lets a synthetic
 * `listing-complete` / `listing-error` flow through `FilePane`'s real
 * listing-id gate into the coordinator's `onPathChange` handler — the only way
 * to express the stale-listing drop (scenario 1) and the optimistic-commit
 * ordering (scenario 8) at the braid layer.
 *
 * Assertions are on OBSERVABLE OUTCOMES (committed pane state read off the
 * explorer store, history depth, persisted-call spies), never internal function
 * identities.
 *
 * The capture-prop (mocked-FilePane) seam (b) scenarios — pinned-tab fork,
 * cancel/MTP/unreachable/unmount edge flows, and the refusal strings — live in
 * the sibling `navigation-transaction-handlers.test.ts`.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

const events = vi.hoisted(() => ({
  recorder: null as ReturnType<typeof import('./integration-test-utils').createListenRecorder> | null,
}))
const listDirectoryStartMock = vi.hoisted(() => vi.fn().mockResolvedValue({ listingId: '', status: {} }))

vi.mock('$lib/app-status-store', () => ({
  loadAppStatus: vi.fn().mockResolvedValue({
    leftPath: '/Users/me',
    rightPath: '/Users/me',
    focusedPane: 'left',
    leftVolumeId: 'root',
    rightVolumeId: 'root',
    leftSortBy: 'name',
    rightSortBy: 'name',
    leftViewMode: 'brief',
    rightViewMode: 'brief',
    leftPaneWidthPercent: 50,
  }),
  saveAppStatus: vi.fn(),
  getLastUsedPathForVolume: vi.fn().mockResolvedValue(undefined),
  saveLastUsedPathForVolume: vi.fn().mockResolvedValue(undefined),
  loadPaneTabs: vi.fn().mockResolvedValue({
    tabs: [
      {
        id: 'left-tab',
        path: '/Users/me',
        volumeId: 'root',
        sortBy: 'name',
        sortOrder: 'ascending',
        viewMode: 'brief',
        pinned: false,
      },
    ],
    activeTabId: 'left-tab',
  }),
  savePaneTabs: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

vi.mock('$lib/tauri-commands', async (importOriginal) => {
  const { createListenRecorder } = await import('./integration-test-utils')
  const recorder = createListenRecorder()
  events.recorder = recorder
  // Spread the real module so rarely-hit exports (network/MTP discovery hooks)
  // resolve to their real (invoke-backed) implementations; `@tauri-apps/api/core`
  // `invoke` is itself mocked, so they're inert. Override the I/O we drive.
  const actual = await importOriginal<typeof import('$lib/tauri-commands')>()
  return {
    ...actual,
    pathExists: vi.fn().mockResolvedValue(true),
    pathExistsChecked: vi.fn().mockResolvedValue({ data: true, timedOut: false }),
    listDirectoryStart: listDirectoryStartMock,
    listDirectoryEnd: vi.fn().mockResolvedValue(undefined),
    cancelListing: vi.fn().mockResolvedValue(undefined),
    findFileIndex: vi.fn().mockResolvedValue(null),
    findFirstFuzzyMatch: vi.fn().mockResolvedValue(null),
    getFileRange: vi.fn().mockResolvedValue({ entries: [] }),
    getFileAt: vi.fn().mockResolvedValue(null),
    getDirStatsBatch: vi.fn().mockResolvedValue([]),
    getDirSizeTooltip: vi.fn().mockResolvedValue(''),
    getListingStats: vi.fn().mockResolvedValue({ data: null, timedOut: false }),
    getPathsAtIndices: vi.fn().mockResolvedValue([]),
    getSyncStatus: vi.fn().mockResolvedValue({ data: {}, timedOut: false }),
    getTotalCount: vi.fn().mockResolvedValue(0),
    refreshListingIndexSizes: vi.fn().mockResolvedValue(undefined),
    openFile: vi.fn().mockResolvedValue(undefined),
    getIcons: vi.fn().mockResolvedValue({ data: {}, timedOut: false }),
    listen: recorder.listen,
    showFileContextMenu: vi.fn(() => Promise.resolve()),
    updateMenuContext: vi.fn(() => Promise.resolve()),
    getRestrictedPaths: vi.fn().mockResolvedValue([]),
    hasFontMetrics: vi.fn().mockResolvedValue(true),
    storeFontMetrics: vi.fn().mockResolvedValue(undefined),
    listVolumes: vi.fn().mockResolvedValue({
      data: [
        { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
        { id: 'ext', name: 'Ext', path: '/Volumes/Ext', category: 'attached_volume', isEjectable: true },
      ],
      timedOut: false,
    }),
    getBusyVolumeIds: vi.fn().mockResolvedValue([]),
    resolvePathVolume: vi.fn().mockResolvedValue({
      volume: { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
      timedOut: false,
    }),
    getDefaultVolumeId: vi.fn().mockResolvedValue('root'),
    getVolumeSpace: vi.fn().mockResolvedValue({ data: null, timedOut: false }),
    refreshListing: vi.fn().mockResolvedValue({ data: null, timedOut: false }),
    DEFAULT_VOLUME_ID: 'root',
    getE2eStartPath: vi.fn().mockResolvedValue(null),
    formatBytes: vi.fn().mockReturnValue('0 B'),
    updateFocusedPane: vi.fn().mockResolvedValue(undefined),
    resortListing: vi.fn().mockResolvedValue({}),
    listNetworkHosts: vi.fn().mockResolvedValue([]),
    getNetworkDiscoveryState: vi.fn().mockResolvedValue('idle'),
    resolveNetworkHost: vi.fn().mockResolvedValue(null),
    listMtpDevices: vi.fn().mockResolvedValue([]),
    onMtpDeviceConnected: vi.fn().mockResolvedValue(() => {}),
    onMtpDeviceDisconnected: vi.fn().mockResolvedValue(() => {}),
    onMtpExclusiveAccessError: vi.fn().mockResolvedValue(() => {}),
    onMtpPermissionError: vi.fn().mockResolvedValue(() => {}),
    updatePaneTabs: vi.fn().mockResolvedValue(undefined),
    updatePinTabMenu: vi.fn().mockResolvedValue(undefined),
    setReopenClosedTabEnabled: vi.fn().mockResolvedValue(undefined),
    showTabContextMenu: vi.fn().mockResolvedValue(null),
    updateViewModeMenu: vi.fn().mockResolvedValue(undefined),
    watchVolumeSpace: vi.fn().mockResolvedValue(undefined),
    unwatchVolumeSpace: vi.fn().mockResolvedValue(undefined),
    showBreadcrumbContextMenu: vi.fn().mockResolvedValue(undefined),
    upgradeToSmbVolumeWithCredentials: vi.fn().mockResolvedValue(undefined),
    disconnectSmbVolume: vi.fn().mockResolvedValue(undefined),
    ejectVolume: vi.fn().mockResolvedValue(undefined),
    onVolumeContextAction: vi.fn().mockResolvedValue(() => {}),
    openInEditor: vi.fn().mockResolvedValue(undefined),
    getIpcErrorMessage: (e: unknown) => String(e),
  }
})

vi.mock('$lib/tauri-commands/ipc-types', () => ({ getIpcErrorMessage: (e: unknown) => String(e) }))

vi.mock('$lib/settings-store', () => ({
  loadSettings: vi.fn().mockResolvedValue({ showHiddenFiles: true }),
  saveSettings: vi.fn().mockResolvedValue(undefined),
  subscribeToSettingsChanges: vi.fn().mockResolvedValue(() => {}),
}))

vi.mock('$lib/settings', () => ({
  initializeSettings: vi.fn().mockResolvedValue(undefined),
  getSetting: vi.fn().mockReturnValue(undefined),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

// Real FilePane renders BriefList, which reads density/row-height from
// reactive-settings. The `$lib/settings` mock returns `undefined` for every
// setting, which would crash `getRowHeight`'s `densityMappings[uiDensity]`
// lookup. Keep the real module (its date/size formatters are used by
// SelectionInfo / DateLabel) and override only the settings-backed getters with
// safe defaults so the list renders without a live settings store.
vi.mock('$lib/settings/reactive-settings.svelte', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/settings/reactive-settings.svelte')>()),
  getRowHeight: vi.fn().mockReturnValue(24),
  getIconSize: vi.fn().mockReturnValue(16),
  getIsCompactDensity: vi.fn().mockReturnValue(false),
  getUseAppIconsForDocuments: vi.fn().mockReturnValue(false),
  getDirectorySortMode: vi.fn().mockReturnValue('likeFiles'),
  getIsCmdrGold: vi.fn().mockReturnValue(false),
  getSizeDisplayMode: vi.fn().mockReturnValue('size'),
  getSizeMismatchWarning: vi.fn().mockReturnValue(false),
  getFileSizeUnit: vi.fn().mockReturnValue('binary'),
  getFileSizeFormat: vi.fn().mockReturnValue('short'),
  getStripedRows: vi.fn().mockReturnValue(false),
  getBriefColumnWidthMode: vi.fn().mockReturnValue('auto'),
  getBriefColumnWidthMaxPx: vi.fn().mockReturnValue(400),
  getNetworkEnabled: vi.fn().mockReturnValue(true),
  getTypeToJumpResetDelay: vi.fn().mockReturnValue(1000),
}))

import DualPaneExplorer from './DualPaneExplorer.svelte'
import { explorerState, _resetForTesting } from './explorer-state.svelte'
import { getActiveTab, pushHistoryEntry } from '../tabs/tab-state-manager.svelte'
import { saveLastUsedPathForVolume } from '$lib/app-status-store'
import type { NavigateResult } from './navigate'

type ExplorerHandle = {
  navigate: (intent: {
    pane: 'left' | 'right'
    to: { volumeId?: string; path: string }
    source: 'user' | 'mcp'
  }) => NavigateResult
  selectVolumeByName: (pane: 'left' | 'right', name: string) => Promise<boolean>
}

/** Mount the explorer and let initialization (paths, volumes, settings, first listing) settle. */
async function mountExplorer(): Promise<{ target: HTMLDivElement; handle: ExplorerHandle }> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const handle = mount(DualPaneExplorer, { target }) as unknown as ExplorerHandle
  for (let i = 0; i < 20; i++) await tick()
  await new Promise((r) => setTimeout(r, 30))
  await tick()
  return { target, handle }
}

/** Most recent listingId minted by ANY pane (the freshest live listener). */
function latestListingId(): string {
  const calls = listDirectoryStartMock.mock.calls
  return calls[calls.length - 1]?.[5] as string
}

/**
 * Drive ONLY the left pane to a fresh path on `root` and return the listingId it
 * minted. Both panes start on the same path/volume, so the mount-time
 * `listDirectoryStart` calls are indistinguishable by args; re-driving the left
 * pane in isolation gives a deterministic, left-owned listingId to fire later.
 */
async function driveLeftLoad(handle: ExplorerHandle, path: string): Promise<string> {
  listDirectoryStartMock.mockClear()
  // The FilePane navigateToPath promise (`result.settled`) rejects with
  // "Superseded by new navigation" when the next load supersedes this one (which
  // happens in every test that drives a second load or unmounts before firing
  // completion). Swallow that expected rejection so it doesn't surface as an
  // unhandled error.
  const result = handle.navigate({ pane: 'left', to: { path }, source: 'user' })
  if (result.status === 'started') void result.settled.catch(() => {})
  for (let i = 0; i < 5; i++) await tick()
  await new Promise((r2) => setTimeout(r2, 10))
  return latestListingId()
}

function leftTab() {
  return getActiveTab(explorerState.getTabMgr('left'))
}
function rightTab() {
  return getActiveTab(explorerState.getTabMgr('right'))
}

beforeEach(() => {
  _resetForTesting()
  events.recorder?.reset()
  listDirectoryStartMock.mockClear()
  vi.mocked(saveLastUsedPathForVolume).mockClear()
})

describe('listen capture-and-replay helper (seam a smoke test)', () => {
  it('records listeners and fires them with the {payload} event shape', async () => {
    const { createListenRecorder } = await import('./integration-test-utils')
    const recorder = createListenRecorder()
    const received: unknown[] = []
    const unlisten = await recorder.listen('listing-complete', (e) => received.push(e.payload))

    expect(recorder.listenerCount('listing-complete')).toBe(1)
    recorder.fireListingEvent('listing-complete', { listingId: 'x', totalCount: 3 })
    expect(received).toEqual([{ listingId: 'x', totalCount: 3 }])

    // Unlisten drops the callback — a later fire reaches no one.
    unlisten()
    expect(recorder.listenerCount('listing-complete')).toBe(0)
    recorder.fireListingEvent('listing-complete', { listingId: 'y', totalCount: 9 })
    expect(received).toEqual([{ listingId: 'x', totalCount: 3 }])
  })

  it('fires every live listener for an event name', async () => {
    const { createListenRecorder } = await import('./integration-test-utils')
    const recorder = createListenRecorder()
    const a: unknown[] = []
    const b: unknown[] = []
    await recorder.listen('listing-error', (e) => a.push(e.payload))
    await recorder.listen('listing-error', (e) => b.push(e.payload))
    recorder.fireListingEvent('listing-error', { listingId: 'z' })
    expect(a).toEqual([{ listingId: 'z' }])
    expect(b).toEqual([{ listingId: 'z' }])
  })
})

describe('scenario 1: stale onPathChange after a volume flip (L6) is dropped', () => {
  it('real-volume branch: drops a stale listing-complete whose path is not on the pane new volume', async () => {
    const { handle } = await mountExplorer()

    // Start a left-pane load for a real path on root; capture its (now in-flight) id.
    const staleId = await driveLeftLoad(handle, '/Users/me/deep')
    expect(staleId).toBeTruthy()

    // The user flips the pane to a DIFFERENT real volume before the load completes.
    const tab = leftTab()
    tab.volumeId = 'ext'
    tab.path = '/Volumes/Ext'
    await tick() // let the volume-path $derived recompute

    vi.mocked(saveLastUsedPathForVolume).mockClear()

    // The stale completion lands for /Users/me/deep, which is NOT on /Volumes/Ext.
    events.recorder?.fireListingEvent('listing-complete', {
      listingId: staleId,
      totalCount: 0,
      volumeRoot: '/',
    })
    for (let i = 0; i < 5; i++) await tick()
    await new Promise((r) => setTimeout(r, 10))

    // Dropped: the path stays the new volume's path, no foreign last-used-path write.
    expect(leftTab().path).toBe('/Volumes/Ext')
    expect(leftTab().volumeId).toBe('ext')
    const foreignWrites = vi.mocked(saveLastUsedPathForVolume).mock.calls.filter((c) => c[1] === '/Users/me/deep')
    expect(foreignWrites).toEqual([])
  })

  it('network branch: drops a stale listing-complete whose path lacks the smb:// prefix', async () => {
    const { handle } = await mountExplorer()
    const staleId = await driveLeftLoad(handle, '/Users/me/deep')

    // Flip to the virtual network volume (path scheme is smb://, not a real prefix).
    const tab = leftTab()
    tab.volumeId = 'network'
    tab.path = 'smb://'
    await tick()

    events.recorder?.fireListingEvent('listing-complete', {
      listingId: staleId,
      totalCount: 0,
      volumeRoot: '/',
    })
    for (let i = 0; i < 5; i++) await tick()

    // A non-smb path is dropped on a network pane.
    expect(leftTab().path).toBe('smb://')
  })

  it('search-results branch: drops a stale listing-complete whose path lacks the search-results:// prefix', async () => {
    const { handle } = await mountExplorer()
    const staleId = await driveLeftLoad(handle, '/Users/me/deep')

    const tab = leftTab()
    tab.volumeId = 'search-results'
    tab.path = 'search-results://sr-1'
    await tick()

    events.recorder?.fireListingEvent('listing-complete', {
      listingId: staleId,
      totalCount: 0,
      volumeRoot: '/',
    })
    for (let i = 0; i < 5; i++) await tick()

    expect(leftTab().path).toBe('search-results://sr-1')
  })

  it('commits a non-stale listing-complete whose path IS on the current volume', async () => {
    const { handle } = await mountExplorer()
    const id = await driveLeftLoad(handle, '/Users/me/deep')

    const depthBefore = leftTab().history.stack.length
    events.recorder?.fireListingEvent('listing-complete', {
      listingId: id,
      totalCount: 0,
      volumeRoot: '/',
    })
    for (let i = 0; i < 5; i++) await tick()
    await new Promise((r) => setTimeout(r, 10))

    // /Users/me/deep is on root → committed (path advances, history pushed).
    expect(leftTab().path).toBe('/Users/me/deep')
    expect(leftTab().history.stack.length).toBe(depthBefore + 1)
  })
})

describe('scenario 8: optimistic-commit ordering (P4)', () => {
  it('in-place path nav commits the coordinator pane path only when the listing settles, then once', async () => {
    const { handle } = await mountExplorer()
    expect(leftTab().path).toBe('/Users/me')

    listDirectoryStartMock.mockClear()

    // Drive an in-place path navigation. `navigate()` calls the FilePane
    // primitive, which mints a new listingId and starts loading.
    const result = handle.navigate({ pane: 'left', to: { path: '/Users/me/sub' }, source: 'user' })
    // The in-place arm STARTS (returns the FilePane settle promise), never refuses.
    expect(result.status).toBe('started')
    if (result.status === 'started') void result.settled.catch(() => {})

    // Let the load kick off (a fresh listingId is minted) but do NOT fire its
    // completion yet.
    for (let i = 0; i < 5; i++) await tick()
    await new Promise((r) => setTimeout(r, 10))

    expect(listDirectoryStartMock).toHaveBeenCalled()
    const newId = latestListingId()

    // CURRENT BRAID: the coordinator pane path (active-tab path read off the
    // store) has NOT yet advanced to the target — the in-place commit lives in
    // `applyPathChange`, fired from `onPathChange` at `listing-complete`. This
    // pins the settle-coupled commit of the in-place arm (the timing
    // `navigate-and-select`'s `await settled` depends on). The optimistic
    // volume-switch arm (committed synchronously, before any listing) is pinned
    // in the handlers suite.
    expect(leftTab().path).toBe('/Users/me')

    // Now settle the listing → the commit lands exactly once.
    const depthBefore = leftTab().history.stack.length
    events.recorder?.fireListingEvent('listing-complete', {
      listingId: newId,
      totalCount: 0,
      volumeRoot: '/',
    })
    for (let i = 0; i < 5; i++) await tick()
    await new Promise((r) => setTimeout(r, 10))

    expect(leftTab().path).toBe('/Users/me/sub')
    // History gains exactly one entry for the new path (single commit).
    expect(leftTab().history.stack.length).toBe(depthBefore + 1)
    expect(getActiveTab(explorerState.getTabMgr('left')).history.stack.at(-1)?.path).toBe('/Users/me/sub')
  })

  it('volume switch commits volume + path + history SYNCHRONOUSLY, before any listing (optimistic)', async () => {
    const { handle } = await mountExplorer()
    expect(leftTab().volumeId).toBe('root')
    const depthBefore = leftTab().history.stack.length

    // A volume switch goes through `handleVolumeChange`, which commits the pane
    // state immediately (no await before the commit). Assert the commit is
    // observable right after the call resolves its synchronous part, BEFORE any
    // listing-complete fires — this is the P4 optimism guard against an
    // accidental validate-then-commit rewrite.
    void handle.selectVolumeByName('left', 'Ext')
    // Don't fire any listing event. Let the synchronous commit + microtasks run.
    await tick()

    expect(leftTab().volumeId).toBe('ext')
    expect(leftTab().path).toBe('/Volumes/Ext')
    expect(leftTab().history.stack.length).toBe(depthBefore + 1)
    expect(leftTab().history.stack.at(-1)).toMatchObject({ volumeId: 'ext', path: '/Volumes/Ext' })
  })
})

describe('volume-unmount redirect (per-pane, NO history push)', () => {
  it('redirects each affected pane to the default volume at ~ WITHOUT pushing history', async () => {
    await mountExplorer()

    // Put BOTH panes on the volume that will be unmounted, at different depths.
    const left = leftTab()
    left.volumeId = 'ext'
    left.path = '/Volumes/Ext/photos'
    left.history = pushHistoryEntry(
      { stack: [{ volumeId: 'ext', path: '/Volumes/Ext' }], currentIndex: 0 },
      { volumeId: 'ext', path: '/Volumes/Ext/photos' },
    )
    const right = rightTab()
    right.volumeId = 'ext'
    right.path = '/Volumes/Ext'
    right.history = { stack: [{ volumeId: 'ext', path: '/Volumes/Ext' }], currentIndex: 0 }
    await tick()

    const leftDepthBefore = leftTab().history.stack.length
    const rightDepthBefore = rightTab().history.stack.length

    // The backend emits `volume-unmounted` with the unmounted volume's PATH; the
    // DPE listener maps it to the volume id and redirects each affected pane.
    events.recorder?.fireListingEvent('volume-unmounted', { volumePath: '/Volumes/Ext' })
    for (let i = 0; i < 6; i++) await tick()
    await new Promise((r) => setTimeout(r, 10))

    // Both panes redirected to the default volume at ~ …
    expect(leftTab().volumeId).toBe('root')
    expect(leftTab().path).toBe('~')
    expect(rightTab().volumeId).toBe('root')
    expect(rightTab().path).toBe('~')

    // … and NO history entry was pushed (the history-push asymmetry: a fallback
    // redirect for an unmounted volume must not grow a Back target).
    expect(leftTab().history.stack.length).toBe(leftDepthBefore)
    expect(rightTab().history.stack.length).toBe(rightDepthBefore)
  })
})
