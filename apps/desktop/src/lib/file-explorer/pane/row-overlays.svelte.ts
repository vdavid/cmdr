/**
 * The three per-row status overlays a file pane paints on top of its entries:
 * the cloud sync badge (iCloud / Dropbox), the per-file image-index badge, and
 * the per-folder image-index coverage badge.
 *
 * All three share one shape: the List components hand up the paths they just
 * rendered, we fetch a status for each, and the resulting map goes back down as
 * a prop. So they share one owner: the three maps, their fetchers, the gates
 * that decide whether a fetch happens at all, the idle sync poll, the
 * enrichment-driven refresh, and every timer/listener they need.
 *
 * Gates (both re-derive live, so a Settings toggle applies without a restart):
 * - The FILE badge needs image indexing on, the file-badge setting on, AND a
 *   local pane (index paths are OS paths; an archive / MTP / virtual pane's
 *   paths could never match the index).
 * - The FOLDER coverage badge needs image indexing on and a local pane, but
 *   deliberately NOT the file-badge setting: folder overlays (and the drive
 *   dot) are inherently sparse and always on.
 *
 * When a gate goes off, the matching map is cleared right away so stale badges
 * can't linger; turning it back on repopulates on the next visible-range fetch,
 * navigation, or enrich tick.
 */

import {
  getSyncStatus,
  mediaIndexFileStatus,
  mediaIndexFolderCoverage,
  onMediaEnrichProgress,
  onMediaEnrichTerminal,
  type FileIndexState,
  type FolderCoverage,
  type UnlistenFn,
} from '$lib/tauri-commands'
import type { SyncStatus } from '../types'
import { getMediaIndexEnabled, getMediaIndexShowFileStatusIcons } from '$lib/settings/reactive-settings.svelte'
import { createDebounce } from '$lib/utils/timing'

/** How often the idle poll re-reads the sync status of the visible paths. */
const SYNC_POLL_INTERVAL_MS = 3000
/** Delay before the single retry a timed-out sync-status fetch schedules. */
const SYNC_RETRY_DELAY_MS = 5000
/**
 * Debounce on the enrich-progress refresh. `media-enrich-progress` is already
 * throttled backend-side; badges only need to catch up, not track every tick.
 */
const ENRICH_REFRESH_DEBOUNCE_MS = 400

export interface RowOverlaysDeps {
  /** The pane's volume id, scoping every image-index query (reactive read). */
  getVolumeId: () => string
  /** The pane's listing id, or `''` when it has none yet (reactive read). */
  getListingId: () => string
  /** True when the pane's volume kind is `local` (reactive read off `caps`). */
  getIsLocalPane: () => boolean
}

export interface RowOverlays {
  /** Cloud sync status per visible path. */
  readonly syncStatusMap: Record<string, SyncStatus>
  /** Image-index state per visible file path. */
  readonly indexStatusMap: Record<string, FileIndexState>
  /** Image-index coverage per visible folder path. */
  readonly folderCoverageMap: Record<string, FolderCoverage>
  /** Fetch cloud sync status for the paths a List component just rendered. */
  fetchSyncStatusForPaths: (paths: string[]) => Promise<void>
  /** Fetch image-index state for the file paths a List component just rendered. */
  fetchIndexStatusForPaths: (paths: string[]) => Promise<void>
  /** Fetch image-index coverage for the folder paths a List component just rendered. */
  fetchFolderCoverageForPaths: (folderPaths: string[]) => Promise<void>
  /** Drop the sync map (the listing loader calls this when a new listing starts). */
  clearSyncStatusMap: () => void
  /** Drop the file-badge map (same listing-swap call site). */
  clearIndexStatusMap: () => void
  /** Drop the folder-coverage map (same listing-swap call site). */
  clearFolderCoverageMap: () => void
  /** Cancel a pending sync-status retry (same listing-swap call site). */
  clearSyncRetryTimer: () => void
  /** Start the idle sync poll + the enrichment listeners. Call from `onMount`. */
  start: () => void
  /** Stop every timer and listener this owns. Call from `onDestroy`. */
  cleanup: () => void
}

export function createRowOverlays(deps: RowOverlaysDeps): RowOverlays {
  let syncStatusMap = $state<Record<string, SyncStatus>>({})
  let indexStatusMap = $state<Record<string, FileIndexState>>({})
  let folderCoverageMap = $state<Record<string, FolderCoverage>>({})

  const fileStatusEnabled = $derived(
    deps.getIsLocalPane() && getMediaIndexEnabled() && getMediaIndexShowFileStatusIcons(),
  )
  const folderCoverageEnabled = $derived(deps.getIsLocalPane() && getMediaIndexEnabled())

  // Pending retry timer for timed-out sync-status fetches (max 1 retry).
  let syncRetryTimer: ReturnType<typeof setTimeout> | undefined
  let syncPollInterval: ReturnType<typeof setInterval> | undefined
  const enrichUnlisten: UnlistenFn[] = []

  async function fetchSyncStatusForPaths(paths: string[]): Promise<void> {
    if (paths.length === 0) return

    // Cancel any pending retry: a new fetch supersedes it
    clearTimeout(syncRetryTimer)
    syncRetryTimer = undefined

    try {
      const { data: statuses, timedOut } = await getSyncStatus(paths)
      syncStatusMap = { ...syncStatusMap, ...statuses }

      if (timedOut) {
        // Schedule a single retry after a short delay
        syncRetryTimer = setTimeout(() => {
          syncRetryTimer = undefined
          void getSyncStatus(paths)
            .then(({ data: retryStatuses }) => {
              syncStatusMap = { ...syncStatusMap, ...retryStatuses }
            })
            .catch(() => {
              // Give up silently on retry failure
            })
        }, SYNC_RETRY_DELAY_MS)
      }
    } catch {
      // Silently ignore - sync status is optional
    }
  }

  // The backend returns one entry per path in request order.
  async function fetchIndexStatusForPaths(paths: string[]): Promise<void> {
    if (!fileStatusEnabled || paths.length === 0) return
    try {
      const statuses = await mediaIndexFileStatus(deps.getVolumeId(), paths)
      const next: Record<string, FileIndexState> = { ...indexStatusMap }
      for (const status of statuses) {
        next[status.path] = status.state
      }
      indexStatusMap = next
    } catch {
      // Silently ignore - the image-index overlay is optional.
    }
  }

  // The backend returns one entry per folder in request order.
  async function fetchFolderCoverageForPaths(folderPaths: string[]): Promise<void> {
    if (!folderCoverageEnabled || folderPaths.length === 0) return
    try {
      const coverages = await mediaIndexFolderCoverage(deps.getVolumeId(), folderPaths)
      const next: Record<string, FolderCoverage> = { ...folderCoverageMap }
      for (const coverage of coverages) {
        next[coverage.path] = coverage
      }
      folderCoverageMap = next
    } catch {
      // Silently ignore - the folder-coverage overlay is optional.
    }
  }

  /** Re-query the paths already in the maps (the visible set), like the sync poll does. */
  function refreshKnownIndexPaths(): void {
    const paths = Object.keys(indexStatusMap)
    if (paths.length > 0) void fetchIndexStatusForPaths(paths)
    const folderPaths = Object.keys(folderCoverageMap)
    if (folderPaths.length > 0) void fetchFolderCoverageForPaths(folderPaths)
  }

  const debouncedRefreshIndexStatus = createDebounce(refreshKnownIndexPaths, ENRICH_REFRESH_DEBOUNCE_MS)

  $effect(() => {
    if (!fileStatusEnabled && Object.keys(indexStatusMap).length > 0) {
      indexStatusMap = {}
    }
  })

  $effect(() => {
    if (!folderCoverageEnabled && Object.keys(folderCoverageMap).length > 0) {
      folderCoverageMap = {}
    }
  })

  return {
    get syncStatusMap() {
      return syncStatusMap
    },
    get indexStatusMap() {
      return indexStatusMap
    },
    get folderCoverageMap() {
      return folderCoverageMap
    },
    fetchSyncStatusForPaths,
    fetchIndexStatusForPaths,
    fetchFolderCoverageForPaths,
    clearSyncStatusMap: () => {
      syncStatusMap = {}
    },
    clearIndexStatusMap: () => {
      indexStatusMap = {}
    },
    clearFolderCoverageMap: () => {
      folderCoverageMap = {}
    },
    clearSyncRetryTimer: () => {
      clearTimeout(syncRetryTimer)
      syncRetryTimer = undefined
    },
    start: () => {
      // Poll sync status so iCloud/Dropbox icons update while idle
      syncPollInterval = setInterval(() => {
        const paths = Object.keys(syncStatusMap)
        if (!deps.getListingId() || paths.length === 0) return
        void fetchSyncStatusForPaths(paths)
      }, SYNC_POLL_INTERVAL_MS)

      // Refresh the image-index overlays when THIS volume enriches (event-driven, not
      // polled). The terminal event refreshes immediately so the last images flip to
      // `indexed` without waiting out the progress debounce.
      void onMediaEnrichProgress((payload) => {
        if (payload.volumeId === deps.getVolumeId()) debouncedRefreshIndexStatus.call()
      }).then((unlisten) => enrichUnlisten.push(unlisten))
      void onMediaEnrichTerminal((payload) => {
        if (payload.volumeId !== deps.getVolumeId()) return
        refreshKnownIndexPaths()
      }).then((unlisten) => enrichUnlisten.push(unlisten))
    },
    cleanup: () => {
      clearInterval(syncPollInterval)
      clearTimeout(syncRetryTimer)
      syncRetryTimer = undefined
      for (const unlisten of enrichUnlisten) unlisten()
      enrichUnlisten.length = 0
      debouncedRefreshIndexStatus.cancel()
    },
  }
}
