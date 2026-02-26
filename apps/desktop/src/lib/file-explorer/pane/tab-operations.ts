import { confirmDialog } from '$lib/utils/confirm-dialog'
import { showTabContextMenu, onTabContextAction, updatePinTabMenu } from '$lib/tauri-commands'
import { savePaneTabs, type ViewMode } from '$lib/app-status-store'
import {
    createTabManager,
    getActiveTab,
    addTab,
    closeTab,
    closeOtherTabs,
    switchTab,
    cycleTab as cycleTabInManager,
    getAllTabs,
    getTabCount,
    pinTab,
    unpinTab,
    type TabManager,
} from '../tabs/tab-state-manager.svelte'
import type { TabState, TabId, PersistedTab, PersistedPaneTabs } from '../tabs/tab-types'
import { createHistory } from '../navigation/navigation-history'
import { DEFAULT_SORT_BY, defaultSortOrders, type SortColumn } from '../types'
import { getAppLogger } from '$lib/logging/logger'
import { addToast } from '$lib/ui/toast'
import type FilePane from './FilePane.svelte'

const log = getAppLogger('fileExplorer')

// --- Tab initialization helpers ---

export function createInitialTabState(
    path: string,
    volumeId: string,
    sortBy: SortColumn = DEFAULT_SORT_BY,
    viewMode: ViewMode = 'brief',
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
    }
}

export function createTabManagerFromPersisted(paneTabs: PersistedPaneTabs): TabManager {
    const tabs = paneTabs.tabs.map(
        (pt): TabState => ({
            ...pt,
            history: createHistory(pt.volumeId, pt.path),
            cursorFilename: null,
        }),
    )

    const mgr = createTabManager(tabs[0])
    for (let i = 1; i < tabs.length; i++) {
        mgr.tabs.push(tabs[i])
    }
    mgr.activeTabId = paneTabs.activeTabId
    return mgr
}

export function buildPersistedPaneTabs(mgr: TabManager): PersistedPaneTabs {
    return {
        tabs: getAllTabs(mgr).map(
            (tab): PersistedTab => ({
                id: tab.id,
                path: tab.path,
                volumeId: tab.volumeId,
                sortBy: tab.sortBy,
                sortOrder: tab.sortOrder,
                viewMode: tab.viewMode,
                pinned: tab.pinned,
            }),
        ),
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
) {
    const mgr = getTabMgr(pane)
    const tab = getAllTabs(mgr).find((t) => t.id === tabId)
    if (tab?.pinned) {
        const ok = await confirmDialog('This tab is pinned. Close it anyway?', 'Close pinned tab')
        if (!ok) return
    }
    closeTab(mgr, tabId)
    saveTabsForPane(pane, getTabMgr)
    if (pane === focusedPane) syncPinTabMenu()
}

export function handleTabMiddleClick(
    pane: 'left' | 'right',
    tabId: TabId,
    getTabMgr: (pane: 'left' | 'right') => TabManager,
    focusedPane: 'left' | 'right',
    syncPinTabMenu: () => void,
) {
    const mgr = getTabMgr(pane)
    const tab = getAllTabs(mgr).find((t) => t.id === tabId)
    if (!tab) return
    if (tab.pinned) {
        unpinTab(mgr, tabId)
    }
    void handleTabClose(pane, tabId, getTabMgr, focusedPane, syncPinTabMenu)
}

export async function handleTabContextMenu(
    pane: 'left' | 'right',
    tabId: TabId,
    event: MouseEvent,
    getTabMgr: (pane: 'left' | 'right') => TabManager,
    focusedPane: 'left' | 'right',
    syncPinTabMenu: () => void,
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
            saveTabsForPane(pane, getTabMgr)
            if (pane === focusedPane && tabId === mgr.activeTabId) syncPinTabMenu()
            break
        case 'tab_close_others':
            closeOtherTabs(mgr, tabId)
            saveTabsForPane(pane, getTabMgr)
            break
        case 'tab_close': {
            void handleTabClose(pane, tabId, getTabMgr, focusedPane, syncPinTabMenu)
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
    }

    const success = addTab(mgr, activeTab.id, cloneTab)
    if (success && wasPinned) {
        unpinTab(mgr, activeTab.id)
    }
    if (success) {
        saveTabsForPane(focusedPane, getTabMgr)
    }
    return success
}

/** Closes the active tab. Returns 'closed' or 'last-tab'. */
export function closeActiveTab(
    focusedPane: 'left' | 'right',
    getTabMgr: (pane: 'left' | 'right') => TabManager,
): 'closed' | 'last-tab' {
    const mgr = getTabMgr(focusedPane)
    const result = closeTab(mgr, mgr.activeTabId)
    if (result.closed) {
        saveTabsForPane(focusedPane, getTabMgr)
    }
    return result.closed ? 'closed' : 'last-tab'
}

/** Closes the active tab with pinned confirmation if needed. */
export async function closeActiveTabWithConfirmation(
    focusedPane: 'left' | 'right',
    getTabMgr: (pane: 'left' | 'right') => TabManager,
): Promise<'closed' | 'last-tab' | 'cancelled'> {
    const mgr = getTabMgr(focusedPane)
    const activeTab = getActiveTab(mgr)

    // Last tab: close window without confirmation (even if pinned)
    if (getTabCount(mgr) <= 1) {
        return 'last-tab'
    }

    // Pinned tab: confirm before closing
    if (activeTab.pinned) {
        const ok = await confirmDialog('This tab is pinned. Close it anyway?', 'Close pinned tab')
        if (!ok) return 'cancelled'
    }

    const result = closeTab(mgr, mgr.activeTabId)
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
) {
    const mgr = getTabMgr(focusedPane)
    closeOtherTabs(mgr, mgr.activeTabId)
    saveTabsForPane(focusedPane, getTabMgr)
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
    getPaneRef: (pane: 'left' | 'right') => FilePane | undefined,
) {
    const mgr = getTabMgr(focusedPane)
    const paneRef = getPaneRef(focusedPane)
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call
    const cursorFilename = (paneRef?.getFilenameUnderCursor?.() as string | undefined) ?? null
    cycleTabInManager(mgr, direction, cursorFilename)
    saveTabsForPane(focusedPane, getTabMgr)
    syncPinTabMenuForPane(focusedPane, getTabMgr)
}

/** Switch to a specific tab by ID in the given pane. Returns false if tab not found. */
export function switchToTab(
    pane: 'left' | 'right',
    tabId: TabId,
    getTabMgr: (pane: 'left' | 'right') => TabManager,
    getPaneRef: (pane: 'left' | 'right') => FilePane | undefined,
    focusedPane: 'left' | 'right',
) {
    const mgr = getTabMgr(pane)
    const paneRef = getPaneRef(pane)
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call
    const cursorFilename = (paneRef?.getFilenameUnderCursor?.() as string | undefined) ?? null
    const switched = switchTab(mgr, tabId, cursorFilename)
    if (!switched) {
        log.warn(`MCP activate_tab: tab ${tabId} not found in ${pane} pane`)
        return false
    }
    saveTabsForPane(pane, getTabMgr)
    if (pane === focusedPane) syncPinTabMenuForPane(focusedPane, getTabMgr)
    return true
}

/** Get all tabs for a pane (for TabBar). */
export function getTabsForPane(
    pane: 'left' | 'right',
    getTabMgr: (pane: 'left' | 'right') => TabManager,
): { tabs: TabState[]; activeTabId: TabId } {
    const mgr = getTabMgr(pane)
    return { tabs: getAllTabs(mgr), activeTabId: mgr.activeTabId }
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
        addToast('Tab limit reached')
    }
    setFocusedPane(focusedPane === pane ? pane : focusedPane)
}
