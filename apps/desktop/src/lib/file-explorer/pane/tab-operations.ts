import { confirmDialog } from '$lib/utils/confirm-dialog'
import { showTabContextMenu, onTabContextAction, updatePinTabMenu } from '$lib/tauri-commands'
import { savePaneTabs, type ViewMode } from '$lib/app-status-store'
import {
  createTabManager,
  getActiveTab,
  addTab,
  closeTabRecording,
  closeOtherTabsRecording,
  reopenLastClosedTab as reopenLastClosedTabInMgr,
  switchTab,
  cycleTab as cycleTabInManager,
  getAllTabs,
  getTabCount,
  pinTab,
  unpinTab,
  MAX_TABS_PER_PANE,
  retainSnapshotRefs,
  moveTab,
  type MoveTabRefusal,
  type MoveTabResult,
  type TabManager,
} from '../tabs/tab-state-manager.svelte'
import {
  reportTabClosed,
  reportTabMoved,
  reportTabOpened,
  reportTabPinToggled,
  reportTabSwitched,
  type TabMoveOutcome,
} from '../tabs/tab-analytics'
import type { TabState, TabId, PersistedTab, PersistedPaneTabs } from '../tabs/tab-types'
import { createHistory } from '../navigation/navigation-history'
import { isSnapshotPath, latestRealFolder } from '../navigation/real-folder-history'
import { DEFAULT_SORT_BY, defaultSortOrders, type SortColumn } from '../types'
import type { Location } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import type { FilePaneAPI } from './types'

const log = getAppLogger('fileExplorer')

// --- Tab initialization helpers ---

export function createInitialTabState(
  path: string,
  volumeId: string,
  sortBy: SortColumn = DEFAULT_SORT_BY,
  viewMode: ViewMode = 'full',
): TabState {
  return {
    id: crypto.randomUUID(),
    path,
    volumeId,
    history: createHistory(volumeId, path),
    sortBy,
    sortOrder: defaultSortOrders[sortBy],
    viewMode,
    pinned: false,
    cursorFilename: null,
    unreachable: null,
  }
}

export function createTabManagerFromPersisted(paneTabs: PersistedPaneTabs): TabManager {
  const tabs = paneTabs.tabs.map((pt): TabState => ({
    ...pt,
    history: createHistory(pt.volumeId, pt.path),
    cursorFilename: null,
    unreachable: null,
  }))

  const mgr = createTabManager(tabs[0])
  for (let i = 1; i < tabs.length; i++) {
    mgr.tabs.push(tabs[i])
  }
  mgr.activeTabId = paneTabs.activeTabId
  return mgr
}

/**
 * Where a tab should come back next launch. Its own location, unless that's a
 * `search-results://` snapshot: a snapshot id names an in-memory, per-session result set,
 * so a stored one names nothing by the time it's read back and leaves the pane with no
 * folder to list. The newest real folder in the tab's history stands in for it, carrying
 * that entry's volume (the snapshot pane's own is the virtual `search-results`).
 *
 * The tab's live `path` is untouched: navigating Back into a snapshot within the session
 * is what the in-memory store is for. A tab whose history holds no real folder persists
 * as it stands, and `$lib/app-status-store.ts` rescues it at load.
 */
function persistedLocation(tab: TabState): Location {
  if (!isSnapshotPath(tab.path)) return { path: tab.path, volumeId: tab.volumeId }
  // Only where the tab has BEEN, never its forward history: a pane that reached this
  // snapshot by walking Back still has later folders ahead of it, and coming back up in
  // one the user had navigated away from would be a place they never chose to leave open.
  const visited = tab.history.stack.slice(0, tab.history.currentIndex + 1)
  const entry = latestRealFolder(visited, (e) => e.path)
  return entry === null ? { path: tab.path, volumeId: tab.volumeId } : { path: entry.path, volumeId: entry.volumeId }
}

export function buildPersistedPaneTabs(mgr: TabManager): PersistedPaneTabs {
  return {
    tabs: getAllTabs(mgr).map((tab): PersistedTab => ({
      id: tab.id,
      ...persistedLocation(tab),
      sortBy: tab.sortBy,
      sortOrder: tab.sortOrder,
      viewMode: tab.viewMode,
      pinned: tab.pinned,
    })),
    activeTabId: mgr.activeTabId,
  }
}

export function saveTabsForPane(pane: 'left' | 'right', getTabMgr: (pane: 'left' | 'right') => TabManager) {
  void savePaneTabs(pane, buildPersistedPaneTabs(getTabMgr(pane)))
}

// --- Tab bar handlers ---

export async function handleTabClose(
  pane: 'left' | 'right',
  tabId: TabId,
  getTabMgr: (pane: 'left' | 'right') => TabManager,
  focusedPane: 'left' | 'right',
  syncPinTabMenu: () => void,
  getClosedTabsCap: () => number,
) {
  const mgr = getTabMgr(pane)
  const tab = getAllTabs(mgr).find((t) => t.id === tabId)
  const wasPinned = tab?.pinned ?? false
  if (tab?.pinned) {
    const ok = await confirmDialog(
      tString('fileExplorer.tabs.closePinnedConfirm'),
      tString('fileExplorer.tabs.closePinnedTitle'),
    )
    if (!ok) {
      reportTabClosed('single', 'cancelled', getTabCount(mgr), wasPinned)
      return
    }
  }
  const result = closeTabRecording(mgr, tabId, getClosedTabsCap())
  reportTabClosed('single', result.closed ? 'closed' : 'lastTab', getTabCount(mgr), wasPinned)
  saveTabsForPane(pane, getTabMgr)
  if (pane === focusedPane) syncPinTabMenu()
}

export function handleTabMiddleClick(
  pane: 'left' | 'right',
  tabId: TabId,
  getTabMgr: (pane: 'left' | 'right') => TabManager,
  focusedPane: 'left' | 'right',
  syncPinTabMenu: () => void,
  getClosedTabsCap: () => number,
) {
  const mgr = getTabMgr(pane)
  const tab = getAllTabs(mgr).find((t) => t.id === tabId)
  if (!tab) return
  if (tab.pinned) {
    unpinTab(mgr, tabId)
  }
  void handleTabClose(pane, tabId, getTabMgr, focusedPane, syncPinTabMenu, getClosedTabsCap)
}

export async function handleTabContextMenu(
  pane: 'left' | 'right',
  tabId: TabId,
  event: MouseEvent,
  getTabMgr: (pane: 'left' | 'right') => TabManager,
  focusedPane: 'left' | 'right',
  syncPinTabMenu: () => void,
  getClosedTabsCap: () => number,
) {
  event.preventDefault()

  const mgr = getTabMgr(pane)
  const tab = getAllTabs(mgr).find((t) => t.id === tabId)
  if (!tab) return

  const canClose = getTabCount(mgr) > 1
  const hasOtherUnpinnedTabs = getAllTabs(mgr).some((t) => t.id !== tabId && !t.pinned)

  // Listen for the action event BEFORE showing the popup. The event fires
  // asynchronously after popup() returns (muda queues MenuEvent through the
  // event loop, so a synchronous channel always times out).
  const actionPromise = new Promise<string | null>((resolve) => {
    let resolved = false
    let unlisten: (() => void) | undefined

    void onTabContextAction((action: string) => {
      if (!resolved) {
        resolved = true
        unlisten?.()
        resolve(action)
      }
    }).then((fn) => {
      unlisten = fn
      // If already resolved (dismissed before listener registered), clean up
      if (resolved) fn()
    })

    // After showing the popup, set a timeout for dismissed-without-selection.
    // popup() blocks in Rust until the menu closes, so this runs after dismissal.
    void showTabContextMenu(tab.pinned, canClose, hasOtherUnpinnedTabs).then(() => {
      // Give the event loop time to deliver the action event
      setTimeout(() => {
        if (!resolved) {
          resolved = true
          unlisten?.()
          resolve(null)
        }
      }, 500)
    })
  })

  const action = await actionPromise

  // Re-fetch tab state after the context menu (state may have changed during the await)
  const currentTab = getAllTabs(mgr).find((t) => t.id === tabId)
  if (!currentTab) return

  switch (action) {
    case 'tab_pin':
      if (currentTab.pinned) {
        unpinTab(mgr, tabId)
      } else {
        pinTab(mgr, tabId)
      }
      reportTabPinToggled(!currentTab.pinned)
      saveTabsForPane(pane, getTabMgr)
      if (pane === focusedPane && tabId === mgr.activeTabId) syncPinTabMenu()
      break
    case 'tab_close_others':
      closeOtherTabsRecording(mgr, tabId, getClosedTabsCap())
      reportTabClosed('others', 'closed', getTabCount(mgr), false)
      saveTabsForPane(pane, getTabMgr)
      break
    case 'tab_close': {
      void handleTabClose(pane, tabId, getTabMgr, focusedPane, syncPinTabMenu, getClosedTabsCap)
      break
    }
  }
}

/**
 * Creates a new tab in the focused pane via the clone trick:
 * inserts a clone to the left and keeps the current tab active.
 * Returns false if at the tab cap.
 */
export function newTab(
  focusedPane: 'left' | 'right',
  getTabMgr: (pane: 'left' | 'right') => TabManager,
  snapshotHistory: (history: TabState['history']) => TabState['history'],
): boolean {
  const mgr = getTabMgr(focusedPane)
  const activeTab = getActiveTab(mgr)
  const wasPinned = activeTab.pinned

  // Clone trick: insert clone to the LEFT, keep active tab selected.
  // If the active tab is pinned, the clone inherits the pin (it stays
  // in the pinned tab's position) and the active tab gets unpinned
  // (it becomes the new "branched off" tab to the right).
  const cloneTab: TabState = {
    id: crypto.randomUUID(),
    path: activeTab.path,
    volumeId: activeTab.volumeId,
    history: snapshotHistory(activeTab.history),
    sortBy: activeTab.sortBy,
    sortOrder: activeTab.sortOrder,
    viewMode: activeTab.viewMode,
    pinned: wasPinned,
    cursorFilename: null,
    unreachable: null,
  }

  const success = addTab(mgr, activeTab.id, cloneTab)
  if (success) {
    // The clone carries a COPY of the history, so every `search-results://` entry in it
    // now has a second holder and needs a second ref.
    retainSnapshotRefs(cloneTab.history)
  }
  if (success && wasPinned) {
    unpinTab(mgr, activeTab.id)
  }
  if (success) {
    saveTabsForPane(focusedPane, getTabMgr)
  }
  reportTabOpened('new', success ? 'opened' : 'atCap', getTabCount(mgr))
  return success
}

/** Closes the active tab with pinned confirmation if needed. */
export async function closeActiveTabWithConfirmation(
  focusedPane: 'left' | 'right',
  getTabMgr: (pane: 'left' | 'right') => TabManager,
  getClosedTabsCap: () => number,
): Promise<'closed' | 'last-tab' | 'cancelled'> {
  const mgr = getTabMgr(focusedPane)
  const activeTab = getActiveTab(mgr)

  // Last tab: close window without confirmation (even if pinned)
  if (getTabCount(mgr) <= 1) {
    reportTabClosed('single', 'lastTab', getTabCount(mgr), activeTab.pinned)
    return 'last-tab'
  }

  // Pinned tab: confirm before closing
  if (activeTab.pinned) {
    const ok = await confirmDialog(
      tString('fileExplorer.tabs.closePinnedConfirm'),
      tString('fileExplorer.tabs.closePinnedTitle'),
    )
    if (!ok) {
      reportTabClosed('single', 'cancelled', getTabCount(mgr), activeTab.pinned)
      return 'cancelled'
    }
  }

  const result = closeTabRecording(mgr, mgr.activeTabId, getClosedTabsCap())
  reportTabClosed('single', result.closed ? 'closed' : 'lastTab', getTabCount(mgr), activeTab.pinned)
  if (result.closed) {
    saveTabsForPane(focusedPane, getTabMgr)
    return 'closed'
  }
  return 'last-tab'
}

/** Closes all other tabs (except the active one) in the focused pane. */
export function closeOtherTabsInFocusedPane(
  focusedPane: 'left' | 'right',
  getTabMgr: (pane: 'left' | 'right') => TabManager,
  getClosedTabsCap: () => number,
) {
  const mgr = getTabMgr(focusedPane)
  closeOtherTabsRecording(mgr, mgr.activeTabId, getClosedTabsCap())
  reportTabClosed('others', 'closed', getTabCount(mgr), false)
  saveTabsForPane(focusedPane, getTabMgr)
}

/** Reopens the most-recently-closed tab in the focused pane. */
export function reopenLastClosedTabInPane(
  focusedPane: 'left' | 'right',
  getTabMgr: (pane: 'left' | 'right') => TabManager,
): 'reopened' | 'empty' | 'cap' {
  const mgr = getTabMgr(focusedPane)
  const result = reopenLastClosedTabInMgr(mgr, MAX_TABS_PER_PANE)
  if ('reopened' in result) {
    saveTabsForPane(focusedPane, getTabMgr)
    reportTabOpened('reopened', 'opened', getTabCount(mgr))
    return 'reopened'
  }
  reportTabOpened('reopened', result.reason === 'cap' ? 'atCap' : 'nothingToReopen', getTabCount(mgr))
  return result.reason
}

/** Toggles pin state on the active tab in the focused pane. */
export function togglePinActiveTab(focusedPane: 'left' | 'right', getTabMgr: (pane: 'left' | 'right') => TabManager) {
  const mgr = getTabMgr(focusedPane)
  const activeTab = getActiveTab(mgr)
  if (activeTab.pinned) {
    unpinTab(mgr, activeTab.id)
  } else {
    pinTab(mgr, activeTab.id)
  }
  reportTabPinToggled(activeTab.pinned)
  saveTabsForPane(focusedPane, getTabMgr)
  syncPinTabMenuForPane(focusedPane, getTabMgr)
}

/** Syncs the File menu "Pin tab" / "Unpin tab" label with the active tab's state. */
export function syncPinTabMenuForPane(
  focusedPane: 'left' | 'right',
  getTabMgr: (pane: 'left' | 'right') => TabManager,
) {
  const mgr = getTabMgr(focusedPane)
  const activeTab = getActiveTab(mgr)
  void updatePinTabMenu(activeTab.pinned)
}

/** Cycle to next/prev tab in the focused pane. */
export function cycleTab(
  direction: 'next' | 'prev',
  focusedPane: 'left' | 'right',
  getTabMgr: (pane: 'left' | 'right') => TabManager,
  getPaneRef: (pane: 'left' | 'right') => FilePaneAPI | undefined,
) {
  const mgr = getTabMgr(focusedPane)
  const paneRef = getPaneRef(focusedPane)
  const cursorFilename = paneRef?.getFilenameUnderCursor() ?? null
  cycleTabInManager(mgr, direction, cursorFilename)
  reportTabSwitched('cycle')
  saveTabsForPane(focusedPane, getTabMgr)
  syncPinTabMenuForPane(focusedPane, getTabMgr)
}

/** Switch to a specific tab by ID in the given pane. Returns false if tab not found. */
export function switchToTab(
  pane: 'left' | 'right',
  tabId: TabId,
  getTabMgr: (pane: 'left' | 'right') => TabManager,
  getPaneRef: (pane: 'left' | 'right') => FilePaneAPI | undefined,
  focusedPane: 'left' | 'right',
) {
  const mgr = getTabMgr(pane)
  const paneRef = getPaneRef(pane)
  const cursorFilename = paneRef?.getFilenameUnderCursor() ?? null
  const switched = switchTab(mgr, tabId, cursorFilename)
  if (!switched) {
    log.warn(`MCP tab activate: tab ${tabId} not found in ${pane} pane`)
    return false
  }
  reportTabSwitched('pick')
  saveTabsForPane(pane, getTabMgr)
  if (pane === focusedPane) syncPinTabMenuForPane(focusedPane, getTabMgr)
  return true
}

/** Handle new tab creation for a specific pane's "+" button. */
export function handleNewTab(
  pane: 'left' | 'right',
  focusedPane: 'left' | 'right',
  setFocusedPane: (pane: 'left' | 'right') => void,
  newTabFn: () => boolean,
) {
  // Temporarily focus this pane so newTab() creates in the right pane
  setFocusedPane(pane)
  const success = newTabFn()
  if (!success) {
    addToast(tString('fileExplorer.tabs.limitReached'), { level: 'warn' })
  }
  setFocusedPane(focusedPane === pane ? pane : focusedPane)
}

// --- Moving a tab (drag to reorder, MCP `tab move`) ---

/** One tab move: a reorder when `toPane` is `fromPane`, a move to the other pane otherwise. */
export interface TabMoveRequest {
  fromPane: 'left' | 'right'
  tabId: TabId
  toPane: 'left' | 'right'
  /** Index the tab holds once moved. Omitted: the end. */
  toIndex?: number
}

export interface TabMoveDeps {
  getTabMgr: (pane: 'left' | 'right') => TabManager
  getPaneRef: (pane: 'left' | 'right') => FilePaneAPI | undefined
  getFocusedPane: () => 'left' | 'right'
}

/** The refusals analytics counts. `notFound` is a race (the tab closed mid-gesture), so it has no entry. */
const moveRefusalOutcomes: Record<Exclude<MoveTabRefusal, 'notFound'>, TabMoveOutcome> = {
  pinned: 'pinned',
  onlyTab: 'onlyTab',
  targetFull: 'atCap',
}

/**
 * Moves a tab and does everything a move owes the rest of the app: persists the pane(s)
 * it touched, reports it, and re-syncs the Pin tab menu if the focused pane's active tab
 * changed. The rules live in `moveTab`; the mouse (`handleTabDrop`) and the MCP `tab`
 * tool both come through here. Pane focus is never touched.
 */
export function moveTabToPane(request: TabMoveRequest, deps: TabMoveDeps): MoveTabResult {
  const { fromPane, tabId, toPane, toIndex } = request
  const source = deps.getTabMgr(fromPane)
  const target = deps.getTabMgr(toPane)
  const crossPane = fromPane !== toPane

  // An active tab that leaves its pane takes its cursor along, read while its FilePane
  // is still mounted, the way a tab switch saves it on the tab being left.
  const leavingCursor =
    crossPane && source.activeTabId === tabId
      ? (deps.getPaneRef(fromPane)?.getFilenameUnderCursor() ?? null)
      : undefined

  const result = moveTab(source, target, tabId, toIndex)
  const scope = crossPane ? 'otherPane' : 'samePane'
  if (!result.moved) {
    if (result.reason !== 'unchanged' && result.reason !== 'notFound') {
      reportTabMoved(scope, moveRefusalOutcomes[result.reason], getTabCount(target))
    }
    return result
  }

  if (leavingCursor !== undefined) target.tabs[result.toIndex].cursorFilename = leavingCursor
  reportTabMoved(scope, 'moved', getTabCount(target))
  saveTabsForPane(fromPane, deps.getTabMgr)
  if (crossPane) saveTabsForPane(toPane, deps.getTabMgr)
  // Only the active tab crossing over changes which tab is active anywhere.
  if (crossPane && result.wasActive && fromPane === deps.getFocusedPane()) {
    syncPinTabMenuForPane(fromPane, deps.getTabMgr)
  }
  return result
}

/**
 * A tab dropped on a tab bar. A full pane is the one refusal a drag can reach (a pinned tab
 * and a pane's only tab never start one): the "not allowed" cursor already showed during
 * the drag, and the toast says why. The MCP tool skips this: its caller gets the refusal as
 * a typed error.
 */
export function handleTabDrop(request: TabMoveRequest, deps: TabMoveDeps): void {
  const result = moveTabToPane(request, deps)
  if (!result.moved && result.reason === 'targetFull') {
    addToast(tString('fileExplorer.tabs.limitReached'), { level: 'warn' })
  }
}
