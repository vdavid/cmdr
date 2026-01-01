// App status persistence for paths and focus state

import { load } from '@tauri-apps/plugin-store'
import type { Store } from '@tauri-apps/plugin-store'

const STORE_NAME = 'app-status.json'
const DEFAULT_PATH = '~'
const ROOT_PATH = '/'
const DEFAULT_VOLUME_ID = 'root'

export type ViewMode = 'full' | 'brief'

export interface AppStatus {
    leftPath: string
    rightPath: string
    focusedPane: 'left' | 'right'
    leftViewMode: ViewMode
    rightViewMode: ViewMode
    leftVolumeId: string
    rightVolumeId: string
}

const DEFAULT_STATUS: AppStatus = {
    leftPath: DEFAULT_PATH,
    rightPath: DEFAULT_PATH,
    focusedPane: 'left',
    leftViewMode: 'brief',
    rightViewMode: 'brief',
    leftVolumeId: DEFAULT_VOLUME_ID,
    rightVolumeId: DEFAULT_VOLUME_ID,
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

        // Resolve paths with fallback
        const resolvedLeftPath = await resolvePathWithFallback(leftPath, pathExists)
        const resolvedRightPath = await resolvePathWithFallback(rightPath, pathExists)

        return {
            leftPath: resolvedLeftPath,
            rightPath: resolvedRightPath,
            focusedPane,
            leftViewMode,
            rightViewMode,
            leftVolumeId,
            rightVolumeId,
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
    } catch {
        // Silently fail - persistence is nice-to-have
    }
}
