/**
 * What the pane reports about its cursor and selection: the `FileEntry` under
 * the cursor and the listing's aggregate stats. Both feed `SelectionInfo` (the
 * pane footer), and `entryUnderCursor` additionally backs the pane API reads
 * (`getPathUnderCursor`, Quick Look's cursor follow, the MCP mirror, the rename
 * conflict dialog).
 *
 * The two travel together: they refetch on the same triggers (cursor move,
 * selection change, listing swap, watcher diff), they clear together on a
 * listing swap, and both go through timing wrappers so a held arrow key or a
 * range-select can't flood the backend with per-keystroke IPC. The virtual
 * scroll is fully synchronous and unaffected by either.
 *
 * On a search-results pane there's no backend listing to query, so both halves
 * come straight out of the in-memory snapshot: the cursor entry is mirrored from
 * the row under the cursor, and the stats are folded by `snapshot-stats.ts`.
 */

import { untrack } from 'svelte'
import { getFileAt, getListingStats, getSelectionEntries, type FolderSizes } from '#lib/tauri-commands/index.js'
import type { FileEntry, ListingStats } from '../types'
import type { CanonicalPath } from '#lib/path/canonical.js'
import type { SearchSnapshot } from '#lib/search/snapshot-store.svelte.js'
import { applyFolderSizes, updateIndexSizesInPlace } from '../views/file-list-utils'
import { createDebounce, createThrottle } from '#lib/utils/timing.js'
import { createParentEntry } from './parent-entry'
import { computeSnapshotStats } from './snapshot-stats'
import { readCursorRow, type CursorReadOptions } from './pressed-rows'

/** The status bar can lag a frame behind the cursor; the list itself never does. */
const CURSOR_FETCH_DEBOUNCE_MS = 16
/** Stats show a live count at a steady cadence rather than per selection toggle. */
const STATS_THROTTLE_MS = 150

export interface SelectionInfoFeedDeps {
  getListingId: () => string
  getLoading: () => boolean
  getTotalCount: () => number
  getCursorIndex: () => number
  /**
   * Reactive; changes whenever the pane's rows are swapped or reshuffled (quick
   * filter, sort, hidden files, diffs, a new listing). A row change can leave the
   * cursor index alone while a different file lands under it.
   */
  getRowRevision: () => number
  /** Whether the synthetic `..` row occupies index 0 (shifts backend indices by one). */
  getHasParent: () => boolean
  /** `currentPath` with `~` expanded, or null before the home dir resolves. */
  getCanonicalPath: () => CanonicalPath | null
  getIncludeHidden: () => boolean
  getIsSearchResultsView: () => boolean
  getSearchSnapshot: () => SearchSnapshot | undefined
  /** The pane's selected indices in insertion order, as the stats IPC wants them. */
  getSelectedIndices: () => number[]
  /** How many rows are selected. The stats refetch tracks this, not the indices. */
  getSelectionSize: () => number
  /** Push pane state to MCP (debounced by the caller) after a cursor move. */
  syncMcp: () => void
  /**
   * A read confirmed which row sits at `index`: same listing, cursor unmoved since
   * the read started. Feeds the history entry's remembered cursor
   * (`history-cursor-sync.svelte.ts`). Never fired for `..`.
   */
  onCursorRow?: (index: number, rowPath: string) => void
  /** The listing revision the pane's rows belong to (`pane-row-state.ts`), for the checked cursor read. */
  getLastSequence: () => number
}

export interface SelectionInfoFeed {
  /** The entry under the cursor, or null when there's nothing to report. */
  readonly entry: FileEntry | null
  /** Aggregate stats for the listing + current selection, or null. */
  readonly stats: ListingStats | null
  /** Refetch the entry under the cursor now. */
  fetchEntry: () => Promise<void>
  /**
   * The entry under the cursor for a command that ACTS on it, read against the
   * revision the pane applied (`pressed-rows.ts::readCursorRow`). `null` when
   * there's none, or when the rows never settled (announced unless `announce: false`).
   */
  readCursorEntry: (options?: CursorReadOptions) => Promise<FileEntry | null>
  /** Refetch the listing stats now. */
  fetchStats: () => Promise<void>
  /**
   * Applies pushed folder readings to the cursor entry when it's one of them, in place: no IPC,
   * and no new entry object for the effects keyed on it to re-run over.
   */
  applyFolderSizes: (folders: FolderSizes[]) => void
  /** Drop the entry (the listing loader calls this when a new listing starts). */
  clearEntry: () => void
  /** Cancel the pending debounce/throttle. Call from `onDestroy`. */
  cleanup: () => void
}

export function createSelectionInfoFeed(deps: SelectionInfoFeedDeps): SelectionInfoFeed {
  let entry = $state<FileEntry | null>(null)
  let stats = $state<ListingStats | null>(null)

  /**
   * ❗ Not `fetchEntry`'s plain `getFileAt`: the cursor index belongs to the revision
   * the pane applied, and a watcher insert reaches the backend first, so for a few
   * milliseconds that row number names the neighbor. Fine to DISPLAY, wrong to act on.
   */
  async function readCursorEntry(options: CursorReadOptions = {}): Promise<FileEntry | null> {
    if (deps.getHasParent() && deps.getCursorIndex() === 0) {
      const canonical = deps.getCanonicalPath()
      return canonical ? createParentEntry(canonical) : null
    }
    if (deps.getTotalCount() === 0) return null
    const pane = {
      getListingId: deps.getListingId,
      getCursorIndex: deps.getCursorIndex,
      hasParentEntry: deps.getHasParent,
      getLastSequence: deps.getLastSequence,
    }
    const read = await readCursorRow(
      pane,
      async ({ listingId, backendRows, expectedSequence }) =>
        (await getSelectionEntries(listingId, deps.getIncludeHidden(), backendRows, expectedSequence))[0] ?? null,
      options,
    )
    return read.kind === 'read' ? read.value : null
  }

  async function fetchEntry(): Promise<void> {
    const listingId = deps.getListingId()
    if (!listingId) {
      entry = null
      return
    }

    const hasParent = deps.getHasParent()
    const cursorIndex = deps.getCursorIndex()

    // Handle ".." entry specially
    if (hasParent && cursorIndex === 0) {
      const canonical = deps.getCanonicalPath()
      entry = canonical ? createParentEntry(canonical) : null
      return
    }

    // Empty listing at a volume root (no ".." synthetic entry, no real entries):
    // calling getFileAt(0) here would log a spurious FE/BE index-mismatch error.
    if (deps.getTotalCount() === 0) {
      entry = null
      return
    }

    // Adjust index for ".." entry
    const backendIndex = hasParent ? cursorIndex - 1 : cursorIndex

    try {
      entry = await getFileAt(listingId, backendIndex, deps.getIncludeHidden())
    } catch {
      entry = null
    }
    if (entry && deps.getListingId() === listingId && deps.getCursorIndex() === cursorIndex) {
      deps.onCursorRow?.(cursorIndex, entry.path)
    }

    // Overlay the per-folder `recursiveSizePending` flag (and refresh the
    // recursive size) onto the cursor entry. It lives only on `DirStats`, not
    // on `get_file_range`, so SelectionInfo's Brief readout couldn't show the
    // "size updating" hourglass without this. Reuses the same enrichment the
    // list rows get; no-op for files. Fire-and-forget (mutates in place, so
    // Svelte reactivity updates SelectionInfo); pushed updates then land through
    // `applyFolderSizes` (`listing-index-sizes-changed`). Skips "..", whose entry path is the *parent*
    // folder, so enriching it would fetch the wrong folder's stats.
    if (entry?.isDirectory && entry.name !== '..') {
      void updateIndexSizesInPlace([entry])
    }
  }

  async function fetchStats(): Promise<void> {
    // A snapshot pane's rows are already in memory, so the same numbers come from
    // a fold instead of an IPC. Synchronous: the `await` below never runs here.
    if (deps.getIsSearchResultsView()) {
      const snap = deps.getSearchSnapshot()
      stats = snap ? computeSnapshotStats(snap.entries, deps.getSelectedIndices()) : null
      return
    }

    const listingId = deps.getListingId()
    if (!listingId) {
      stats = null
      return
    }

    try {
      // Convert selected indices to backend indices (adjust for ".." entry)
      const hasParent = deps.getHasParent()
      const selected = deps.getSelectedIndices()
      const backendIndices = selected.length > 0 ? selected.map((i) => (hasParent ? i - 1 : i)) : undefined

      stats = await getListingStats(listingId, deps.getIncludeHidden(), backendIndices)
    } catch {
      stats = null
    }
  }

  const debouncedFetchEntry = createDebounce(() => void fetchEntry(), CURSOR_FETCH_DEBOUNCE_MS)
  const throttledFetchStats = createThrottle(() => void fetchStats(), STATS_THROTTLE_MS)

  // Re-fetch the entry under the cursor when the cursor moves or the rows change
  // under it. Also sync to MCP so `cmdr://state` reflects keyboard nav (arrows,
  // Insert, PageUp/Down, Home/End, click-to-position), not only listing changes and
  // visible-range scrolls.
  $effect(() => {
    void deps.getCursorIndex() // Track
    void deps.getRowRevision() // Track
    if (deps.getListingId() && !deps.getLoading()) {
      debouncedFetchEntry.call()
      deps.syncMcp()
    }
  })

  /**
   * Search-results pane: mirror the snapshot row under the cursor into `entry`
   * so SelectionInfo and the other consumers see a real `FileEntry`. The cursor
   * index changes via FilePane's keyboard handler and the snapshot itself is
   * immutable, so the read here is cheap and synchronous. No-op for non-search
   * panes; the effect above handles those.
   */
  $effect(() => {
    if (!deps.getIsSearchResultsView()) return
    const snap = deps.getSearchSnapshot()
    if (!snap) {
      entry = null
      return
    }
    // TS doesn't model array bounds (no `noUncheckedIndexedAccess`), but the
    // cursor can briefly point past the snapshot's entries after a delete-
    // sync mutation. Keep the guard at runtime.

    const cursorIndex = deps.getCursorIndex()
    const e = snap.entries[cursorIndex]
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime bounds guard; cursor can point past entries after delete-sync (see comment above)
    if (!e) {
      entry = null
      return
    }
    untrack(() => deps.onCursorRow?.(cursorIndex, e.path))
    entry = {
      name: e.name,
      path: e.path,
      isDirectory: e.isDirectory,
      isSymlink: false,
      size: e.size ?? undefined,
      modifiedAt: e.modifiedAt ?? undefined,
      permissions: 0o644,
      owner: '',
      group: '',
      iconId: e.iconId,
      extendedMetadataLoaded: true,
      parentPath: e.parentPath,
    }
  })

  // Re-fetch listing stats when the selection changes.
  $effect(() => {
    void deps.getSelectionSize() // Track selection changes
    if (deps.getIsSearchResultsView()) return // The effect below owns a snapshot pane's stats.
    if (deps.getListingId() && !deps.getLoading()) {
      throttledFetchStats.call()
    }
  })

  /**
   * Search-results pane: fold the stats out of the snapshot, unthrottled. There's
   * no IPC to spare here, and the snapshot is also the thing that CHANGES (a
   * still-running walk appends rows, a delete-sync purges them), so the read of
   * `getSearchSnapshot()` is what keeps the count honest while the walk fills.
   */
  $effect(() => {
    if (!deps.getIsSearchResultsView()) return
    void deps.getSearchSnapshot() // Track appends and purges
    void deps.getSelectionSize() // Track selection changes
    void fetchStats()
  })

  return {
    get entry() {
      return entry
    },
    get stats() {
      return stats
    },
    fetchEntry,
    readCursorEntry,
    fetchStats,
    applyFolderSizes: (folders: FolderSizes[]) => {
      // `..` carries the PARENT folder's path, so it never matches a child reading.
      if (entry?.isDirectory && entry.name !== '..') applyFolderSizes([entry], folders)
    },
    clearEntry: () => {
      entry = null
    },
    cleanup: () => {
      debouncedFetchEntry.cancel()
      throttledFetchStats.cancel()
    },
  }
}
