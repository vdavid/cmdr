import {
  getFileRange,
  updateLeftPaneState,
  updateRightPaneState,
  type PaneFileEntry,
  type PaneState,
} from '$lib/tauri-commands'
import { type CanonicalPath, parentOf } from '$lib/path/canonical'
import type { ViewMode } from '$lib/app-status-store'
import type { SearchResultEntry } from '$lib/ipc/bindings'
import type { SnapshotSort } from '$lib/search/snapshot-store.svelte'
import { getWalkedGround, isVolumeAggregating } from '$lib/indexing/index-state.svelte'
import { isPathAffectedByWalk } from '$lib/indexing/walked-ground'
import { isDirSizeUpdating } from '../views/full-list-utils'
import { snapshotMcpRows } from './snapshot-mcp-rows'

/** Whether one mirrored row's folder size can still move. */
type InFluxAnswer = (entry: { path: string; recursiveSizePending?: boolean | null }) => boolean

/**
 * The pane's "can this number still move" answer, resolved ONCE per push and
 * then asked per row — the same composition `FullList` renders its hourglass
 * from (`aggregating || under a walk || this dir's own pending writes`), so a
 * row marked `[size-unsettled]` in `cmdr://state` is exactly a row wearing the
 * hourglass on screen.
 *
 * Read outside a reactive context on purpose: a push is triggered (navigation,
 * selection, an `index-dir-updated` tick), not subscribed, so this wants the
 * current value, not a subscription. The index storm that moves these numbers
 * fires `index-dir-updated` throughout, which is what re-pushes.
 */
function inFluxAnswerFor(volumeId: string): InFluxAnswer {
  const ground = getWalkedGround(volumeId)
  const aggregating = isVolumeAggregating(volumeId)
  return (entry) =>
    isDirSizeUpdating(aggregating || isPathAffectedByWalk(ground, entry.path), entry.recursiveSizePending ?? false)
}

export interface PaneMcpSyncDeps {
  paneId: 'left' | 'right'
  /**
   * Whether this pane's kind mirrors to the MCP `PaneState` store
   * (`VolumeCapabilities.syncsToMcp`). `false` only for the network kind, whose
   * push `NetworkBrowser` owns (see `syncPaneStateToMcp`). FilePane supplies this
   * from its derived caps, so the gate reads the kind capability, not a
   * `getIsNetworkView()` derivation off a raw `volumeId ===` derived.
   */
  getSyncsToMcp: () => boolean
  getListingId: () => string
  /** Rows the BACKEND listing holds (no `..`). Zero on a pane with no listing. */
  getTotalCount: () => number
  /**
   * Rows the pane DISPLAYS, `..` included — FilePane's `effectiveTotalCount`. It
   * differs from `getTotalCount()` twice over: a pane with a parent row counts one
   * more, and a search-results pane counts its snapshot's entries while the backend
   * count sits at zero. This is what reaches `totalFiles`.
   */
  getRowCount: () => number
  /**
   * The search-results snapshot's entries when this pane is showing one, else
   * `null`. A snapshot pane has no listing to fetch rows from, so its rows come
   * from here instead (`snapshot-mcp-rows.ts`).
   */
  getSnapshotEntries: () => readonly SearchResultEntry[] | null
  /**
   * The order a search-results pane's rows are in, or `null` for the search
   * engine's ranked order. Meaningless on any other pane, which reads its sort
   * off `getSortBy` / `getSortOrder` instead.
   */
  getSnapshotSort: () => SnapshotSort | null
  getHasParent: () => boolean
  getVisibleRangeStart: () => number
  getVisibleRangeEnd: () => number
  getCanonicalPath: () => CanonicalPath | null
  getIncludeHidden: () => boolean
  getCurrentPath: () => string
  getVolumeId: () => string
  getVolumeName: () => string | undefined
  getCursorIndex: () => number
  getViewMode: () => ViewMode
  getSelectedIndices: () => number[]
  getSortBy: () => string
  getSortOrder: () => string
  getShowHiddenFiles: () => boolean
  getTypeToJump: () => {
    buffer: string
    indicatorVisible: boolean
    indicatorStale: boolean
  }
  getLastJumpMatchedName: () => string | null
}

/**
 * Mirrors a `FilePane`'s state into the MCP `PaneState` store so `cmdr://state`
 * reflects navigation, selection, and type-to-jump for MCP-driven tests/agents.
 *
 * Only the network pane is skipped: `NetworkBrowser` owns the MCP push for that
 * view, and FilePane's sync would clobber its host list.
 */
/**
 * How many rows of the visible range `cmdr://state` carries. A cap rather than
 * the whole listing: an agent reads what the user is looking at, and a pane
 * showing 74,000 files would otherwise serialize all of them on every sync.
 */
const MAX_MIRRORED_ROWS = 100

export function createPaneMcpSync(deps: PaneMcpSyncDeps) {
  // Map sort column names to MCP format (constant, no need to recreate)
  const sortFieldMap: Record<string, string> = {
    name: 'name',
    extension: 'ext',
    size: 'size',
    modified: 'modified',
    created: 'created',
  }

  /**
   * The `sort:` line `cmdr://state` renders for this pane, as an explicit
   * `field` / `order` pair.
   *
   * A SEARCH-RESULTS pane answers from its snapshot, never from the tab: the tab
   * still carries the sort of the folder the user came from, and reporting that
   * would tell an agent the rows are in an order they are not in. Its ranked
   * state reports `relevance:desc`, which is what a result set ranked
   * best-match-first is sorted by, rather than the `name:asc` a silent default
   * would have shown.
   *
   * ❗ Always explicit, never empty: the resource renderer substitutes
   * `name:asc` for an empty field, and a pane that pushed nothing there would be
   * described by that substitution instead of by itself.
   */
  function mcpSort(): { field: string; order: 'asc' | 'desc' } {
    if (deps.getSnapshotEntries() !== null) {
      const snapshotSort = deps.getSnapshotSort()
      if (!snapshotSort) return { field: 'relevance', order: 'desc' }
      return {
        field: sortFieldMap[snapshotSort.column] ?? 'name',
        order: snapshotSort.order === 'ascending' ? 'asc' : 'desc',
      }
    }
    return {
      field: sortFieldMap[deps.getSortBy()] ?? 'name',
      order: deps.getSortOrder() === 'ascending' ? 'asc' : 'desc',
    }
  }

  /**
   * Returns true when MCP shouldn't carry a BACKEND-listing file list for this
   * pane: the network pane (NetworkBrowser owns that push) or no listing yet. A
   * search-results pane also has no listing id, and its rows come from the
   * snapshot branch in `buildMcpFileList` before this is consulted. Extracted to
   * keep that function under the cyclomatic complexity cap.
   */
  function skipMcpFileSync(): boolean {
    return !deps.getSyncsToMcp() || !deps.getListingId() || deps.getTotalCount() === 0
  }

  /**
   * Map one listing entry onto its MCP mirror. Extracted from `buildMcpFileList`
   * for the same reason as `skipMcpFileSync`: every `?? null` coercion counts
   * toward that function's cyclomatic complexity cap.
   *
   * `PaneFileEntry` uses `null` for absent fields (post-Group-A wire format)
   * while `FileEntry` uses `undefined`, so `?? null` coerces across both.
   */
  function toMcpFileEntry(
    entry: Awaited<ReturnType<typeof getFileRange>>[number],
    inFlux: InFluxAnswer,
  ): PaneFileEntry {
    return {
      name: entry.name,
      path: entry.path,
      isDirectory: entry.isDirectory,
      size: entry.size ?? null,
      recursiveSize: entry.recursiveSize ?? null,
      // eslint-disable-next-line svelte/prefer-svelte-reactivity -- not reactive state, just formatting a timestamp
      modified: entry.modifiedAt != null ? new Date(entry.modifiedAt * 1000).toISOString() : null,
      // The SAME per-row answer the file list's hourglass renders from, ❌ never
      // the raw `recursiveSizePending` field alone: that covers only this dir's
      // own draining writes, and misses the walk that is rewriting it, which is
      // exactly when the number is furthest from the truth.
      recursiveSizeUpdating: inFlux(entry),
      // The honest-sizes pair, so `cmdr://state` renders the same `≥` lower
      // bound and staleness the file list shows instead of passing a partial
      // total off as a settled one.
      recursiveSizeComplete: entry.recursiveSizeComplete ?? null,
      recursiveSizeStale: entry.recursiveSizeStale ?? null,
      recursivePhysicalSize: entry.recursivePhysicalSize ?? null,
      // Finder tags (filled visible-range-first by enrich_tags). Surface the
      // same dots the UI shows as a `[tags:…]` marker in cmdr://state.
      tags: entry.tags ?? [],
    }
  }

  /** Build file list for MCP state sync */
  async function buildMcpFileList(): Promise<PaneFileEntry[]> {
    const files: PaneFileEntry[] = []
    // A search-results pane's rows are already in the frontend, so they never go
    // through the listing cache. Same visible-range window and same row cap.
    const snapshotEntries = deps.getSnapshotEntries()
    if (snapshotEntries) {
      return snapshotMcpRows(snapshotEntries, deps.getVisibleRangeStart(), deps.getVisibleRangeEnd(), MAX_MIRRORED_ROWS)
    }
    if (skipMcpFileSync()) return files

    const listingId = deps.getListingId()
    const hasParent = deps.getHasParent()
    const includeHidden = deps.getIncludeHidden()
    const totalCount = deps.getTotalCount()
    const visibleRangeStart = deps.getVisibleRangeStart()
    const visibleRangeEnd = deps.getVisibleRangeEnd()
    const canonicalPath = deps.getCanonicalPath()

    // Calculate backend indices from visible range (frontend indices include "..")
    const backendStart = hasParent ? Math.max(0, visibleRangeStart - 1) : visibleRangeStart
    const backendEnd = hasParent ? Math.max(0, visibleRangeEnd - 1) : visibleRangeEnd

    // Include ".." entry if it's in the visible range
    if (hasParent && visibleRangeStart === 0 && canonicalPath) {
      const parentPath = parentOf(canonicalPath)
      files.push({
        name: '..',
        path: parentPath,
        isDirectory: true,
        size: null,
        recursiveSize: null,
        modified: null,
        // The `..` row pushes no size, so there's nothing here that could move.
        recursiveSizeUpdating: null,
        recursiveSizeComplete: null,
        recursiveSizeStale: null,
        recursivePhysicalSize: null,
      })
    }

    // ONE call for the whole range, capped at `MAX_MIRRORED_ROWS`. A row at a
    // time was one IPC round trip each, and that is what wedged the app on a big
    // directory (`docs/notes/listing-row-fetch-quadratic-2026-08-22.md`).
    //
    // The count is also clamped to the cached `totalCount`, and a range that
    // comes back SHORT of it is the expected answer while a `directory-diff` is
    // mid-flight: the listing shrank under the count the frontend is iterating.
    // A short list is the right MCP state for that moment.
    const maxToFetch = Math.max(0, Math.min(backendEnd - backendStart, MAX_MIRRORED_ROWS, totalCount - backendStart))
    if (maxToFetch > 0) {
      const range = await getFileRange(listingId, backendStart, maxToFetch, includeHidden)
      const inFlux = inFluxAnswerFor(deps.getVolumeId())
      for (const entry of range) files.push(toMcpFileEntry(entry, inFlux))
    }
    return files
  }

  /**
   * Sync pane state to Rust for MCP context tools.
   * Called when files load, cursor position changes, or view mode changes.
   *
   * Skipped entirely on the Network virtual volume: `NetworkBrowser`
   * (mounted inside `NetworkMountView`) owns the pane-state push for that
   * view and writes the host list as `files`. Without this guard, FilePane's
   * own sync races NetworkBrowser's and overwrites it with stale local-pane
   * data (empty `files`, the old fixture `path`, and a leftover
   * `totalFiles`/`loadedRange`). That clobber is why three SMB tests
   * (`guest host shows share count`, `auth host shows share count`,
   * `50-share host shows correct share count`) used to time out at the 30s
   * pollUntil deadline — `cmdr://state` never contained the host entries
   * NetworkBrowser had just pushed.
   *
   * MTP volumes are not affected: their file list comes from a normal
   * `list_directory` against the volume, so FilePane's sync is the right
   * source of truth there.
   */
  async function syncPaneStateToMcp() {
    // The network skip folds into the `syncsToMcp` capability, supplied by
    // FilePane's derived caps. A search-results pane DOES sync: it is a real pane
    // an agent can move the cursor in and delete from, and while it pushed nothing
    // the store went on describing the directory the pane came from.
    if (!deps.getSyncsToMcp()) return
    try {
      const files = await buildMcpFileList()
      const hasParent = deps.getHasParent()
      const visibleRangeStart = deps.getVisibleRangeStart()
      const visibleRangeEnd = deps.getVisibleRangeEnd()
      const typeToJump = deps.getTypeToJump()
      // Rows on screen, `..` included. FilePane already folds the parent row and
      // the snapshot's own count into one derived, so this doesn't redo either.
      const effectiveTotal = deps.getRowCount()
      // Use actual visible range, clamped to valid bounds
      const loadedStart = Math.max(0, visibleRangeStart)
      const loadedEnd = Math.min(effectiveTotal, visibleRangeEnd)
      // Surface type-to-jump state so MCP-driven tests can assert it.
      // Only populated while a buffer or visible indicator exists:
      // keeps the YAML clean in the common case (no jump active).
      const typeToJumpInfo =
        typeToJump.indicatorVisible || typeToJump.buffer !== ''
          ? {
              buffer: typeToJump.buffer,
              indicatorVisible: typeToJump.indicatorVisible,
              indicatorStale: typeToJump.indicatorStale,
              lastMatchedName: deps.getLastJumpMatchedName(),
            }
          : null

      const sort = mcpSort()
      const state: PaneState = {
        path: deps.getCurrentPath(),
        volumeId: deps.getVolumeId(),
        // PaneState (typed binding) wants `string | null`; the local var is
        // `string | undefined`. Coerce to satisfy the IPC contract.
        volumeName: deps.getVolumeName() ?? null,
        files,
        cursorIndex: deps.getCursorIndex(),
        viewMode: deps.getViewMode(),
        selectedIndices: deps.getSelectedIndices(),
        sortField: sort.field,
        sortOrder: sort.order,
        totalFiles: effectiveTotal,
        // Says whether `totalFiles` counts a `..` row. The backend's empty-pane
        // gate subtracts it before deciding there's nothing to act on, so a
        // parentless pane holding one file isn't read as an empty folder.
        hasParentRow: hasParent,
        loadedStart,
        loadedEnd,
        showHidden: deps.getShowHiddenFiles(),
        typeToJump: typeToJumpInfo,
      }

      const updateFn = deps.paneId === 'left' ? updateLeftPaneState : updateRightPaneState
      await updateFn(state)
    } catch {
      // Silently ignore sync errors - MCP is optional
    }
  }

  return {
    skipMcpFileSync,
    buildMcpFileList,
    syncPaneStateToMcp,
  }
}
