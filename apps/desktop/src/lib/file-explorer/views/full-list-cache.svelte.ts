/**
 * The prefetch buffer behind `FullList.svelte`: which entries the view currently
 * holds, and every rule for keeping them in step with the backend listing.
 *
 * The rows themselves live in the Rust `LISTING_CACHE`, never in Svelte state, so
 * what this owns is a WINDOW into it (the visible range plus a prefetch margin) and
 * the decision of when that window is stale. Three refresh flavours share it:
 *
 * - **Hard refresh** on a cold context change (nav, sort, hidden-files toggle): refetch
 *   from scratch, then atomically replace the old window. The caller gets `'reset'`
 *   back so it can force that fetch and suppress the width transition for one paint.
 * - **Soft refresh** when `totalCount` or `softRefreshTick` moves (`directory-diff`
 *   bursts, in-place renames): refetch in the background and swap atomically, so rows
 *   stay on screen and the pane never flickers empty mid-bulk-operation.
 * - **Static entries** (the search-results virtual volume): the host pane owns the array
 *   outright, so every backend path here goes inert.
 *
 * ❌ Each dep is its own GETTER, not one bag read whole. Every method reads only the
 * props it actually uses, which is what keeps the host's `$effect`s subscribed to
 * exactly what the original inline code tracked. Collapsing them into a single
 * `props()` call over-subscribes: the `..`-row stats would refetch on every
 * `directory-diff` tick, and the static-entries mirror would rewrite on any prop at all.
 */

import type { FileEntry } from '../types'
import { getDirStatsBatch } from '$lib/tauri-commands'
import { noteRenderedFolderSizes } from '$lib/indexing/first-size-timing'
import {
  createParentEntry,
  getEntryAt as getEntryAtUtil,
  indexOfEntry as indexOfEntryUtil,
  fetchVisibleRange as fetchVisibleRangeUtil,
  calculateFetchRange,
  isRangeCached,
  shouldResetCache,
  refetchIconsForEntries,
  updateIndexSizesInPlace,
  applyFolderSizes,
  type DirStats,
} from './file-list-utils'
import type { ListingIndexSizesChanged } from '$lib/tauri-commands'

/** Live reads of `FullList`'s props. One getter per prop; see the ❌ note above. */
export interface FullListCacheDeps {
  listingId: () => string
  /** The volume this listing is on, for the first-honest-size measurement. */
  volumeId: () => string
  totalCount: () => number
  includeHidden: () => boolean
  hasParent: () => boolean
  parentPath: () => string
  currentPath: () => string
  cacheGeneration: () => number
  softRefreshTick: () => number
  /**
   * Frontend-owned entries that replace the backend listing entirely
   * (search-results pane). `undefined` on a normal pane.
   */
  staticEntries: () => FileEntry[] | undefined
  onSyncStatusRequest: () => ((paths: string[]) => void) | undefined
  onIndexStatusRequest: () => ((paths: string[]) => void) | undefined
  onFolderCoverageRequest: () => ((folderPaths: string[]) => void) | undefined
}

/** Whether a window starts with the synthetic `..` row, and where that row points. */
interface ParentRow {
  hasParent: boolean
  parentPath: string
}

/** A row ready to render: the entry plus its UI index (`..` included when `hasParent`). */
export interface WindowRow {
  file: FileEntry
  globalIndex: number
}

/** The window `windowRows` and `fetch` operate on (UI indices, `endIndex` exclusive). */
export interface VisibleWindowRange {
  startIndex: number
  endIndex: number
}

/**
 * What `syncToProps` decided this pass. `'idle'` means "nothing to do, and don't
 * fetch either" (static-entries pane, no listing, or no measured container yet).
 */
export type CacheSync = 'reset' | 'refresh' | 'none' | 'idle'

export interface FullListCache {
  /** The cached slice of the listing. Reactive. */
  readonly entries: FileEntry[]
  /** Backend indices the slice covers, `end` exclusive. Reactive. */
  readonly range: { start: number; end: number }
  /** Recursive stats for the CURRENT directory, shown on the `..` row. Reactive. */
  readonly parentDirStats: DirStats | null
  /** The entry at a UI index, or `undefined` when it isn't in the window. */
  getEntryAt: (globalIndex: number) => FileEntry | undefined
  /** The UI index of a loaded row, or `undefined` when it isn't in the window. */
  indexOfEntry: (path: string) => number | undefined
  /** The rows to render for a virtual window, skipping indices not yet fetched. */
  windowRows: (args: VisibleWindowRange) => WindowRow[]
  /** Fetches the window's range. `force` skips the "already cached" short-circuit. */
  fetch: (args: VisibleWindowRange & { force?: boolean }) => Promise<void>
  /**
   * Re-runs the reset / soft-refresh decision against the current props. Pass
   * `ready: false` (no measured container yet) to read the props without acting,
   * which keeps the caller's effect subscribed without committing to a state.
   */
  syncToProps: (ready: boolean) => CacheSync
  /** Mirrors the `staticEntries` prop into the cache. No-op on a normal pane. */
  syncStaticEntries: () => void
  /** Refreshes index size fields on cached directories AND on the `..` row. */
  refreshIndexSizes: () => void
  /**
   * Applies a pushed `listing-index-sizes-changed` to the cached rows and the `..` row, with no
   * IPC. A `full` change (every row moved) re-reads instead, like `refreshIndexSizes`.
   */
  applyIndexSizes: (change: ListingIndexSizesChanged) => void
  /** Re-fetches icons for the cached entries (icon cache cleared). */
  refetchIcons: () => void
  /** Loads (or clears) the current folder's recursive stats for the `..` row. */
  syncParentDirStats: () => void
}

export function createFullListCache(deps: FullListCacheDeps): FullListCache {
  let entries = $state<FileEntry[]>([])
  let range = $state({ start: 0, end: 0 })
  let parentDirStats = $state<DirStats | null>(null)
  let isFetching = false
  let fetchEpoch = 0
  // The epoch `entries` was fetched in. After a cold context change the old rows stay
  // painted until the forced fetch lands, but they no longer match the indices, so
  // lookups must not hand them out as the entry under the cursor.
  let entriesEpoch = 0
  // The listing and `..` row `entries` were fetched under. Rows retained from another
  // listing paint under their own `..`, not the new one: the new `..` can be one of the
  // old rows (`/a` → `/a/b/c` makes it `/a/b`), a duplicate key in the keyed `#each`.
  let entriesListingId = ''
  let entriesParentRow: ParentRow = { hasParent: false, parentPath: '' }
  let queuedFetch: (VisibleWindowRange & { force?: boolean }) | null = null

  /** The cached rows that are safe to act on: none while retained rows await replacement. */
  function actionableEntries(): FileEntry[] {
    return entriesEpoch === fetchEpoch ? entries : []
  }

  function queueFetchIfBusy(args: VisibleWindowRange & { force?: boolean }): boolean {
    if (!isFetching) return false
    if (args.force) queuedFetch = args
    return true
  }

  // Previous prop values, so `syncToProps` can tell a cold context change from a
  // diff-driven one. Plain locals: they're bookkeeping, nothing renders them.
  let prevCacheProps = { listingId: '', includeHidden: false, cacheGeneration: 0 }
  let prevTotalCount = 0
  let prevSoftTick = 0

  async function fetch({
    startIndex,
    endIndex,
    force = false,
  }: VisibleWindowRange & { force?: boolean }): Promise<void> {
    // Static-entries branch (search-results pane): the array is already in
    // memory, no IPC needed. `syncStaticEntries` mirrors it into `entries`.
    if (deps.staticEntries() !== undefined) return
    const listingId = deps.listingId()
    if (!listingId || queueFetchIfBusy({ startIndex, endIndex, force })) return
    const capturedEpoch = fetchEpoch

    const hasParent = deps.hasParent()
    const parentPath = deps.parentPath()
    const totalCount = deps.totalCount()

    // Check if range is already cached BEFORE setting isFetching
    // This prevents blocking subsequent fetches when data is already available
    const { fetchStart, fetchEnd } = calculateFetchRange({
      startItem: startIndex,
      endItem: endIndex,
      hasParent,
      totalCount,
    })
    if (!force && isRangeCached(fetchStart, fetchEnd, range)) {
      return // Already cached
    }

    isFetching = true
    try {
      const result = await fetchVisibleRangeUtil({
        listingId,
        startItem: startIndex,
        endItem: endIndex,
        hasParent,
        totalCount,
        includeHidden: deps.includeHidden(),
        cachedRange: range,
        onSyncStatusRequest: deps.onSyncStatusRequest(),
        onIndexStatusRequest: deps.onIndexStatusRequest(),
        onFolderCoverageRequest: deps.onFolderCoverageRequest(),
        force,
      })
      if (result && capturedEpoch === fetchEpoch && listingId === deps.listingId()) {
        entries = result.entries
        range = result.range
        entriesEpoch = capturedEpoch
        entriesListingId = listingId
        entriesParentRow = { hasParent, parentPath }
        noteRenderedFolderSizes(entries, deps.volumeId())
      }
    } catch {
      // Never leave rows from another listing under the current breadcrumb.
      if (force && capturedEpoch === fetchEpoch && listingId === deps.listingId()) {
        entries = []
        range = { start: 0, end: 0 }
        entriesEpoch = capturedEpoch
      }
    } finally {
      isFetching = false
      const next = queuedFetch
      queuedFetch = null
      if (next) void fetch(next)
    }
  }

  /** Re-reads the index sizes of the cached directories AND the `..` row over IPC. */
  function refreshIndexSizesNow(): void {
    const hasParent = deps.hasParent()
    if (entries.length === 0 && !hasParent) return
    void updateIndexSizesInPlace(entries, hasParent ? deps.currentPath() : undefined).then((stats) => {
      parentDirStats = stats
      noteRenderedFolderSizes(entries, deps.volumeId())
    })
  }

  return {
    get entries() {
      return entries
    },
    get range() {
      return range
    },
    get parentDirStats() {
      return parentDirStats
    },

    getEntryAt: (globalIndex: number) =>
      getEntryAtUtil(
        globalIndex,
        deps.hasParent(),
        deps.parentPath(),
        actionableEntries(),
        range,
        parentDirStats ?? undefined,
      ),

    indexOfEntry: (path: string) => indexOfEntryUtil(path, deps.hasParent(), actionableEntries(), range),

    windowRows: ({ startIndex, endIndex }: VisibleWindowRange) => {
      // Read the live props either way, so the caller's `$derived` re-runs when they move.
      const live: ParentRow = { hasParent: deps.hasParent(), parentPath: deps.parentPath() }
      const retained = entries.length > 0 && entriesListingId !== deps.listingId()
      const { hasParent, parentPath } = retained ? entriesParentRow : live
      // Spread to read every element, so the caller's `$derived` re-runs on an
      // in-place entry mutation (index-size enrichment) and not only on a swap.
      const slice = [...entries]
      const rangeStart = range.start
      const rangeEnd = range.end

      const rows: WindowRow[] = []
      for (let i = startIndex; i < endIndex; i++) {
        let entry: FileEntry | undefined
        if (hasParent && i === 0) {
          entry = createParentEntry(parentPath, parentDirStats ?? undefined)
        } else {
          const backendIndex = hasParent ? i - 1 : i
          if (backendIndex >= rangeStart && backendIndex < rangeEnd) {
            entry = slice[backendIndex - rangeStart]
          }
        }
        if (entry) {
          rows.push({ file: entry, globalIndex: i })
        }
      }
      return rows
    },

    fetch,

    syncToProps: (ready: boolean) => {
      // Static-entries panes sync through `syncStaticEntries`; skip the cache /
      // diff machinery entirely so they never reach for a backend fetch.
      if (deps.staticEntries() !== undefined) return 'idle'
      const currentProps = {
        listingId: deps.listingId(),
        includeHidden: deps.includeHidden(),
        cacheGeneration: deps.cacheGeneration(),
      }
      const currentTotal = deps.totalCount()
      const currentTick = deps.softRefreshTick()
      if (!currentProps.listingId || !ready) return 'idle'

      if (shouldResetCache(currentProps, prevCacheProps)) {
        fetchEpoch++
        prevCacheProps = currentProps
        prevTotalCount = currentTotal
        prevSoftTick = currentTick
        return 'reset'
      }

      if (currentTotal !== prevTotalCount || currentTick !== prevSoftTick) {
        prevTotalCount = currentTotal
        prevSoftTick = currentTick
        return 'refresh'
      }

      return 'none'
    },

    syncStaticEntries: () => {
      const src = deps.staticEntries()
      if (src === undefined) return
      entries = src
      range = { start: 0, end: src.length }
      entriesEpoch = fetchEpoch
      entriesListingId = deps.listingId()
    },

    refreshIndexSizes: refreshIndexSizesNow,

    applyIndexSizes: (change: ListingIndexSizesChanged) => {
      if (change.full) {
        refreshIndexSizesNow()
        return
      }
      applyFolderSizes(entries, change.folders)
      if (change.currentDirChanged && deps.hasParent()) parentDirStats = change.currentDir
      noteRenderedFolderSizes(entries, deps.volumeId())
    },

    refetchIcons: () => {
      if (entries.length > 0) {
        refetchIconsForEntries(entries)
      }
    },

    syncParentDirStats: () => {
      const currentPath = deps.currentPath()
      // Static-entries panes have a synthetic "directory" with no path to stat,
      // and don't render a `..` row anyway.
      if (deps.staticEntries() !== undefined || !deps.hasParent() || !currentPath) {
        parentDirStats = null
        return
      }
      void getDirStatsBatch([currentPath])
        .then((results) => {
          parentDirStats = results[0] ?? null
        })
        .catch(() => {
          // Silently ignore -- indexing may not be initialized yet.
        })
    },
  }
}
