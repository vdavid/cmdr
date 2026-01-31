/**
 * Shared utilities for BriefList and FullList components.
 */

import type { FileEntry, SyncStatus } from './types'
import { getFileRange } from '$lib/tauri-commands'
import { prefetchIcons } from '$lib/icon-cache'
import { getUseAppIconsForDocuments } from '$lib/settings/reactive-settings.svelte'

/** Prefetch buffer - load this many items around visible range */
export const PREFETCH_BUFFER = 200

/** Sync status icon paths - returns undefined if no icon should be shown */
export function getSyncIconPath(status: SyncStatus | undefined): string | undefined {
    if (!status) return undefined
    const iconMap: Record<SyncStatus, string | undefined> = {
        synced: '/icons/sync-synced.svg',
        online_only: '/icons/sync-online-only.svg',
        uploading: '/icons/sync-uploading.svg',
        downloading: '/icons/sync-downloading.svg',
        unknown: undefined,
    }
    return iconMap[status]
}

/** Creates a parent directory entry ("..") */
export function createParentEntry(parentPath: string): FileEntry {
    return {
        name: '..',
        path: parentPath,
        isDirectory: true,
        isSymlink: false,
        permissions: 0o755,
        owner: '',
        group: '',
        iconId: 'dir',
        extendedMetadataLoaded: true,
    }
}

/** Gets entry at global index, handling ".." entry */
export function getEntryAt(
    globalIndex: number,
    hasParent: boolean,
    parentPath: string,
    cachedEntries: FileEntry[],
    cachedRange: { start: number; end: number },
): FileEntry | undefined {
    if (hasParent && globalIndex === 0) {
        return createParentEntry(parentPath)
    }

    // Backend index (without ".." entry)
    const backendIndex = hasParent ? globalIndex - 1 : globalIndex

    // Find in cached entries
    if (backendIndex >= cachedRange.start && backendIndex < cachedRange.end) {
        return cachedEntries[backendIndex - cachedRange.start]
    }

    return undefined
}

/** Fallback emoji for files without icons */
export function getFallbackEmoji(file: FileEntry): string {
    if (file.isSymlink) return '🔗'
    if (file.isDirectory) return '📁'
    return '📄'
}

/** Parameters for fetchVisibleRange */
export interface FetchRangeParams {
    listingId: string
    startItem: number
    endItem: number
    hasParent: boolean
    totalCount: number
    includeHidden: boolean
    cachedRange: { start: number; end: number }
    onSyncStatusRequest?: (paths: string[]) => void
}

/** Result of fetchVisibleRange */
export interface FetchRangeResult {
    entries: FileEntry[]
    range: { start: number; end: number }
}

/** Calculates the fetch range for visible items with prefetch buffer */
export function calculateFetchRange(params: {
    startItem: number
    endItem: number
    hasParent: boolean
    totalCount: number
}): { fetchStart: number; fetchEnd: number } {
    const { startItem, endItem, hasParent, totalCount } = params

    // Account for ".." entry
    let adjustedStart = startItem
    let adjustedEnd = endItem
    if (hasParent) {
        adjustedStart = Math.max(0, adjustedStart - 1)
        adjustedEnd = Math.max(0, adjustedEnd - 1)
    }

    // Add prefetch buffer
    const fetchStart = Math.max(0, adjustedStart - PREFETCH_BUFFER / 2)
    const fetchEnd = Math.min(hasParent ? totalCount - 1 : totalCount, adjustedEnd + PREFETCH_BUFFER / 2)

    return { fetchStart, fetchEnd }
}

/** Checks if the needed range is already cached */
export function isRangeCached(
    fetchStart: number,
    fetchEnd: number,
    cachedRange: { start: number; end: number },
): boolean {
    return fetchStart >= cachedRange.start && fetchEnd <= cachedRange.end
}

/** Fetches entries for a visible range with prefetch buffer */
export async function fetchVisibleRange(params: FetchRangeParams): Promise<FetchRangeResult | null> {
    const { listingId, startItem, endItem, hasParent, totalCount, includeHidden, cachedRange, onSyncStatusRequest } =
        params

    const { fetchStart, fetchEnd } = calculateFetchRange({ startItem, endItem, hasParent, totalCount })

    // Only fetch if needed range isn't cached
    if (isRangeCached(fetchStart, fetchEnd, cachedRange)) {
        return null // Already cached
    }

    const entries = await getFileRange(listingId, fetchStart, fetchEnd - fetchStart, includeHidden)

    // Prefetch icons for visible entries
    const iconIds = entries.map((e) => e.iconId).filter((id) => id)
    const useAppIcons = getUseAppIconsForDocuments()
    void prefetchIcons(iconIds, useAppIcons)

    // Request sync status for visible paths
    const paths = entries.map((e) => e.path)
    onSyncStatusRequest?.(paths)

    return {
        entries,
        range: { start: fetchStart, end: fetchStart + entries.length },
    }
}

/** Checks if cache props changed and returns whether reset is needed */
export function shouldResetCache(
    current: { listingId: string; includeHidden: boolean; totalCount: number; cacheGeneration: number },
    previous: { listingId: string; includeHidden: boolean; totalCount: number; cacheGeneration: number },
): boolean {
    return (
        current.listingId !== previous.listingId ||
        current.includeHidden !== previous.includeHidden ||
        current.totalCount !== previous.totalCount ||
        current.cacheGeneration !== previous.cacheGeneration
    )
}
