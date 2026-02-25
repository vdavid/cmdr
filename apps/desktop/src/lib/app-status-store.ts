// App status persistence for paths and focus state

import { load } from '@tauri-apps/plugin-store'
import type { Store } from '@tauri-apps/plugin-store'
import type { SortColumn } from './file-explorer/types'
import { defaultSortOrders } from './file-explorer/types'
import type { PersistedTab, PersistedPaneTabs } from './file-explorer/tabs/tab-types'

const STORE_NAME = 'app-status.json'
const DEFAULT_PATH = '~'
const ROOT_PATH = '/'
const DEFAULT_VOLUME_ID = 'root'
const DEFAULT_SORT_BY: SortColumn = 'name'

export type ViewMode = 'full' | 'brief'

export interface AppStatus {
    leftPath: string
    rightPath: string
    focusedPane: 'left' | 'right'
    leftViewMode: ViewMode
    rightViewMode: ViewMode
    leftVolumeId: string
    rightVolumeId: string
    leftSortBy: SortColumn
    rightSortBy: SortColumn
    /** Left pane width as percentage (25-75). Default: 50 */
    leftPaneWidthPercent: number
}

const DEFAULT_LEFT_PANE_WIDTH_PERCENT = 50

const DEFAULT_STATUS: AppStatus = {
    leftPath: DEFAULT_PATH,
    rightPath: DEFAULT_PATH,
    focusedPane: 'left',
    leftViewMode: 'brief',
    rightViewMode: 'brief',
    leftVolumeId: DEFAULT_VOLUME_ID,
    rightVolumeId: DEFAULT_VOLUME_ID,
    leftSortBy: DEFAULT_SORT_BY,
    rightSortBy: DEFAULT_SORT_BY,
    leftPaneWidthPercent: DEFAULT_LEFT_PANE_WIDTH_PERCENT,
}

let storeInstance: Store | null = null

async function getStore(): Promise<Store> {
    if (!storeInstance) {
        storeInstance = await load(STORE_NAME)
    }
    return storeInstance
}

/**
 * Resolves a path with fallback logic.
 * If the path doesn't exist, tries parent directories up to root.
 * Falls back to home (~) if nothing exists.
 */
async function resolvePathWithFallback(path: string, pathExists: (p: string) => Promise<boolean>): Promise<string> {
    // Start with the saved path
    let currentPath = path

    // Try the path and its parents
    while (currentPath && currentPath !== ROOT_PATH) {
        if (await pathExists(currentPath)) {
            return currentPath
        }
        // Try parent directory
        const parentPath = currentPath.substring(0, currentPath.lastIndexOf('/')) || ROOT_PATH
        currentPath = parentPath === currentPath ? ROOT_PATH : parentPath
    }

    // Check if root exists
    if (await pathExists(ROOT_PATH)) {
        return ROOT_PATH
    }

    // Ultimate fallback to home
    return DEFAULT_PATH
}

function parseViewMode(raw: unknown): ViewMode {
    return raw === 'full' || raw === 'brief' ? raw : 'brief'
}

function parseSortColumn(raw: unknown): SortColumn {
    const validColumns: SortColumn[] = ['name', 'extension', 'size', 'modified', 'created']
    if (typeof raw === 'string' && validColumns.includes(raw as SortColumn)) {
        return raw as SortColumn
    }
    return DEFAULT_SORT_BY
}

function parsePaneWidthPercent(raw: unknown): number {
    if (typeof raw === 'number' && raw >= 25 && raw <= 75) {
        return raw
    }
    return DEFAULT_LEFT_PANE_WIDTH_PERCENT
}

export async function loadAppStatus(pathExists: (p: string) => Promise<boolean>): Promise<AppStatus> {
    try {
        const store = await getStore()
        const leftPath = ((await store.get('leftPath')) as string) || DEFAULT_PATH
        const rightPath = ((await store.get('rightPath')) as string) || DEFAULT_PATH
        const rawFocusedPane = await store.get('focusedPane')
        const focusedPane: 'left' | 'right' = rawFocusedPane === 'right' ? 'right' : 'left'
        const leftViewMode = parseViewMode(await store.get('leftViewMode'))
        const rightViewMode = parseViewMode(await store.get('rightViewMode'))
        const leftVolumeId = ((await store.get('leftVolumeId')) as string) || DEFAULT_VOLUME_ID
        const rightVolumeId = ((await store.get('rightVolumeId')) as string) || DEFAULT_VOLUME_ID
        const leftSortBy = parseSortColumn(await store.get('leftSortBy'))
        const rightSortBy = parseSortColumn(await store.get('rightSortBy'))
        const leftPaneWidthPercent = parsePaneWidthPercent(await store.get('leftPaneWidthPercent'))

        // Resolve paths with fallback - skip for virtual 'network' volume
        const resolvedLeftPath =
            leftVolumeId === 'network' ? leftPath : await resolvePathWithFallback(leftPath, pathExists)
        const resolvedRightPath =
            rightVolumeId === 'network' ? rightPath : await resolvePathWithFallback(rightPath, pathExists)

        return {
            leftPath: resolvedLeftPath,
            rightPath: resolvedRightPath,
            focusedPane,
            leftViewMode,
            rightViewMode,
            leftVolumeId,
            rightVolumeId,
            leftSortBy,
            rightSortBy,
            leftPaneWidthPercent,
        }
    } catch {
        // If store fails, return defaults
        return DEFAULT_STATUS
    }
}

const SAVE_DEBOUNCE_MS = 200
let saveDebounceTimer: ReturnType<typeof setTimeout> | null = null
let pendingSave: Partial<AppStatus> | null = null

/** Debounced save: merges with pending writes and flushes after 200ms of inactivity. */
export function saveAppStatus(status: Partial<AppStatus>): void {
    pendingSave = { ...pendingSave, ...status }

    if (saveDebounceTimer !== null) {
        clearTimeout(saveDebounceTimer)
    }

    saveDebounceTimer = setTimeout(() => {
        const toSave = pendingSave
        pendingSave = null
        saveDebounceTimer = null
        if (toSave) {
            void doSaveAppStatus(toSave)
        }
    }, SAVE_DEBOUNCE_MS)
}

async function doSaveAppStatus(status: Partial<AppStatus>): Promise<void> {
    try {
        const store = await getStore()
        if (status.leftPath !== undefined) {
            await store.set('leftPath', status.leftPath)
        }
        if (status.rightPath !== undefined) {
            await store.set('rightPath', status.rightPath)
        }
        if (status.focusedPane !== undefined) {
            await store.set('focusedPane', status.focusedPane)
        }
        if (status.leftViewMode !== undefined) {
            await store.set('leftViewMode', status.leftViewMode)
        }
        if (status.rightViewMode !== undefined) {
            await store.set('rightViewMode', status.rightViewMode)
        }
        if (status.leftVolumeId !== undefined) {
            await store.set('leftVolumeId', status.leftVolumeId)
        }
        if (status.rightVolumeId !== undefined) {
            await store.set('rightVolumeId', status.rightVolumeId)
        }
        if (status.leftSortBy !== undefined) {
            await store.set('leftSortBy', status.leftSortBy)
        }
        if (status.rightSortBy !== undefined) {
            await store.set('rightSortBy', status.rightSortBy)
        }
        if (status.leftPaneWidthPercent !== undefined) {
            await store.set('leftPaneWidthPercent', status.leftPaneWidthPercent)
        }
        await store.save()
    } catch {
        // Silently fail - persistence is nice-to-have
    }
}

/** Map of volumeId -> last used path for that volume */
export type VolumePathMap = Record<string, string>

function isValidPathMap(value: unknown): value is VolumePathMap {
    if (typeof value !== 'object' || value === null) return false
    return Object.entries(value).every(([k, v]) => typeof k === 'string' && typeof v === 'string')
}

/**
 * Gets the last used path for a specific volume.
 * Returns undefined if no path is stored.
 */
export async function getLastUsedPathForVolume(volumeId: string): Promise<string | undefined> {
    try {
        const store = await getStore()
        const lastUsedPaths = await store.get('lastUsedPaths')
        if (isValidPathMap(lastUsedPaths)) {
            return lastUsedPaths[volumeId]
        }
        return undefined
    } catch {
        return undefined
    }
}

/**
 * Saves the last used path for a specific volume.
 * This is more efficient than loading/saving the full status.
 */
export async function saveLastUsedPathForVolume(volumeId: string, path: string): Promise<void> {
    try {
        const store = await getStore()
        const lastUsedPaths = await store.get('lastUsedPaths')
        const paths: VolumePathMap = isValidPathMap(lastUsedPaths) ? lastUsedPaths : {}
        paths[volumeId] = path
        await store.set('lastUsedPaths', paths)
        await store.save()
    } catch {
        // Silently fail - persistence is nice-to-have
    }
}

// ============================================================================
// Command palette query persistence
// ============================================================================

/**
 * Loads the last used command palette query.
 * Returns empty string if not previously saved.
 */
export async function loadPaletteQuery(): Promise<string> {
    try {
        const store = await getStore()
        const query = await store.get('paletteQuery')
        return typeof query === 'string' ? query : ''
    } catch {
        return ''
    }
}

/**
 * Saves the current command palette query for next time.
 */
export async function savePaletteQuery(query: string): Promise<void> {
    try {
        const store = await getStore()
        await store.set('paletteQuery', query)
        await store.save()
    } catch {
        // Silently fail
    }
}

// ============================================================================
// Settings window section persistence
// ============================================================================

const DEFAULT_SETTINGS_SECTION = ['General', 'Appearance']

/**
 * Loads the last viewed settings section.
 * Returns default section if not previously saved.
 */
export async function loadLastSettingsSection(): Promise<string[]> {
    try {
        const store = await getStore()
        const section = await store.get('lastSettingsSection')
        if (Array.isArray(section) && section.every((s): s is string => typeof s === 'string')) {
            return section
        }
        return DEFAULT_SETTINGS_SECTION
    } catch {
        return DEFAULT_SETTINGS_SECTION
    }
}

/**
 * Saves the current settings section for next time.
 */
export async function saveLastSettingsSection(section: string[]): Promise<void> {
    try {
        const store = await getStore()
        await store.set('lastSettingsSection', section)
        await store.save()
    } catch {
        // Silently fail
    }
}

// ============================================================================
// Tab persistence
// ============================================================================

function isValidPersistedTab(raw: unknown): raw is PersistedTab {
    if (typeof raw !== 'object' || raw === null) return false
    const obj = raw as Record<string, unknown>
    return (
        typeof obj.id === 'string' &&
        typeof obj.path === 'string' &&
        typeof obj.volumeId === 'string' &&
        parseSortColumn(obj.sortBy) === obj.sortBy &&
        (obj.sortOrder === 'ascending' || obj.sortOrder === 'descending') &&
        (obj.viewMode === 'full' || obj.viewMode === 'brief') &&
        typeof obj.pinned === 'boolean'
    )
}

function isValidPersistedPaneTabs(raw: unknown): raw is PersistedPaneTabs {
    if (typeof raw !== 'object' || raw === null) return false
    const obj = raw as Record<string, unknown>
    if (!Array.isArray(obj.tabs) || typeof obj.activeTabId !== 'string') return false
    return obj.tabs.length > 0 && obj.tabs.every(isValidPersistedTab)
}

/**
 * Loads persisted tab state for a pane side.
 * Falls back to migration from old scalar keys if no tab data exists.
 */
export async function loadPaneTabs(
    side: 'left' | 'right',
    pathExistsFn: (p: string) => Promise<boolean>,
): Promise<PersistedPaneTabs> {
    try {
        const store = await getStore()
        const key = `${side}Tabs`
        const raw = await store.get(key)

        if (isValidPersistedPaneTabs(raw)) {
            // Validate paths exist, fall back for any that don't
            const validatedTabs = await Promise.all(
                raw.tabs.map(async (tab) => {
                    if (tab.volumeId === 'network') return tab
                    const resolvedPath = await resolvePathWithFallback(tab.path, pathExistsFn)
                    return { ...tab, path: resolvedPath }
                }),
            )
            return { tabs: validatedTabs, activeTabId: raw.activeTabId }
        }

        // TODO(2026-04-01): remove migration
        // Migration from old scalar keys
        const path = ((await store.get(`${side}Path`)) as string) || DEFAULT_PATH
        const volumeId = ((await store.get(`${side}VolumeId`)) as string) || DEFAULT_VOLUME_ID
        const sortBy = parseSortColumn(await store.get(`${side}SortBy`))
        const viewMode = parseViewMode(await store.get(`${side}ViewMode`))
        const resolvedPath = volumeId === 'network' ? path : await resolvePathWithFallback(path, pathExistsFn)

        const tab: PersistedTab = {
            id: crypto.randomUUID(),
            path: resolvedPath,
            volumeId,
            sortBy,
            sortOrder: defaultSortOrders[sortBy],
            viewMode,
            pinned: false,
        }

        return { tabs: [tab], activeTabId: tab.id }
    } catch {
        const id = crypto.randomUUID()
        return {
            tabs: [
                {
                    id,
                    path: DEFAULT_PATH,
                    volumeId: DEFAULT_VOLUME_ID,
                    sortBy: DEFAULT_SORT_BY,
                    sortOrder: defaultSortOrders[DEFAULT_SORT_BY],
                    viewMode: 'brief' as ViewMode,
                    pinned: false,
                },
            ],
            activeTabId: id,
        }
    }
}

/** Saves tab state for a pane side. */
export async function savePaneTabs(side: 'left' | 'right', paneTabs: PersistedPaneTabs): Promise<void> {
    try {
        const store = await getStore()
        await store.set(`${side}Tabs`, paneTabs)
        await store.save()
    } catch {
        // Silently fail
    }
}
