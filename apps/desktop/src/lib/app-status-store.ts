// App status persistence for paths and focus state

import { load } from '@tauri-apps/plugin-store'
import type { Store } from '@tauri-apps/plugin-store'
import type { SortColumn, SortOrder } from './file-explorer/types'
import { defaultSortOrders } from './file-explorer/types'

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

export async function saveAppStatus(status: Partial<AppStatus>): Promise<void> {
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

// ============================================================================
// Column sort order memory (app-wide, per-column)
// ============================================================================

type ColumnSortOrders = Partial<Record<SortColumn, SortOrder>>

function isValidSortOrders(value: unknown): value is ColumnSortOrders {
    if (typeof value !== 'object' || value === null) return false
    const validColumns: string[] = ['name', 'extension', 'size', 'modified', 'created']
    const validOrders: string[] = ['ascending', 'descending']
    return Object.entries(value).every(([k, v]) => validColumns.includes(k) && validOrders.includes(v as string))
}

/**
 * Gets the remembered sort order for a column.
 * Returns the default sort order for that column if not previously set.
 * @public
 */
export async function getColumnSortOrder(column: SortColumn): Promise<SortOrder> {
    try {
        const store = await getStore()
        const orders = await store.get('columnSortOrders')
        if (isValidSortOrders(orders) && orders[column]) {
            return orders[column]
        }
        return defaultSortOrders[column]
    } catch {
        return defaultSortOrders[column]
    }
}

/**
 * Saves the sort order for a column (remembered for next time this column is clicked).
 * @public
 */
export async function saveColumnSortOrder(column: SortColumn, order: SortOrder): Promise<void> {
    try {
        const store = await getStore()
        const orders = await store.get('columnSortOrders')
        const current: ColumnSortOrders = isValidSortOrders(orders) ? orders : {}
        current[column] = order
        await store.set('columnSortOrders', current)
        await store.save()
    } catch {
        // Silently fail
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
