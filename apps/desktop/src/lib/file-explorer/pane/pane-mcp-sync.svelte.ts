import {
  getFileAt,
  updateLeftPaneState,
  updateRightPaneState,
  type PaneFileEntry,
  type PaneState,
} from '$lib/tauri-commands'
import { type CanonicalPath, parentOf } from '$lib/path/canonical'
import type { ViewMode } from '$lib/app-status-store'

export interface PaneMcpSyncDeps {
  paneId: 'left' | 'right'
  getIsNetworkView: () => boolean
  getIsSearchResultsView: () => boolean
  getListingId: () => string
  getTotalCount: () => number
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
 * Network and search-results panes are skipped: `NetworkBrowser` owns the MCP
 * push for the network view (FilePane's sync would clobber its host list), and
 * a search-results snapshot is local dialog state, not a directory agents query.
 */
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
   * Returns true when MCP shouldn't carry a file list for this pane. Either it's
   * a virtual-volume pane (network / search-results — the snapshot or NetworkBrowser
   * owns that pane state) or there's no listing yet. Extracted from `buildMcpFileList`
   * to keep that function under the cyclomatic complexity cap.
   */
  function skipMcpFileSync(): boolean {
    return (
      deps.getIsNetworkView() || deps.getIsSearchResultsView() || !deps.getListingId() || deps.getTotalCount() === 0
    )
  }

  /** Build file list for MCP state sync */
  async function buildMcpFileList(): Promise<PaneFileEntry[]> {
    const files: PaneFileEntry[] = []
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
      })
    }

    // Limit to 100 files max for performance
    const maxToFetch = Math.min(backendEnd - backendStart, 100)
    for (let i = 0; i < maxToFetch; i++) {
      const backendIndex = backendStart + i
      if (backendIndex >= totalCount) break
      const entry = await getFileAt(listingId, backendIndex, includeHidden)
      // Null means the listing on the BE has fewer entries than our cached
      // `totalCount` (a directory-diff is mid-flight). Stop here: keeps the
      // partial MCP state consistent and avoids trailing out-of-bounds calls
      // for the rest of the visible range.
      if (!entry) break
      files.push({
        name: entry.name,
        path: entry.path,
        isDirectory: entry.isDirectory,
        // PaneFileEntry uses `null` for absent fields (post-Group-A wire format).
        // FileEntry uses `undefined`, so coerce. `?? null` handles both.
        size: entry.size ?? null,
        recursiveSize: entry.recursiveSize ?? null,
        modified: entry.modifiedAt != null ? new Date(entry.modifiedAt * 1000).toISOString() : null,
        recursiveSizePending: entry.recursiveSizePending ?? null,
      })
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
    // Search-results panes don't sync to MCP either: the snapshot is local
    // dialog state, not a directory MCP agents are expected to query.
    if (deps.getIsNetworkView() || deps.getIsSearchResultsView()) return
    try {
      const files = await buildMcpFileList()
      const hasParent = deps.getHasParent()
      const totalCount = deps.getTotalCount()
      const visibleRangeStart = deps.getVisibleRangeStart()
      const visibleRangeEnd = deps.getVisibleRangeEnd()
      const typeToJump = deps.getTypeToJump()
      const effectiveTotal = hasParent ? totalCount + 1 : totalCount
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
