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
import { snapshotMcpRows } from './snapshot-mcp-rows'

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
  function toMcpFileEntry(entry: Awaited<ReturnType<typeof getFileRange>>[number]): PaneFileEntry {
    return {
      name: entry.name,
      path: entry.path,
      isDirectory: entry.isDirectory,
      size: entry.size ?? null,
      recursiveSize: entry.recursiveSize ?? null,
      // eslint-disable-next-line svelte/prefer-svelte-reactivity -- not reactive state, just formatting a timestamp
      modified: entry.modifiedAt != null ? new Date(entry.modifiedAt * 1000).toISOString() : null,
      recursiveSizePending: entry.recursiveSizePending ?? null,
      // The honest-sizes trio, so `cmdr://state` renders the same `≥` lower
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
        recursiveSizePending: null,
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
      for (const entry of range) files.push(toMcpFileEntry(entry))
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
        sortField: sortFieldMap[deps.getSortBy()] ?? 'name',
        sortOrder: deps.getSortOrder() === 'ascending' ? 'asc' : 'desc',
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
