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
 *
 * **Both image-index fetches are COALESCED, and that's load-bearing.** They're driven
 * by things that arrive in storms: every visible-range render, every listing swap, and
 * every enrichment tick. Uncoalesced, and with each call outlasting the enrich
 * debounce, they stacked one backend query per trigger; a burst of watcher-driven
 * refreshes during a large transfer then took the backend's whole blocking pool and
 * froze the panes and the volume picker until restart. One in flight per pane, newest
 * request wins. ❌ Don't call `mediaIndexFileStatus` / `mediaIndexFolderCoverage`
 * around these.
 *
 * The sync-status fetch deliberately isn't coalesced: the backend batches it, joins
 * concurrent requests for overlapping paths, and applies its own deadline, so the
 * coalescing already happens where it has the most information.
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
import { createCoalesced, createDebounce } from '$lib/utils/timing'

/** How often the idle poll re-reads the sync status of the visible paths. */
const SYNC_POLL_INTERVAL_MS = 3000

/**
 * Whether any visible row is a cloud file, and so whether the idle poll has
 * anything it could learn.
 *
 * `unknown` covers both "not a cloud file" and "the provider could not say", and
 * neither becomes a live cloud status without something the pane already reacts
 * to: a move re-lists, and a fresh listing re-fetches. One cloud file keeps the
 * whole folder polled, since its neighbours ride the same batch.
 */
function hasCloudFile(statuses: Record<string, SyncStatus>): boolean {
  return Object.values(statuses).some((status) => status !== 'unknown')
}
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
  const indexStatusFetch = createCoalesced(async (paths: string[]) => {
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
  })

  // The backend returns one entry per folder in request order.
  const folderCoverageFetch = createCoalesced(async (folderPaths: string[]) => {
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
  })

  function fetchIndexStatusForPaths(paths: string[]): Promise<void> {
    if (!fileStatusEnabled || paths.length === 0) return Promise.resolve()
    return indexStatusFetch.call(paths)
  }

  function fetchFolderCoverageForPaths(folderPaths: string[]): Promise<void> {
    if (!folderCoverageEnabled || folderPaths.length === 0) return Promise.resolve()
    return folderCoverageFetch.call(folderPaths)
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
        // A folder with no cloud files has nothing that can change: `unknown` is
        // what a plain local file reports, and it only moves if the file itself
        // does, which re-lists and re-fetches anyway. Polling one re-asked the
        // provider for every visible row every three seconds forever; on an idle
        // prod session that was two batches of 267 and 377 paths every 3 s, per
        // pane, for answers that could not move.
        if (!hasCloudFile(syncStatusMap)) return
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
      // A request queued behind an in-flight fetch would otherwise still fire for a
      // pane that no longer exists.
      indexStatusFetch.cancel()
      folderCoverageFetch.cancel()
    },
  }
}
