// Icon cache for efficient icon loading
// Caches icon data URLs by icon ID to avoid redundant Tauri calls

import { writable } from 'svelte/store'
import {
    getIcons,
    refreshDirectoryIcons as refreshIconsCommand,
    clearExtensionIconCache as clearExtensionIconCacheCommand,
    clearDirectoryIconCache as clearDirectoryIconCacheCommand,
} from './tauri-commands'

const STORAGE_KEY = 'cmdr-icon-cache'

/** In-memory cache for current session */
const memoryCache = new Map<string, string>()

/**
 * Reactive version counter - increments when cache updates.
 * Components can subscribe to this to know when to re-render.
 */
export const iconCacheVersion = writable(0)

/**
 * Reactive counter that increments when part of the icon cache is cleared
 * (extension icons, directory icons, etc.).
 * List components subscribe to this to re-fetch icons for visible files.
 */
export const iconCacheCleared = writable(0)

/** Load persisted cache from localStorage */
function loadFromStorage(): void {
    try {
        const stored = localStorage.getItem(STORAGE_KEY)
        if (stored) {
            const parsed = JSON.parse(stored) as Record<string, string>
            for (const [id, url] of Object.entries(parsed)) {
                memoryCache.set(id, url)
            }
        }
    } catch {
        // Ignore storage errors
    }
}

/** Persist cache to localStorage */
function saveToStorage(): void {
    try {
        const obj: Record<string, string> = {}
        for (const [id, url] of memoryCache) {
            obj[id] = url
        }
        localStorage.setItem(STORAGE_KEY, JSON.stringify(obj))
    } catch {
        // Ignore storage errors
    }
}

// Load on module init
if (typeof localStorage !== 'undefined') {
    loadFromStorage()
}

/**
 * Prefetches icons for the given IDs.
 * Fetches only those not already cached.
 * Increments iconCacheVersion when new icons are loaded, triggering re-renders.
 *
 * @param iconIds - Array of icon IDs to prefetch
 * @param useAppIconsForDocuments - Whether to use app icons as fallback for documents
 */
export async function prefetchIcons(iconIds: string[], useAppIconsForDocuments: boolean): Promise<void> {
    const uncached = iconIds.filter((id) => !memoryCache.has(id))
    if (uncached.length === 0) return

    // Deduplicate
    const unique = [...new Set(uncached)]
    const icons = await getIcons(unique, useAppIconsForDocuments)

    let added = false
    for (const [id, url] of Object.entries(icons)) {
        memoryCache.set(id, url)
        added = true
    }

    if (added) {
        saveToStorage()
        // Trigger reactive update for subscribed components
        iconCacheVersion.update((v) => v + 1)
    }
}

/**
 * Gets icon from cache only (no fetch).
 * Returns undefined if not cached.
 */
export function getCachedIcon(iconId: string): string | undefined {
    return memoryCache.get(iconId)
}

/**
 * Refreshes icons for a directory listing.
 * Fetches icons in parallel for:
 * - All directories by exact path (for custom folder icons)
 * - All unique extensions (for file association changes)
 *
 * Updates the cache and triggers re-render if any icons changed.
 * @param directoryPaths - Array of directory paths to fetch icons for
 * @param extensions - Array of file extensions (without dot)
 * @param useAppIconsForDocuments - Whether to use app icons as fallback for documents
 * @public
 */
export async function refreshDirectoryIcons(
    directoryPaths: string[],
    extensions: string[],
    useAppIconsForDocuments: boolean,
): Promise<void> {
    if (directoryPaths.length === 0 && extensions.length === 0) return

    const icons = await refreshIconsCommand(directoryPaths, extensions, useAppIconsForDocuments)

    let changed = false
    for (const [id, url] of Object.entries(icons)) {
        const existing = memoryCache.get(id)
        if (existing !== url) {
            memoryCache.set(id, url)
            changed = true
        }
    }

    if (changed) {
        saveToStorage()
        iconCacheVersion.update((v) => v + 1)
    }
}

/**
 * Clears all cached extension icons from both memory and localStorage.
 * Called when the "use app icons for documents" setting changes.
 * After calling this, extension icons will be re-fetched with the new setting.
 */
export async function clearExtensionIconCache(): Promise<void> {
    // Clear backend cache
    await clearExtensionIconCacheCommand()

    // Clear frontend cache (extension icons only)
    for (const key of memoryCache.keys()) {
        if (key.startsWith('ext:')) {
            memoryCache.delete(key)
        }
    }

    // Persist the change
    saveToStorage()

    // Notify list components to re-fetch icons for visible files
    // This must happen BEFORE incrementing iconCacheVersion so components
    // can re-fetch before re-rendering with the cleared cache
    iconCacheCleared.update((v) => v + 1)

    // Trigger reactive update so components re-fetch icons
    iconCacheVersion.update((v) => v + 1)
}

/**
 * Clears all cached directory icons from both memory and localStorage.
 * Called when the system theme or accent color changes, since macOS renders
 * folder icons with the current accent color baked in.
 */
export async function clearDirectoryIconCache(): Promise<void> {
    await clearDirectoryIconCacheCommand()

    for (const key of memoryCache.keys()) {
        if (key === 'dir' || key === 'symlink-dir' || key.startsWith('path:')) {
            memoryCache.delete(key)
        }
    }

    saveToStorage()
    iconCacheCleared.update((v) => v + 1)
    iconCacheVersion.update((v) => v + 1)
}
