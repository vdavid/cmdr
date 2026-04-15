import { loadAppStatus, loadPaneTabs } from '$lib/app-status-store'
import { loadSettings } from '$lib/settings-store'
import { pathExists, getDefaultVolumeId, resolvePathVolume, getE2eStartPath } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
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
  showHiddenFiles: boolean
  leftPaneWidthPercent: number
}

/**
 * Loads persisted state and resolves volumes for all tabs.
 * Returns fully initialized tab managers and app state, ready to use.
 */
export async function loadPersistedState(): Promise<InitializedState> {
  // Load persisted state (tabs + app status + settings) in parallel
  const [leftPaneTabs, rightPaneTabs, status, settings] = await Promise.all([
    loadPaneTabs('left', pathExists),
    loadPaneTabs('right', pathExists),
    loadAppStatus(pathExists),
    loadSettings(),
  ])

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

  // Resolve volume IDs for all tabs in parallel, tracking timeouts
  const resolvedLeftTabs = await Promise.all(
    leftPaneTabs.tabs.map(async (tab) => {
      const resolution = await resolveVolumeId(tab.volumeId, tab.path, !!e2eStartPath)
      return {
        ...tab,
        volumeId: resolution.volumeId,
        unreachablePath: resolution.timedOut ? tab.path : null,
      }
    }),
  )
  const resolvedRightTabs = await Promise.all(
    rightPaneTabs.tabs.map(async (tab) => {
      const resolution = await resolveVolumeId(tab.volumeId, tab.path, !!e2eStartPath)
      return {
        ...tab,
        volumeId: resolution.volumeId,
        unreachablePath: resolution.timedOut ? tab.path : null,
      }
    }),
  )

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

  // E2E override: apply fixture paths to the active tab data BEFORE creating tab managers,
  // so the managers are initialized with the correct paths from the start.
  // Must override both path AND volumeId — persisted state may have a non-root volume
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
    showHiddenFiles: settings.showHiddenFiles,
    leftPaneWidthPercent: status.leftPaneWidthPercent,
  }
}
