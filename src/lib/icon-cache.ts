// Icon cache for efficient icon loading
// Caches icon data URLs by icon ID to avoid redundant Tauri calls

import { getIcons } from './tauri-commands'

const STORAGE_KEY = 'rusty-commander-icon-cache'

/** In-memory cache for current session */
const memoryCache = new Map<string, string>()

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
 */
export async function prefetchIcons(iconIds: string[]): Promise<void> {
    const uncached = iconIds.filter((id) => !memoryCache.has(id))
    if (uncached.length === 0) return

    // Deduplicate
    const unique = [...new Set(uncached)]
    const icons = await getIcons(unique)

    for (const [id, url] of Object.entries(icons)) {
        memoryCache.set(id, url)
    }
    saveToStorage()
}

/**
 * Gets icon from cache only (no fetch).
 * Returns undefined if not cached.
 */
export function getCachedIcon(iconId: string): string | undefined {
    return memoryCache.get(iconId)
}
