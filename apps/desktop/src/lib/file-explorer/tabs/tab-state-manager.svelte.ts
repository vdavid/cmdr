import type { TabId, TabState } from './tab-types'

export const MAX_TABS_PER_PANE = 10

export interface TabManager {
    tabs: TabState[]
    activeTabId: TabId
}

/** Creates a tab manager with a single initial tab */
export function createTabManager(initialTab: TabState): TabManager {
    const tabs = $state<TabState[]>([initialTab])
    let activeTabId = $state<TabId>(initialTab.id)

    return {
        get tabs() {
            return tabs
        },
        set tabs(value: TabState[]) {
            tabs.length = 0
            tabs.push(...value)
        },
        get activeTabId() {
            return activeTabId
        },
        set activeTabId(value: TabId) {
            activeTabId = value
        },
    }
}

/** Returns the currently active tab. Falls back to first tab if active ID is stale. */
export function getActiveTab(mgr: TabManager): TabState {
    const tab = mgr.tabs.find((t) => t.id === mgr.activeTabId)
    if (!tab) {
        if (mgr.tabs.length > 0) {
            mgr.activeTabId = mgr.tabs[0].id
            return mgr.tabs[0]
        }
        throw new Error('Tab manager has no tabs')
    }
    return tab
}

/**
 * Inserts a new tab to the left of beforeTabId.
 * Returns false if at cap (10 tabs).
 */
export function addTab(mgr: TabManager, beforeTabId: TabId, tabState: TabState): boolean {
    if (mgr.tabs.length >= MAX_TABS_PER_PANE) {
        return false
    }

    const beforeIndex = mgr.tabs.findIndex((t) => t.id === beforeTabId)
    if (beforeIndex === -1) {
        // If beforeTabId not found, append at end
        mgr.tabs.push(tabState)
    } else {
        mgr.tabs.splice(beforeIndex, 0, tabState)
    }

    // Don't change activeTabId — the clone trick relies on this:
    // the clone is inserted to the LEFT, the original tab stays active
    return true
}

/** Result of closing a tab */
export type CloseTabResult = { closed: true; newActiveTabId: TabId } | { closed: false }

/** Closes a tab. Returns the new active tab ID if closed, or false if it's the last tab. */
export function closeTab(mgr: TabManager, tabId: TabId): CloseTabResult {
    if (mgr.tabs.length <= 1) {
        return { closed: false }
    }

    const index = mgr.tabs.findIndex((t) => t.id === tabId)
    if (index === -1) {
        return { closed: false }
    }

    const wasActive = mgr.activeTabId === tabId

    mgr.tabs.splice(index, 1)

    if (wasActive) {
        // Activate next tab to the right, or left neighbor if rightmost
        const newIndex = index < mgr.tabs.length ? index : mgr.tabs.length - 1
        mgr.activeTabId = mgr.tabs[newIndex].id
    }

    return { closed: true, newActiveTabId: mgr.activeTabId }
}

/** Closes all unpinned tabs except the given one. Pinned tabs stay. The given tab becomes active. */
export function closeOtherTabs(mgr: TabManager, tabId: TabId): void {
    mgr.tabs = mgr.tabs.filter((t) => t.id === tabId || t.pinned)
    mgr.activeTabId = tabId
}

/** Stores cursor filename on the old active tab, then activates the new tab.
 *  Returns false if the target tab ID doesn't exist (no-op in that case). */
export function switchTab(mgr: TabManager, tabId: TabId, cursorFilename: string | null): boolean {
    const targetTab = mgr.tabs.find((t) => t.id === tabId)
    if (!targetTab) {
        return false
    }
    const currentTab = mgr.tabs.find((t) => t.id === mgr.activeTabId)
    if (currentTab) {
        currentTab.cursorFilename = cursorFilename
    }
    mgr.activeTabId = tabId
    return true
}

/** Pins a tab */
export function pinTab(mgr: TabManager, tabId: TabId): void {
    const tab = mgr.tabs.find((t) => t.id === tabId)
    if (tab) {
        tab.pinned = true
    }
}

/** Unpins a tab */
export function unpinTab(mgr: TabManager, tabId: TabId): void {
    const tab = mgr.tabs.find((t) => t.id === tabId)
    if (tab) {
        tab.pinned = false
    }
}

/** Debounce state for cycleTab */
let cycleDebounceTimer: ReturnType<typeof setTimeout> | null = null
let cycleDebounceActiveTabId: TabId | null = null

const CYCLE_DEBOUNCE_MS = 50

/**
 * Cycles to the next or previous tab, wrapping around.
 * Uses leading-edge debounce (~50ms) so rapid cycling only commits the final tab.
 * Returns the new active tab ID.
 */
export function cycleTab(mgr: TabManager, direction: 'next' | 'prev', cursorFilename: string | null): TabId {
    const currentIndex = mgr.tabs.findIndex((t) => t.id === mgr.activeTabId)
    if (currentIndex === -1) {
        return mgr.activeTabId
    }

    const nextIndex =
        direction === 'next'
            ? (currentIndex + 1) % mgr.tabs.length
            : (currentIndex - 1 + mgr.tabs.length) % mgr.tabs.length

    const targetTabId = mgr.tabs[nextIndex].id

    // Leading-edge debounce: first call fires immediately, subsequent calls within
    // the debounce window are batched, and the final one fires after the timeout
    if (cycleDebounceTimer === null) {
        // First call: fire immediately
        switchTab(mgr, targetTabId, cursorFilename)
        cycleDebounceActiveTabId = targetTabId

        cycleDebounceTimer = setTimeout(() => {
            // After timeout, commit the last stored tab if it differs from what was already applied
            if (cycleDebounceActiveTabId !== null && cycleDebounceActiveTabId !== mgr.activeTabId) {
                switchTab(mgr, cycleDebounceActiveTabId, cursorFilename)
            }
            cycleDebounceTimer = null
            cycleDebounceActiveTabId = null
        }, CYCLE_DEBOUNCE_MS)
    } else {
        // Subsequent call within debounce window: just store the target
        cycleDebounceActiveTabId = targetTabId
    }

    return targetTabId
}

/** Resets the cycle debounce state. Useful for testing. */
export function resetCycleDebounce(): void {
    if (cycleDebounceTimer !== null) {
        clearTimeout(cycleDebounceTimer)
        cycleDebounceTimer = null
    }
    cycleDebounceActiveTabId = null
}

/** Returns all tabs */
export function getAllTabs(mgr: TabManager): TabState[] {
    return mgr.tabs
}

/** Returns the number of tabs */
export function getTabCount(mgr: TabManager): number {
    return mgr.tabs.length
}
