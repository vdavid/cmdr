import {
  loadAppStatus,
  loadPaneTabs,
  hasPersistedPaneState,
  resolvePersistedPath,
  saveAppStatusNow,
  savePaneTabs,
} from '$lib/app-status-store'
import { hydrateRail } from '$lib/ask-cmdr/ask-cmdr-trigger.svelte'
import { isE2eRun } from '$lib/app-mode'
import {
  pathExists,
  getDefaultVolumeId,
  resolvePathVolume,
  getE2eStartPath,
  checkFullDiskAccessQuiet,
  listSavedServers,
} from '$lib/tauri-commands'
import { isAtOrUnder } from '$lib/servers/server-path-utils'
import { volumeScheme } from '$lib/volume-scheme'
import { getAppLogger } from '$lib/logging/logger'
import { applyFirstRunLayout, resolveFirstRunLayout } from './first-run-layout'
import { createTabManagerFromPersisted } from './tab-operations'
import { getAllTabs, type TabManager } from '../tabs/tab-state-manager.svelte'
import type { PersistedTab, PersistedPaneTabs } from '../tabs/tab-types'

const log = getAppLogger('fileExplorer')

interface VolumeResolution {
  volumeId: string
  timedOut: boolean
}

export interface InitializedState {
  leftTabMgr: TabManager
  rightTabMgr: TabManager
  focusedPane: 'left' | 'right'
  leftPaneWidthPercent: number
}

/**
 * Loads persisted state and resolves volumes for all tabs.
 * Returns fully initialized tab managers and app state, ready to use.
 */
export async function loadPersistedState(): Promise<InitializedState> {
  // Load persisted state (tabs + app status) in parallel
  const [leftPaneTabs, rightPaneTabs, status] = await Promise.all([
    loadPaneTabs('left', pathExists),
    loadPaneTabs('right', pathExists),
    loadAppStatus(pathExists),
  ])

  // Restore the Ask Cmdr rail's persisted open/width (reopening loads its active thread).
  hydrateRail(status.askCmdrRailOpen, status.askCmdrRailWidth)

  // On a first run with Full Disk Access, open home on the left and Downloads on the
  // right. Once per install, and never over a layout somebody already has: the rule and
  // its guardrails live in `first-run-layout.ts`. Runs before the E2E override below so
  // a fixture path still wins.
  const firstRun = await resolveFirstRunLayout({
    isAutomatedRun: isE2eRun,
    layoutAlreadyApplied: status.firstRunLayoutApplied,
    hasPersistedPaneState,
    hasFullDiskAccess: checkFullDiskAccessQuiet,
    pathExists,
  })
  if (firstRun.kind === 'openHomeAndDownloads') {
    log.info('First run: opening {leftPath} and {rightPath}', {
      leftPath: firstRun.leftPath,
      rightPath: firstRun.rightPath,
    })
    applyFirstRunLayout(leftPaneTabs, rightPaneTabs, firstRun)
  }

  // E2E test override: use CMDR_E2E_START_PATH subdirectories when set
  const e2eStartPath = await getE2eStartPath()

  // Determine the correct volume IDs by finding which volume contains each tab's path
  // This is more reliable than trusting the stored volumeId, which may be stale
  // Exception: 'network' is a virtual volume, trust the stored ID for that
  const defaultId = await getDefaultVolumeId()

  async function resolveVolumeId(volumeId: string, path: string, hasE2eOverride: boolean): Promise<VolumeResolution> {
    if (volumeId === 'network' && !hasE2eOverride) return { volumeId: 'network', timedOut: false }
    const result = await resolvePathVolume(path)
    if (result.volume) return { volumeId: result.volume.id, timedOut: false }
    if (result.timedOut) {
      log.warn('Volume resolution timed out for path: {path}', { path })
      return { volumeId: defaultId, timedOut: true }
    }
    // Path doesn't exist, but volume is reachable
    return { volumeId: defaultId, timedOut: false }
  }

  const shareRoots = await unmountedShareRoots([...leftPaneTabs.tabs, ...rightPaneTabs.tabs])

  async function restoreTab(tab: PersistedTab) {
    const share = await restoreShareTab(tab, shareRoots)
    if (share.kept) return { ...tab, unreachablePath: null }
    const resolution = await resolveVolumeId(tab.volumeId, share.path, !!e2eStartPath)
    return {
      ...tab,
      path: share.path,
      volumeId: resolution.volumeId,
      unreachablePath: resolution.timedOut ? share.path : null,
    }
  }

  // Resolve volume IDs for all tabs in parallel, tracking timeouts
  const resolvedLeftTabs = await Promise.all(leftPaneTabs.tabs.map(restoreTab))
  const resolvedRightTabs = await Promise.all(rightPaneTabs.tabs.map(restoreTab))

  // Collect unreachable paths by tab ID before stripping extra fields
  const unreachableByTabId: Record<string, string> = {}
  for (const tab of [...resolvedLeftTabs, ...resolvedRightTabs]) {
    if (tab.unreachablePath) {
      unreachableByTabId[tab.id] = tab.unreachablePath
    }
  }

  const toPersistedTab = (tab: (typeof resolvedLeftTabs)[number]): PersistedTab => ({
    id: tab.id,
    path: tab.path,
    volumeId: tab.volumeId,
    sortBy: tab.sortBy,
    sortOrder: tab.sortOrder,
    viewMode: tab.viewMode,
    pinned: tab.pinned,
  })
  const resolvedLeftPaneTabs: PersistedPaneTabs = {
    tabs: resolvedLeftTabs.map(toPersistedTab),
    activeTabId: leftPaneTabs.activeTabId,
  }
  const resolvedRightPaneTabs: PersistedPaneTabs = {
    tabs: resolvedRightTabs.map(toPersistedTab),
    activeTabId: rightPaneTabs.activeTabId,
  }

  // Everything the first-run rule decided gets written HERE, once, in this order, and
  // BEFORE the E2E override below can rewrite these paths to fixture ones.
  //
  // The nav-state subscriber can't cover the layout: it seeds its baseline from the loaded
  // state without saving, and an applied layout IS that state, so it would live in memory
  // for one session and then vanish, with the marker guaranteeing it never comes back.
  // Tabs before the marker, because a quit in between only lets the rule run again next
  // launch, while the reverse would lose the layout for good. Awaited rather than
  // fire-and-forget for the same reason `saveAppStatusNow` skips the debounce: startup is
  // followed by plenty of things that can quit the app.
  if (firstRun.kind === 'openHomeAndDownloads') {
    await Promise.all([savePaneTabs('left', resolvedLeftPaneTabs), savePaneTabs('right', resolvedRightPaneTabs)])
    await saveAppStatusNow({
      leftPath: firstRun.leftPath,
      rightPath: firstRun.rightPath,
      firstRunLayoutApplied: true,
    })
  } else if (firstRun.kind === 'markAlreadyLaidOut') {
    await saveAppStatusNow({ firstRunLayoutApplied: true })
  }

  // E2E override: apply fixture paths to the active tab data BEFORE creating tab managers,
  // so the managers are initialized with the correct paths from the start.
  // Must override both path AND volumeId. Persisted state may have a non-root volume
  // (e.g. VirtioFS mount) whose path resolver would mangle the absolute fixture path.
  if (e2eStartPath) {
    const leftActiveTab = resolvedLeftPaneTabs.tabs.find((t) => t.id === resolvedLeftPaneTabs.activeTabId)
    const rightActiveTab = resolvedRightPaneTabs.tabs.find((t) => t.id === resolvedRightPaneTabs.activeTabId)
    const leftTarget = leftActiveTab ?? resolvedLeftPaneTabs.tabs[0]
    const rightTarget = rightActiveTab ?? resolvedRightPaneTabs.tabs[0]
    if (!leftActiveTab) log.warn('E2E path override: left active tab ID mismatch, using first tab')
    if (!rightActiveTab) log.warn('E2E path override: right active tab ID mismatch, using first tab')
    leftTarget.path = `${e2eStartPath}/left`
    leftTarget.volumeId = defaultId
    rightTarget.path = `${e2eStartPath}/right`
    rightTarget.volumeId = defaultId
  }

  // Create tab managers from persisted tab data
  const leftTabMgr = createTabManagerFromPersisted(resolvedLeftPaneTabs)
  const rightTabMgr = createTabManagerFromPersisted(resolvedRightPaneTabs)

  // Apply unreachable state to tabs that timed out during volume resolution
  for (const tab of [...getAllTabs(leftTabMgr), ...getAllTabs(rightTabMgr)]) {
    const originalPath = unreachableByTabId[tab.id]
    if (originalPath) {
      tab.unreachable = { originalPath, retrying: false }
    }
  }

  return {
    leftTabMgr,
    rightTabMgr,
    focusedPane: status.focusedPane,
    leftPaneWidthPercent: status.leftPaneWidthPercent,
  }
}

/**
 * Where each SMB share a restored tab stands on last mounted, for the shares that are
 * saved places and aren't mounted now. Asks for the saved list only when some tab is
 * on a share: it's cached state, but a launch with no share tab has no use for it.
 */
async function unmountedShareRoots(tabs: PersistedTab[]): Promise<Map<string, string>> {
  const roots = new Map<string, string>()
  if (!tabs.some((tab) => volumeScheme(tab.volumeId) === 'smb')) return roots
  try {
    for (const server of await listSavedServers()) {
      for (const place of server.places) {
        // A share no mount went through has an `smb://` root and no folder to land in.
        if (volumeScheme(place.volumeId) === 'smb' && !place.connected && place.appRoot.startsWith('/')) {
          roots.set(place.volumeId, place.appRoot)
        }
      }
    }
  } catch (error) {
    log.warn('Could not read the saved servers, so share tabs walk up as plain folders: {error}', { error })
  }
  return roots
}

/**
 * A restored tab on an SMB share, which `loadPaneTabs` hands back unprobed.
 *
 * ❗ On an unmounted SAVED share it keeps its id and the folder it stood on (`kept`):
 * the pane's `place-connect` dials the share's `saved` row and enters that folder, or
 * the nearest one that still exists. Probing it here would walk to `/Volumes`, and the
 * volume lookup would then file the tab under the boot disk. Any other share's tab
 * (mounted, or nobody saved it) walks up like a plain folder.
 */
async function restoreShareTab(
  tab: PersistedTab,
  shareRoots: Map<string, string>,
): Promise<{ kept: boolean; path: string }> {
  if (volumeScheme(tab.volumeId) !== 'smb') return { kept: false, path: tab.path }
  const root = shareRoots.get(tab.volumeId)
  if (root && isAtOrUnder(tab.path, root)) return { kept: true, path: tab.path }
  return { kept: false, path: await resolvePersistedPath(tab.path, pathExists) }
}
