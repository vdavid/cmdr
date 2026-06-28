/**
 * Transactional-navigation regression suite — the handler/render-prop seam (b) +
 * the refusal-string contract.
 *
 * Pins the coordinator handlers that don't need a real listing event to reach:
 * the pinned-tab fork (`onPathChange` / `onVolumeChange` landings), the
 * snapshot-pane cross-volume routing, the edge-flow handlers
 * (`handleCancelLoading` / `handleMtpFatalError` / `handleRetryUnreachable` /
 * `handleOpenHome` / `handleVolumeUnmount`), and the MCP `navigate` refusal
 * strings (L12). It mounts `DualPaneExplorer` with `FilePane` MOCKED to a
 * prop-capturing stub, so a test can invoke the exact render-prop callback
 * `DualPaneExplorer` wired into the pane (`onPathChange`, `onVolumeChange`,
 * `onCancelLoading`, `onMtpFatalError`, `onRetryUnreachable`, `onOpenHome`) and
 * assert the coordinator's observable outcome on the explorer store.
 *
 * Assertions are on OBSERVABLE OUTCOMES (committed pane state, tab structure,
 * history depth, returned refusal strings, persisted-call spies), never internal
 * function identities.
 *
 * The braid-layer seam (a) scenarios (stale-listing drop, optimistic-commit
 * ordering) live in the sibling `navigation-transaction.test.ts`, which mounts
 * the real `FilePane` + the capture-and-replay `listen` recorder.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

/** Props the mocked FilePane last received, keyed by pane id. */
const captured = vi.hoisted((): { props: Record<string, Record<string, unknown>> } => ({ props: {} }))
/** When set to a pane id, the FilePane mock returns no instance for that pane,
 * so `getPaneRef(pane)` is undefined — used to pin the "Pane not available"
 * refusal branch. */
const suppressRef = vi.hoisted(() => ({ paneId: null as string | null }))

// A no-op function for every method the coordinator may call on a pane ref
// (`getListingId`, `navigateToPath`, `setNetworkHost`, `clearJumpState`, …). The
// instance is a plain object of named no-ops rather than a Proxy: a Proxy that
// answers EVERY property (incl. Svelte's internal symbols) corrupts mount.
function makePaneRefStub(): Record<string, () => unknown> {
  const names = [
    'getListingId',
    'navigateToPath',
    'navigateToParent',
    'setNetworkHost',
    'setNetworkAutoMount',
    'clearJumpState',
    'cancelRename',
    'getCursorEntry',
    'getNetworkCursorEntry',
    'getPathUnderCursor',
    'handleCancelLoading',
    'whenLoadSettles',
    'setCursorIndex',
    'refreshIndexSizes',
    'closeVolumeChooser',
    'refreshVolumeSpace',
  ]
  const stub: Record<string, () => unknown> = {}
  for (const n of names) stub[n] = () => undefined
  // The in-place path-nav arm returns `paneRef.navigateToPath(path)`, whose real
  // shape is `Promise<void>`; mirror that so the coordinator's return type is
  // faithful.
  stub.navigateToPath = () => Promise.resolve()
  return stub
}

vi.mock('./FilePane.svelte', () => ({
  // A Svelte-5 component is a function `(anchor, props) => instance`. The stub
  // records props (so a test can invoke the wired render-prop callbacks) and
  // returns a flat no-op instance so `getPaneRef(pane)?.someMethod()` is inert.
  default: function FilePaneMock(_anchor: unknown, props: Record<string, unknown>) {
    captured.props[props.paneId as string] = props
    if (suppressRef.paneId === props.paneId) return null
    return makePaneRefStub()
  },
}))

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

const resolvePathVolumeMock = vi.hoisted(() =>
  vi.fn().mockResolvedValue({
    volume: { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
    timedOut: false,
  }),
)
const getDefaultVolumeIdMock = vi.hoisted(() => vi.fn().mockResolvedValue('root'))

vi.mock('$lib/tauri-commands', async (importOriginal) => {
  const actual = await importOriginal<typeof import('$lib/tauri-commands')>()
  return {
    ...actual,
    pathExists: vi.fn().mockResolvedValue(true),
    listen: vi.fn(() => Promise.resolve(() => {})),
    getRestrictedPaths: vi.fn().mockResolvedValue([]),
    listVolumes: vi.fn().mockResolvedValue({
      data: [
        { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
        { id: 'ext', name: 'Ext', path: '/Volumes/Ext', category: 'attached_volume', isEjectable: true },
      ],
      timedOut: false,
    }),
    getBusyVolumeIds: vi.fn().mockResolvedValue([]),
    resolvePathVolume: resolvePathVolumeMock,
    getDefaultVolumeId: getDefaultVolumeIdMock,
    getVolumeSpace: vi.fn().mockResolvedValue({ data: null, timedOut: false }),
    DEFAULT_VOLUME_ID: 'root',
    getE2eStartPath: vi.fn().mockResolvedValue(null),
    updateFocusedPane: vi.fn().mockResolvedValue(undefined),
    updatePaneTabs: vi.fn().mockResolvedValue(undefined),
    updatePinTabMenu: vi.fn().mockResolvedValue(undefined),
    setReopenClosedTabEnabled: vi.fn().mockResolvedValue(undefined),
    updateViewModeMenu: vi.fn().mockResolvedValue(undefined),
    watchVolumeSpace: vi.fn().mockResolvedValue(undefined),
    ejectVolume: vi.fn().mockResolvedValue(undefined),
    onMtpDeviceConnected: vi.fn().mockResolvedValue(() => {}),
    onMtpDeviceDisconnected: vi.fn().mockResolvedValue(() => {}),
    onVolumeSpaceChanged: vi.fn().mockResolvedValue(() => {}),
    onWriteSourceItemDone: vi.fn().mockResolvedValue(() => {}),
    onDirectoryDiff: vi.fn().mockResolvedValue(() => {}),
    onDirectoryDeleted: vi.fn().mockResolvedValue(() => {}),
    onMtpExclusiveAccessError: vi.fn().mockResolvedValue(() => {}),
    onMtpPermissionError: vi.fn().mockResolvedValue(() => {}),
    listMtpDevices: vi.fn().mockResolvedValue([]),
    listNetworkHosts: vi.fn().mockResolvedValue([]),
    getNetworkDiscoveryState: vi.fn().mockResolvedValue('idle'),
    onVolumeContextAction: vi.fn().mockResolvedValue(() => {}),
    onVolumeUnmounted: vi.fn().mockResolvedValue(() => {}),
    getIpcErrorMessage: (e: unknown) => String(e),
  }
})

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

vi.mock('../navigation/path-resolution', () => ({
  resolveValidPath: vi.fn().mockResolvedValue('/Users/me'),
}))

// Spy on the toast surface so the MAX_TABS_PER_PANE branch's "Tab limit reached"
// warn toast is assertable. FilePane is mocked, so the coordinator is the only
// consumer here (`addToast`).
vi.mock('$lib/ui/toast', () => ({
  addToast: vi.fn(),
  dismissTransientToasts: vi.fn(),
}))

import DualPaneExplorer from './DualPaneExplorer.svelte'
import { explorerState, _resetForTesting } from './explorer-state.svelte'
import { getActiveTab, pushHistoryEntry } from '../tabs/tab-state-manager.svelte'
import type { NavigateResult } from './navigate'

type ExplorerHandle = {
  navigate: (intent: {
    pane: 'left' | 'right'
    to: { goTo: { volumeId: string; path: string } } | { selectVolume: { volumeId: string; path: string } }
    source: 'user' | 'mcp'
  }) => NavigateResult
  openSearchSnapshotInPane: (snapshotId: string, pane?: 'left' | 'right') => void
}

async function mountExplorer(): Promise<ExplorerHandle> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const handle = mount(DualPaneExplorer, { target }) as unknown as ExplorerHandle
  for (let i = 0; i < 20; i++) await tick()
  await new Promise((r) => setTimeout(r, 20))
  await tick()
  return handle
}

function leftProps(): Record<string, unknown> {
  return captured.props.left
}
function leftTab() {
  return getActiveTab(explorerState.getTabMgr('left'))
}
function leftMgr() {
  return explorerState.getTabMgr('left')
}

/** Settle background async work (resolvePathVolume / getDefaultVolumeId) inside a handler. */
async function settle(): Promise<void> {
  for (let i = 0; i < 6; i++) await tick()
  await new Promise((r) => setTimeout(r, 10))
  await tick()
}

beforeEach(() => {
  _resetForTesting()
  captured.props = {}
  suppressRef.paneId = null
  resolvePathVolumeMock.mockResolvedValue({
    volume: { id: 'root', name: 'Macintosh HD', path: '/', category: 'main_volume', isEjectable: false },
    timedOut: false,
  })
  getDefaultVolumeIdMock.mockResolvedValue('root')
})

describe('scenario 3: pinned-tab fork (L7)', () => {
  it('path-change on a pinned tab opens a NEW unpinned tab; the pinned tab is unchanged', async () => {
    await mountExplorer()
    const mgr = leftMgr()
    const pinned = getActiveTab(mgr)
    pinned.pinned = true
    const pinnedId = pinned.id
    const tabCountBefore = mgr.tabs.length

    // Drive the wired onPathChange render prop (what FilePane calls on a successful listing).
    ;(leftProps().onPathChange as (p: string) => void)('/Users/me/docs')
    await tick()

    expect(mgr.tabs.length).toBe(tabCountBefore + 1)
    // The pinned tab kept its path; a new active unpinned tab carries the target.
    const stillPinned = mgr.tabs.find((t) => t.id === pinnedId)
    expect(stillPinned?.path).toBe('/Users/me')
    expect(stillPinned?.pinned).toBe(true)
    const active = getActiveTab(mgr)
    expect(active.id).not.toBe(pinnedId)
    expect(active.pinned).toBe(false)
    expect(active.path).toBe('/Users/me/docs')
  })

  it('volume-change on a pinned tab opens a NEW unpinned tab with the target volume', async () => {
    await mountExplorer()
    const mgr = leftMgr()
    getActiveTab(mgr).pinned = true
    const pinnedId = getActiveTab(mgr).id
    const tabCountBefore = mgr.tabs.length

    ;(leftProps().onVolumeChange as (v: string, vp: string, tp: string) => void)('ext', '/Volumes/Ext', '/Volumes/Ext')
    await settle()

    expect(mgr.tabs.length).toBe(tabCountBefore + 1)
    const active = getActiveTab(mgr)
    expect(active.id).not.toBe(pinnedId)
    expect(active.pinned).toBe(false)
    expect(active.volumeId).toBe('ext')
    expect(active.path).toBe('/Volumes/Ext')
  })

  it('at MAX_TABS_PER_PANE a pinned path-change navigates in-place and toasts "Tab limit reached"', async () => {
    const { addToast } = await import('$lib/ui/toast')
    const addToastSpy = vi.mocked(addToast)
    addToastSpy.mockClear()

    await mountExplorer()
    const mgr = leftMgr()
    // Fill the pane to the cap with pinned-irrelevant tabs; the active one is pinned.
    const template = getActiveTab(mgr)
    template.pinned = true
    while (mgr.tabs.length < 10) {
      mgr.tabs.push({
        id: `filler-${String(mgr.tabs.length)}`,
        path: '/Users/me',
        volumeId: 'root',
        history: { stack: [{ volumeId: 'root', path: '/Users/me' }], currentIndex: 0 },
        sortBy: 'name',
        sortOrder: 'ascending',
        viewMode: 'brief',
        pinned: false,
        cursorFilename: null,
        unreachable: null,
      })
    }
    const activeId = getActiveTab(mgr).id
    await tick()

    ;(leftProps().onPathChange as (p: string) => void)('/Users/me/docs')
    await tick()

    // No new tab; the pinned active tab navigated in-place; the toast fired.
    expect(mgr.tabs.length).toBe(10)
    expect(getActiveTab(mgr).id).toBe(activeId)
    expect(getActiveTab(mgr).path).toBe('/Users/me/docs')
    expect(addToastSpy).toHaveBeenCalledWith('Tab limit reached', { level: 'warn' })
  })
})

describe('scenario 2: a resolved real-path `{ location }` switches a snapshot pane OFF search-results', () => {
  it('navigating a snapshot pane to a real-volume location switches the pane OFF the search-results volume', async () => {
    const handle = await mountExplorer()
    // Put the left pane on the snapshot virtual volume.
    const tab = leftTab()
    tab.volumeId = 'search-results'
    tab.path = 'search-results://sr-1'
    await tick()

    // The search-results row / dialog "Go to file" resolves the entry's Location
    // at the edge, then routes `{ location }`. A different volume → the switch arm.
    const result = handle.navigate({
      pane: 'left',
      to: { goTo: { volumeId: 'root', path: '/Library/Preferences' } },
      source: 'mcp',
    })
    expect(result.status).toBe('started')
    if (result.status === 'started') await result.settled
    await settle()

    // The pane left the snapshot volume; volumeId + path are now a real location.
    expect(leftTab().volumeId).toBe('root')
    expect(leftTab().path).toBe('/Library/Preferences')
  })

  // The former "no-volume-resolved stays put" case moved to the EDGE: when a path
  // can't be resolved, the edge resolver (`resolveLocationOrToast`) shows a toast
  // and never calls `navigate()`, so there is no in-`navigate()` no-op to test
  // here. Covered by navigate-and-select.test / go-to-path.test.
})

describe('scenario 6: MTP fatal fallback', () => {
  it('falls back to the default volume + path and pushes a history entry', async () => {
    await mountExplorer()
    const tab = leftTab()
    // Put the pane on an MTP-ish volume so the fallback is a real switch.
    tab.volumeId = 'mtp-dev:1'
    tab.path = 'mtp://dev/1/DCIM'
    await tick()
    getDefaultVolumeIdMock.mockResolvedValue('root')
    const depthBefore = leftTab().history.stack.length

    ;(leftProps().onMtpFatalError as (m: string) => void)('Device disconnected')
    await settle()

    // Committed default volume + its path; a history entry was pushed.
    expect(leftTab().volumeId).toBe('root')
    expect(leftTab().path).toBe('/') // root volume path from the mocked volume list
    expect(leftTab().history.stack.length).toBe(depthBefore + 1)
  })
})

describe('scenario 4: unreachable fallback', () => {
  it('retry clears tab.unreachable, commits the original path, and pushes history', async () => {
    await mountExplorer()
    const tab = leftTab()
    tab.unreachable = { originalPath: '/Volumes/Ext/photos', retrying: false }
    await tick()
    resolvePathVolumeMock.mockResolvedValue({
      volume: { id: 'ext', name: 'Ext', path: '/Volumes/Ext', category: 'attached_volume', isEjectable: true },
      timedOut: false,
    })
    const depthBefore = leftTab().history.stack.length

    ;(leftProps().onRetryUnreachable as () => void)()
    await settle()

    expect(leftTab().unreachable).toBeNull()
    expect(leftTab().volumeId).toBe('ext')
    expect(leftTab().path).toBe('/Volumes/Ext/photos')
    expect(leftTab().history.stack.length).toBe(depthBefore + 1)
  })

  it('open-home clears unreachable and goes to ~ on the default volume', async () => {
    await mountExplorer()
    const tab = leftTab()
    tab.unreachable = { originalPath: '/Volumes/Ext/photos', retrying: false }
    await tick()
    getDefaultVolumeIdMock.mockResolvedValue('root')
    const depthBefore = leftTab().history.stack.length

    ;(leftProps().onOpenHome as () => void)()
    await settle()

    expect(leftTab().unreachable).toBeNull()
    expect(leftTab().volumeId).toBe('root')
    expect(leftTab().path).toBe('~')
    expect(leftTab().history.stack.length).toBe(depthBefore + 1)
  })
})

describe('scenario 5: cancel-during-load', () => {
  it('history-back branch: when the cancelled path completed, go back one history entry', async () => {
    await mountExplorer()
    const tab = leftTab()
    // History: /Users/me -> /Users/me/deep (current). The cancelled path equals
    // the current entry, so the handler goes back.
    tab.history = pushHistoryEntry(
      { stack: [{ volumeId: 'root', path: '/Users/me' }], currentIndex: 0 },
      { volumeId: 'root', path: '/Users/me/deep' },
    )
    tab.path = '/Users/me/deep'
    await tick()

    ;(leftProps().onCancelLoading as (p: string, s?: string) => void)('/Users/me/deep')
    await settle()

    // Went back to /Users/me.
    expect(leftTab().path).toBe('/Users/me')
  })

  it('network-restore branch: restores the network entry without leaving the network volume', async () => {
    await mountExplorer()
    const tab = leftTab()
    tab.volumeId = 'network'
    tab.path = 'smb://server/share'
    tab.history = { stack: [{ volumeId: 'network', path: 'smb://server/share' }], currentIndex: 0 }
    await tick()

    ;(leftProps().onCancelLoading as (p: string, s?: string) => void)('smb://server/share/sub')
    await settle()

    expect(leftTab().volumeId).toBe('network')
    expect(leftTab().path).toBe('smb://server/share')
  })

  it('walk-up branch: no history at the cancelled path resolves to the nearest valid parent', async () => {
    const { resolveValidPath } = await import('../navigation/path-resolution')
    vi.mocked(resolveValidPath).mockResolvedValue('/Users')

    await mountExplorer()
    const tab = leftTab()
    // History has a single entry equal to the cancelled path, so canGoBack is false
    // → walk up to the nearest valid parent.
    tab.history = { stack: [{ volumeId: 'root', path: '/Users/me/deep' }], currentIndex: 0 }
    tab.path = '/Users/me/deep'
    await tick()

    ;(leftProps().onCancelLoading as (p: string, s?: string) => void)('/Users/me/deep')
    await settle()

    expect(leftTab().path).toBe('/Users')
  })
})

describe('scenario 7: refusal strings (L12) — byte-for-byte contract', () => {
  it('network-volume pane returns the exact select_volume refusal string', async () => {
    const handle = await mountExplorer()
    const tab = leftTab()
    tab.volumeId = 'network'
    tab.path = 'smb://'
    await tick()

    // Same-volume `{ location }` → the in-place arm, where the refusal fires.
    const result = handle.navigate({
      pane: 'left',
      to: { goTo: { volumeId: 'network', path: '/Users/me/doc' } },
      source: 'mcp',
    })
    expect(result.status).toBe('refused')
    if (result.status === 'refused') {
      expect(result.reason.message).toBe(
        'Pane is on the Network volume. Use select_volume to switch to a local volume first.',
      )
    }
  })

  it('MTP path mismatch returns the exact "not on this MTP volume" string (note the em dash)', async () => {
    const handle = await mountExplorer()
    const tab = leftTab()
    // Pane on a local volume, but the requested path is an mtp:// URL → mismatch.
    tab.volumeId = 'root'
    tab.path = '/Users/me'
    await tick()

    const result = handle.navigate({
      pane: 'left',
      to: { goTo: { volumeId: 'root', path: 'mtp://otherdev/2/DCIM' } },
      source: 'mcp',
    })
    expect(result.status).toBe('refused')
    if (result.status === 'refused') {
      expect(result.reason.message).toBe('Pane is not on this MTP volume — call select_volume first.')
    }
  })

  it('on-MTP-volume pane returns the exact "on the … MTP volume" string', async () => {
    const handle = await mountExplorer()
    const tab = leftTab()
    // volumeId shaped like an MTP storage volume (`mtp-<dev>:<storage>`).
    tab.volumeId = 'mtp-dev:1'
    tab.path = 'mtp://dev/1/DCIM'
    await tick()

    const result = handle.navigate({
      pane: 'left',
      to: { goTo: { volumeId: 'mtp-dev:1', path: '/Users/me/doc' } },
      source: 'mcp',
    })
    // volumeName is undefined for this id (not in the volume list) → falls back to the id.
    expect(result.status).toBe('refused')
    if (result.status === 'refused') {
      expect(result.reason.message).toBe(
        'Pane is on the mtp-dev:1 MTP volume. Use select_volume to switch to a local volume first.',
      )
    }
  })

  it('pane-unavailable returns the exact "Pane not available" string', async () => {
    // Suppress the left pane's FilePane instance so `getPaneRef('left')` is
    // undefined: the local-volume in-place arm then hits the `if (!paneRef)` guard.
    suppressRef.paneId = 'left'
    const handle = await mountExplorer()
    const tab = leftTab()
    tab.volumeId = 'root'
    tab.path = '/Users/me'
    await tick()

    const result = handle.navigate({
      pane: 'left',
      to: { goTo: { volumeId: 'root', path: '/Users/me/doc' } },
      source: 'mcp',
    })
    expect(result.status).toBe('refused')
    if (result.status === 'refused') {
      expect(result.reason.message).toBe('Pane not available')
    }
  })
})
