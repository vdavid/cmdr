/**
 * Shared utilities for BriefList and FullList components.
 */

import type { FileEntry, SyncStatus } from '../types'
import { getFileRange, getDirStatsBatch, type DirStats } from '$lib/tauri-commands'
import { prefetchIcons, prefetchCustomFolderIcons } from '$lib/icon-cache'
import { getUseAppIconsForDocuments } from '$lib/settings/reactive-settings.svelte'
import { getSetting } from '$lib/settings/settings-store'

export type { DirStats } from '$lib/tauri-commands'

/** Gets the prefetch buffer size from settings (items to load around visible range) */
export function getPrefetchBufferSize(): number {
  return getSetting('advanced.prefetchBufferSize')
}

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

/**
 * Creates a parent directory entry (".."). When `stats` is provided, the entry
 * carries the CURRENT directory's recursive size fields, so the ".." row shows
 * the total for the folder we're looking at, not the folder we'd navigate into.
 */
export function createParentEntry(parentPath: string, stats?: DirStats): FileEntry {
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
    recursiveSize: stats?.recursiveSize,
    recursivePhysicalSize: stats?.recursivePhysicalSize,
    recursiveFileCount: stats?.recursiveFileCount,
    recursiveDirCount: stats?.recursiveDirCount,
    // The ".." row shows the CURRENT folder's size — the exact dir the user
    // watches drain — so carry its pending flag on first paint, not just after
    // the first in-place refresh tick.
    recursiveSizePending: stats?.recursiveSizePending,
  }
}

/** Gets entry at global index, handling ".." entry */
export function getEntryAt(
  globalIndex: number,
  hasParent: boolean,
  parentPath: string,
  cachedEntries: FileEntry[],
  cachedRange: { start: number; end: number },
  parentStats?: DirStats,
): FileEntry | undefined {
  if (hasParent && globalIndex === 0) {
    return createParentEntry(parentPath, parentStats)
  }

  // Backend index (without ".." entry)
  const backendIndex = hasParent ? globalIndex - 1 : globalIndex

  // Find in cached entries
  if (backendIndex >= cachedRange.start && backendIndex < cachedRange.end) {
    return cachedEntries[backendIndex - cachedRange.start]
  }

  return undefined
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
  /**
   * Bypass the "already cached" short-circuit. Set when the backing listing
   * changed (e.g. a `directory-diff` event added/removed entries within the
   * cached range) so the cached entries are stale even though the range
   * indices haven't moved.
   */
  force?: boolean
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
  const prefetchBuffer = getPrefetchBufferSize()
  const fetchStart = Math.max(0, adjustedStart - prefetchBuffer / 2)
  const fetchEnd = Math.min(hasParent ? totalCount - 1 : totalCount, adjustedEnd + prefetchBuffer / 2)

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
  const {
    listingId,
    startItem,
    endItem,
    hasParent,
    totalCount,
    includeHidden,
    cachedRange,
    onSyncStatusRequest,
    force,
  } = params

  const { fetchStart, fetchEnd } = calculateFetchRange({ startItem, endItem, hasParent, totalCount })

  // Only fetch if needed range isn't cached (unless `force` says the cache is stale)
  if (!force && isRangeCached(fetchStart, fetchEnd, cachedRange)) {
    return null // Already cached
  }

  const entries = await getFileRange(listingId, fetchStart, fetchEnd - fetchStart, includeHidden)

  // Prefetch icons for visible entries
  const iconIds = entries.map((e) => e.iconId).filter((id) => id)
  const useAppIcons = getUseAppIconsForDocuments()
  void prefetchIcons(iconIds, useAppIcons)

  // Detect + fetch custom-folder icons for the visible directory rows. The
  // backend defers the kHasCustomIcon getxattr off the bulk-listing hot path, so
  // we drive it here only for the bounded set of directories on screen. Plain
  // folders stay generic; packages already arrive as `pkg:` ids above.
  const visibleDirPaths = entries.filter((e) => e.isDirectory && !e.isSymlink).map((e) => e.path)
  void prefetchCustomFolderIcons(visibleDirPaths, useAppIcons)

  // Request sync status for visible paths
  const paths = entries.map((e) => e.path)
  onSyncStatusRequest?.(paths)

  return {
    entries,
    range: { start: fetchStart, end: fetchStart + entries.length },
  }
}

/**
 * Checks if cache props changed in a way that warrants a hard reset (wipe
 * cached entries and column widths, refetch from scratch).
 *
 * Hard resets are for cold context changes: navigation, hidden-files toggle,
 * sort, explicit refresh. `totalCount` changes alone (caused by `directory-diff`
 * events during bulk ops) trigger a *soft* refresh instead — the visible range
 * refetches in the background and atomically replaces, so the user never sees
 * an empty pane mid-burst.
 */
export function shouldResetCache(
  current: { listingId: string; includeHidden: boolean; cacheGeneration: number },
  previous: { listingId: string; includeHidden: boolean; cacheGeneration: number },
): boolean {
  return (
    current.listingId !== previous.listingId ||
    current.includeHidden !== previous.includeHidden ||
    current.cacheGeneration !== previous.cacheGeneration
  )
}

/**
 * Re-fetches icons for already-cached entries.
 * Called when the extension icon cache is cleared to refresh icons for visible files.
 */
export function refetchIconsForEntries(entries: FileEntry[]): void {
  if (entries.length === 0) return
  const iconIds = entries.map((e) => e.iconId).filter((id) => id)
  const useAppIcons = getUseAppIconsForDocuments()
  void prefetchIcons(iconIds, useAppIcons)
}

/**
 * Updates index size fields (recursiveSize, recursiveFileCount, recursiveDirCount)
 * in-place on cached entries. Only directory entries are queried.
 * Mutates entries directly so Svelte 5 fine-grained reactivity updates only affected DOM nodes.
 *
 * When `currentPath` is provided, it's included in the same batch IPC call and
 * its stats are returned so the caller can show the current folder's total on
 * the ".." row. Returns `null` if the current folder isn't indexed yet.
 */
export async function updateIndexSizesInPlace(
  cachedEntries: FileEntry[],
  currentPath?: string,
): Promise<DirStats | null> {
  // Collect directory paths and their indices in the array
  const dirIndices: number[] = []
  const dirPaths: string[] = []
  for (let i = 0; i < cachedEntries.length; i++) {
    if (cachedEntries[i].isDirectory) {
      dirIndices.push(i)
      dirPaths.push(cachedEntries[i].path)
    }
  }

  // Append currentPath as the last query so we can pick its stats off the end.
  const hasCurrent = currentPath !== undefined && currentPath !== ''
  if (hasCurrent) dirPaths.push(currentPath)

  if (dirPaths.length === 0) return null

  let stats: (DirStats | null)[]
  try {
    stats = await getDirStatsBatch(dirPaths)
  } catch {
    // Silently ignore -- indexing may not be initialized
    return null
  }

  for (let j = 0; j < dirIndices.length; j++) {
    const entry = cachedEntries[dirIndices[j]]
    const stat = stats[j]
    if (stat) {
      entry.recursiveSize = stat.recursiveSize
      entry.recursivePhysicalSize = stat.recursivePhysicalSize
      entry.recursiveFileCount = stat.recursiveFileCount
      entry.recursiveDirCount = stat.recursiveDirCount
    }
    // Update the hourglass flag every refresh, even when `stat` is null, so a
    // dir that has drained clears back to false instead of staying stuck-on
    // from a prior tick.
    entry.recursiveSizePending = stat?.recursiveSizePending ?? false
  }

  return hasCurrent ? (stats[stats.length - 1] ?? null) : null
}
