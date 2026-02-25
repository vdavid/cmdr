import type { NavigationHistory } from '../navigation/navigation-history'
import type { SortColumn, SortOrder } from '../types'
import type { ViewMode } from '$lib/app-status-store'

export type TabId = string // crypto.randomUUID()

/** Full runtime state for one tab */
export interface TabState {
    id: TabId
    path: string
    volumeId: string
    history: NavigationHistory
    sortBy: SortColumn
    sortOrder: SortOrder
    viewMode: ViewMode
    pinned: boolean
    /** Saved on switch-away, restored on switch-to */
    cursorFilename: string | null
}

/** Stored in app-status.json per tab */
export interface PersistedTab {
    id: TabId
    path: string
    volumeId: string
    sortBy: SortColumn
    sortOrder: SortOrder
    viewMode: ViewMode
    pinned: boolean
}

/** Stored in app-status.json per pane side */
export interface PersistedPaneTabs {
    tabs: PersistedTab[]
    activeTabId: TabId
}
