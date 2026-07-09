/**
 * The listing loader for a file pane: the streaming directory-load pipeline plus
 * the per-pane generation + listingId token model that keeps a foreign (stale)
 * listing from landing in the wrong pane or overwriting a newer navigation.
 *
 * Lifted out of `FilePane.svelte` as the last (deliberately deferred, highest-
 * risk) cluster. Behavior-preserving: the six streaming listeners, the
 * `pendingLoad` promise machinery, and the reset semantics are moved verbatim.
 *
 * Ownership (the surgical / getter-setter idiom, like `type-to-jump-controller`
 * owns its buffer but injects `cursorIndex`): the loader OWNS the orchestration
 * and the staleness machinery — the `loadGeneration` counter (its only two bump
 * sites, `loadDirectory` and `adoptListing`, both live here), `isDestroyed`, the
 * active listing's `loadedPath`, the six `unlisten*` handles, and the
 * `pendingLoad` resolver/rejecter. The pane's lifecycle `$state`
 * (`loading` / `listingId` / `totalCount` / `error` / `friendlyError` /
 * `openingFolder` / `loadingCount` / `finalizingCount` / `volumeRootFromEvent` /
 * `lastSequence`) STAYS in `FilePane` (~60 non-loader read sites — selection,
 * stats, menu, MCP sync, markup, five sub-factory dep getters) and is read/
 * written through injected accessors.
 *
 * The staleness guard: every streaming listener's SYNCHRONOUS entry checks
 * `isEventForCurrentLoad(payload.listingId, captured, loadGeneration)`. Two async
 * tails (the `onListingError` `pathExistsChecked` continuation and
 * `handleListingComplete`'s post-`await findFileIndex` cursor write) run UNGUARDED
 * — this is current behavior and is preserved deliberately; do NOT add re-guards.
 */
import { tick } from 'svelte'
import type { FriendlyError } from '../types'
import type { SwapState } from './types'
import {
  cancelListing,
  findFileIndex,
  pathExistsChecked,
  listDirectoryEnd,
  listDirectoryStart,
  onListingOpening,
  onListingProgress,
  onListingReadComplete,
  onListingComplete,
  onListingError,
  onListingCancelled,
  trackEvent,
  type ListingCompleteEvent,
  type UnlistenFn,
} from '$lib/tauri-commands'
import { sweepListingTags } from './tag-sweep'
import { resolveValidPath } from '../navigation/path-resolution'
import { renderListingError } from '$lib/errors/listing-error'
import { evictPerPathIconsForDir } from '$lib/icon-cache'
import { cancelClickToRename } from '../rename/rename-activation'
import { dismissTransientToastsForPane } from '$lib/ui/toast'
import { getAppLogger } from '$lib/logging/logger'
import { getSetting } from '$lib/settings'
import type { DirectorySortMode } from '$lib/settings'
import type { SortColumn, SortOrder } from '../types'
import { basenameOf, type CanonicalPath, parentOf } from '$lib/path/canonical'
import type { ListViewAPI } from './types'
import type { VolumeCapabilities } from './volume-capabilities'
import * as benchmark from '$lib/benchmark'
import { isEventForCurrentLoad } from './listing-token'

const log = getAppLogger('fileExplorer')

export interface ListingLoaderDeps {
  paneId: 'left' | 'right'

  // Reactive reads (props + deriveds).
  getVolumeId: () => string
  getVolumePath: () => string
  getCurrentPath: () => string
  setCurrentPath: (path: string) => void
  getCanonicalPath: () => CanonicalPath | null
  getIncludeHidden: () => boolean
  getSortBy: () => SortColumn
  getSortOrder: () => SortOrder
  getDirectorySortMode: () => DirectorySortMode
  getCaps: () => VolumeCapabilities
  getHasParent: () => boolean
  getIsMtpView: () => boolean
  getViewMode: () => string
  getBriefListRef: () => ListViewAPI | undefined
  getFullListRef: () => ListViewAPI | undefined

  // Pane lifecycle `$state` — stays in FilePane, read/written through here.
  getListingId: () => string
  setListingId: (id: string) => void
  getLoading: () => boolean
  setLoading: (loading: boolean) => void
  getTotalCount: () => number
  setTotalCount: (count: number) => void
  getLastSequence: () => number
  setLastSequence: (sequence: number) => void
  setError: (error: string | null) => void
  setFriendlyError: (friendly: FriendlyError | null) => void
  setOpeningFolder: (opening: boolean) => void
  setLoadingCount: (count: number | undefined) => void
  setFinalizingCount: (count: number | undefined) => void
  setVolumeRootFromEvent: (root: string | undefined) => void

  // Shared FilePane state the loader pokes (RAW setters — NOT the FilePaneAPI
  // `setCursorIndex`, which scrolls / ticks / syncs MCP; the loader does its own).
  getCursorIndex: () => number
  setCursorIndexRaw: (index: number) => void
  clearEntryUnderCursor: () => void
  clearSyncStatusMap: () => void
  clearSyncRetryTimer: () => void
  bumpCacheGeneration: () => void

  // Collaborators created in FilePane.
  selection: {
    clearSelection: () => void
    getSelectedIndices: () => number[]
    setSelectedIndices: (indices: number[]) => void
  }
  renameCancel: () => void
  jumpClear: () => void
  syncMcp: () => void
  fetchEntryUnderCursor: () => void
  fetchListingStats: () => void

  // Callbacks bubbled to the parent.
  onPathChange?: (path: string) => void
  onVolumeChange?: (volumeId: string, volumePath: string, targetPath: string) => void
  onMtpFatalError?: (error: string) => void
  onCancelLoading?: (cancelledPath: string, selectName?: string) => void
  /**
   * A header-encrypted archive (a `-mhe=on` 7z) needs its password even to LIST
   * it: the whole metadata is encrypted. Fired instead of leaving only the
   * fallback error pane, so the parent can raise the browse-time password prompt.
   * `retry` re-lists the SAME directory (after the password is stored the re-list
   * succeeds); `wrongAttempt` swaps the prompt copy after a rejected password.
   */
  onArchiveNeedsPassword?: (info: {
    volumeId: string
    archivePath: string
    wrongAttempt: boolean
    retry: () => void
  }) => void
}

export interface ListingLoader {
  loadDirectory: (path: string, selectName?: string) => Promise<void>
  navigateToParent: () => Promise<boolean>
  navigateToPath: (path: string, selectName?: string) => Promise<void>
  navigateToFallback: (validPath: string | null) => void
  handleCancelLoading: () => void
  whenLoadSettles: () => Promise<void>
  resetLoadingState: (errorMessage?: string, preserveTotalCount?: boolean, friendly?: FriendlyError | null) => void
  getSwapState: () => SwapState
  adoptListing: (state: SwapState) => void
  /** Full listing teardown for the owning component's `onDestroy`. */
  cleanup: () => void
}

export function createListingLoader(deps: ListingLoaderDeps): ListingLoader {
  // Track the current load operation to cancel outdated ones.
  let loadGeneration = 0
  // Set on unmount so the background Finder-tag sweep stops (it's a detached
  // async loop the listing-cancel machinery doesn't reach).
  let isDestroyed = false
  // The directory path of the currently active listing. Plain bookkeeping (not
  // reactive): used to evict this directory's per-path icons (`path:*` / `pkg:*`)
  // when the listing ends, so a folder re-iconed while away is re-detected next
  // time it's shown rather than served stale from the session cache.
  let loadedPath = ''
  // Streaming event listeners.
  let unlistenOpening: UnlistenFn | undefined
  let unlistenProgress: UnlistenFn | undefined
  let unlistenComplete: UnlistenFn | undefined
  let unlistenError: UnlistenFn | undefined
  let unlistenCancelled: UnlistenFn | undefined
  let unlistenReadComplete: UnlistenFn | undefined

  // Pending load completion resolver: used by navigateToPath to signal when the
  // listing is done. Set at the start of loadDirectory, resolved by
  // handleListingComplete / error / cancel handlers.
  let pendingLoadResolve: (() => void) | null = null
  let pendingLoadReject: ((reason: string) => void) | null = null

  function resolvePendingLoad() {
    pendingLoadResolve?.()
    pendingLoadResolve = null
    pendingLoadReject = null
  }

  function rejectPendingLoad(reason: string) {
    pendingLoadReject?.(reason)
    pendingLoadResolve = null
    pendingLoadReject = null
  }

  function resetLoadingState(errorMessage?: string, preserveTotalCount = false, friendly?: FriendlyError | null) {
    if (errorMessage) deps.setError(errorMessage)
    deps.setFriendlyError(friendly ?? null)
    deps.setListingId('')
    if (!preserveTotalCount) deps.setTotalCount(0)
    deps.setLoading(false)
    deps.setOpeningFolder(false)
    deps.setLoadingCount(undefined)
    deps.setFinalizingCount(undefined)
    // Reject pending load promise on error/cancel
    if (errorMessage) {
      rejectPendingLoad(errorMessage)
    } else {
      rejectPendingLoad('Loading cancelled')
    }
  }

  /**
   * Navigates to a fallback path after the current path became invalid.
   * If the resolved path is outside the current volume (~ or /), switches
   * to the root volume instead of trying to list it on a non-root volume.
   */
  function navigateToFallback(validPath: string | null) {
    const target = validPath ?? '~'
    const isOutsideVolume = deps.getVolumeId() !== 'root' && (target === '~' || target === '/')
    if (isOutsideVolume && deps.onVolumeChange) {
      // The volume root was unreachable: switch to the root volume
      log.info('Volume root unreachable, switching to root volume with path: {target}', { target })
      deps.onVolumeChange('root', '/', target)
    } else {
      deps.setCurrentPath(target)
      void loadDirectory(target)
    }
  }

  async function loadDirectory(path: string, selectName?: string) {
    // Cancel any active rename when navigating
    deps.renameCancel()
    cancelClickToRename()
    // Clear only THIS pane's stale transient feedback. App-global toasts (updater,
    // transfer, downloads, indexing) and the other pane's toasts survive, so a
    // background navigation (e.g. an SMB reconnect retry) can't wipe them.
    dismissTransientToastsForPane(deps.paneId)
    // Directory change invalidates in-flight type-to-jump buffer (per plan § 6).
    deps.jumpClear()

    // Reset benchmark epoch for this navigation
    benchmark.resetEpoch()
    benchmark.logEventValue('loadDirectory CALLED', path)

    const volumeId = deps.getVolumeId()
    const listingId = deps.getListingId()

    // Debug logging for diagnosing concurrent list_directory calls
    log.debug(
      '[FilePane] loadDirectory called: paneId={paneId}, volumeId={volumeId}, path={path}, selectName={selectName}, currentLoading={loading}, currentListingId={listingId}',
      { paneId: deps.paneId, volumeId, path, selectName: selectName ?? 'none', loading: deps.getLoading(), listingId },
    )

    // Reject any pending load from a previous navigation
    rejectPendingLoad('Superseded by new navigation')

    // Increment generation to cancel any in-flight requests
    const thisGeneration = ++loadGeneration
    log.debug('[FilePane] loadDirectory: generation={generation}', { generation: thisGeneration })

    // Cancel any abandoned listing from previous navigation
    if (listingId) {
      log.debug('[FilePane] loadDirectory: cancelling previous listing {listingId}', { listingId })
      void cancelListing(listingId)
      void listDirectoryEnd(listingId)
      // Evict the closed directory's per-path icons (no longer visible).
      evictPerPathIconsForDir(loadedPath)
      deps.setListingId('')
      loadedPath = ''
      deps.setLastSequence(0)
    }

    // Clean up previous event listeners
    unlistenOpening?.()
    unlistenProgress?.()
    unlistenReadComplete?.()
    unlistenComplete?.()
    unlistenError?.()
    unlistenCancelled?.()

    // Set loading state BEFORE starting IPC call
    // This ensures the UI shows the loading spinner immediately
    deps.setLoading(true)
    deps.setOpeningFolder(false)
    deps.setLoadingCount(undefined)
    deps.setFinalizingCount(undefined)
    deps.setError(null)
    deps.setFriendlyError(null)
    deps.clearSyncStatusMap()
    deps.clearSyncRetryTimer()
    deps.selection.clearSelection()
    deps.setTotalCount(0) // Reset to show empty list immediately
    deps.clearEntryUnderCursor() // Clear old under-the-cursor entry info

    // Store path and selectName for use in event handlers
    const loadPath = path
    const loadSelectName = selectName

    // Loading state is set synchronously above; Svelte will render it on the next
    // microtask. The IPC call below is non-blocking (spawns a background task and
    // returns immediately), so no double-RAF paint wait is needed.
    await tick()

    const includeHidden = deps.getIncludeHidden()

    try {
      // Generate listingId first and set up listeners BEFORE starting the streaming
      // This prevents a race condition where fast folders complete before listeners are ready
      const newListingId = crypto.randomUUID()
      deps.setListingId(newListingId)
      loadedPath = path
      deps.setLastSequence(0)
      const captured = { listingId: newListingId, generation: thisGeneration }

      // Register all event listeners in parallel (no ordering dependency between them)
      ;[unlistenOpening, unlistenProgress, unlistenReadComplete, unlistenComplete, unlistenError, unlistenCancelled] =
        await Promise.all([
          onListingOpening((payload) => {
            if (isEventForCurrentLoad(payload.listingId, captured, loadGeneration)) {
              deps.setOpeningFolder(true)
            }
          }),
          onListingProgress((payload) => {
            if (isEventForCurrentLoad(payload.listingId, captured, loadGeneration)) {
              deps.setLoadingCount(payload.loadedCount)
            }
          }),
          onListingReadComplete((payload) => {
            if (isEventForCurrentLoad(payload.listingId, captured, loadGeneration)) {
              deps.setFinalizingCount(payload.totalCount)
            }
          }),
          onListingComplete((payload) => {
            if (isEventForCurrentLoad(payload.listingId, captured, loadGeneration)) {
              void handleListingComplete(payload, loadPath, loadSelectName)
            }
          }),
          onListingError((payload) => {
            if (isEventForCurrentLoad(payload.listingId, captured, loadGeneration)) {
              // For MTP volumes, trigger fallback on error (device likely disconnected)
              if (deps.getIsMtpView()) {
                resetLoadingState(payload.message)
                log.warn('MTP listing error, triggering fallback: {error}', {
                  error: payload.message,
                })
                deps.onMtpFatalError?.(payload.message)
                return
              }

              // For local volumes, check if the path was deleted.
              // Use the checked variant so a connection-blip "false" doesn't get treated as
              // "deleted": show the error pane in that case instead of walking up.
              void pathExistsChecked(loadPath).then(({ data: exists, timedOut }) => {
                if (!exists && !timedOut) {
                  // Path is gone: auto-navigate to nearest valid parent
                  log.info('Listing error for deleted path, navigating to valid parent: {path}', {
                    path: loadPath,
                  })
                  void resolveValidPath(loadPath, { volumeRoot: deps.getVolumePath() }).then((validPath) => {
                    navigateToFallback(validPath)
                  })
                } else {
                  // Path exists, or we couldn't tell: show the original listing error
                  const rendered = payload.error ? renderListingError(payload.error) : undefined
                  resetLoadingState(payload.message, false, rendered)
                  // Record the failed path in history so Cmd+[ goes back one step,
                  // not two. The success path pushes via the `onPathChange` call in
                  // `handleListingComplete`; without this call, an error pane would
                  // be visually displayed but absent from history, so Back would
                  // skip over it. `pushPath` deduplicates same-path retries.
                  deps.onPathChange?.(loadPath)

                  // A header-encrypted archive needs its password even to LIST it.
                  // Raise the browse-time password prompt ON TOP of the fallback
                  // error pane rendered above: on submit `retry` re-lists this same
                  // path (which now succeeds); on cancel the prompt closes and the
                  // "This archive needs a password" pane stays put (the user simply
                  // doesn't get in).
                  const reason = payload.error?.reason
                  if (reason?.reason === 'archiveNeedsPassword') {
                    deps.onArchiveNeedsPassword?.({
                      volumeId: deps.getVolumeId(),
                      archivePath: loadPath,
                      wrongAttempt: reason.wrongAttempt,
                      retry: () => {
                        void loadDirectory(loadPath)
                      },
                    })
                  }
                }
              })
            }
          }),
          onListingCancelled((payload) => {
            if (isEventForCurrentLoad(payload.listingId, captured, loadGeneration)) {
              // Cancellation handled by onCancelLoading callback
              resetLoadingState(undefined, true)
            }
          }),
        ])

      // Now start streaming listing - listeners are already set up
      benchmark.logEvent('IPC listDirectoryStart CALL')
      log.debug('[FilePane] calling listDirectoryStart: volumeId={volumeId}, path={loadPath}, listingId={listingId}', {
        volumeId,
        loadPath,
        listingId: newListingId,
      })
      const result = await listDirectoryStart(
        volumeId,
        path,
        includeHidden,
        deps.getSortBy(),
        deps.getSortOrder(),
        newListingId,
        deps.getDirectorySortMode(),
      )
      benchmark.logEventValue('IPC listDirectoryStart RETURNED', result.listingId)
      log.debug('[FilePane] listDirectoryStart returned: status={status}', {
        status: JSON.stringify(result.status),
      })

      // Check if this load was cancelled while we were starting
      if (thisGeneration !== loadGeneration) {
        // Cancel the abandoned listing
        void cancelListing(newListingId)
        return
      }
    } catch (e) {
      if (thisGeneration !== loadGeneration) return
      resetLoadingState(e instanceof Error ? e.message : String(e))
    }
  }

  // Handle listing completion event
  async function handleListingComplete(
    payload: ListingCompleteEvent,
    loadPath: string,
    loadSelectName: string | undefined,
  ) {
    benchmark.logEventValue('listing-complete received, totalCount', payload.totalCount)
    deps.setTotalCount(payload.totalCount)
    deps.setVolumeRootFromEvent(payload.volumeRoot)

    const includeHidden = deps.getIncludeHidden()

    // Determine initial cursor position
    if (loadSelectName) {
      const foundIndex = await findFileIndex(deps.getListingId(), loadSelectName, includeHidden)
      const adjustedIndex = deps.getHasParent() ? (foundIndex ?? -1) + 1 : (foundIndex ?? 0)
      deps.setCursorIndexRaw(adjustedIndex >= 0 ? adjustedIndex : 0)
    } else {
      deps.setCursorIndexRaw(0)
    }

    deps.setLoading(false)
    deps.setOpeningFolder(false)
    deps.setLoadingCount(undefined)
    deps.setFinalizingCount(undefined)
    benchmark.logEvent('loading = false (UI can render)')

    // NOW push to history (only on successful completion)
    deps.onPathChange?.(loadPath)

    // PII-free analytics: a navigation landed. Only the volume KIND enum crosses; never the path.
    void trackEvent('pane_navigated', { volume_kind: deps.getCaps().kind })

    // Fetch entry under the cursor for SelectionInfo
    deps.fetchEntryUnderCursor()

    // Fetch listing stats for SelectionInfo
    deps.fetchListingStats()

    // Resolve pending load promise (for MCP round-trips waiting on directory load)
    resolvePendingLoad()

    // Sync state to MCP for context tools
    deps.syncMcp()

    // Backfill Finder tags for the WHOLE listing (not just the visible range)
    // so scrolling shows dots instantly and a future sort/filter sees them.
    // Cancelable via `isStale`: stops on unmount, a newer load, or a listing
    // swap. The detached loop's logic lives in `tag-sweep.ts` (testable).
    if (getSetting('listing.showTags') && deps.getCaps().hasBackendListing) {
      const sweepGen = loadGeneration
      const sweepListingId = deps.getListingId()
      void sweepListingTags({
        listingId: sweepListingId,
        totalCount: payload.totalCount,
        includeHidden,
        isStale: () => isDestroyed || sweepGen !== loadGeneration || sweepListingId !== deps.getListingId(),
      })
    }

    // Scroll to cursor after DOM updates
    void tick().then(() => {
      const listRef = deps.getViewMode() === 'brief' ? deps.getBriefListRef() : deps.getFullListRef()
      listRef?.scrollToIndex(deps.getCursorIndex())
    })
  }

  // Handle cancellation during loading (called from DualPaneExplorer on ESC)
  function handleCancelLoading() {
    if (!deps.getLoading() || !deps.getListingId()) return

    // Cancel the Rust-side operation
    void cancelListing(deps.getListingId())

    // Extract the folder name we were trying to enter, so parent can select it when reloading
    const currentPath = deps.getCurrentPath()
    const folderName = currentPath.split('/').pop()

    // Tell parent to navigate back (passes the path we were loading so parent can decide where to go)
    deps.onCancelLoading?.(currentPath, folderName)
  }

  // Navigate to a specific path with optional item selection (used when cancelling navigation).
  // Returns a Promise that resolves when the directory listing completes, or rejects on error.
  function navigateToPath(path: string, selectName?: string): Promise<void> {
    deps.setCurrentPath(path)
    // Start loadDirectory first: it rejects any previous pending load
    void loadDirectory(path, selectName)
    // Then set up our promise (after the previous one was rejected)
    return new Promise<void>((resolve, reject) => {
      pendingLoadResolve = resolve
      pendingLoadReject = (reason: string) => {
        reject(new Error(reason))
      }
    })
  }

  async function navigateToParent(): Promise<boolean> {
    const currentPath = deps.getCurrentPath()
    if (currentPath === '/' || currentPath === deps.getVolumePath()) {
      return false // Already at root
    }
    const canonical = deps.getCanonicalPath()
    if (!canonical) return false // userHomePath not resolved yet
    const currentFolderName = basenameOf(canonical)
    const parentPath = parentOf(canonical)

    deps.setCurrentPath(parentPath)
    // Note: onPathChange is called in listing-complete handler after successful load
    await loadDirectory(parentPath, currentFolderName)
    return true
  }

  /**
   * Returns a promise that resolves when the current load (if any) settles.
   * Used by `moveCursor` (and any other callers that need a stable `listingId`)
   * to avoid the race where the FE has set a fresh `listingId` but
   * `list_directory_start_streaming` hasn't yet inserted the listing into the
   * backend's `LISTING_CACHE`. Wraps the existing `pendingLoadResolve` hook so we
   * don't introduce a second promise track: if no load is in flight, resolves
   * immediately.
   */
  function whenLoadSettles(): Promise<void> {
    if (!deps.getLoading()) return Promise.resolve()
    return new Promise<void>((resolve) => {
      // Chain onto the existing resolver / rejecter so we don't disturb
      // a pending `navigateToPath` caller already waiting on the load.
      const prevResolve = pendingLoadResolve
      const prevReject = pendingLoadReject
      pendingLoadResolve = () => {
        prevResolve?.()
        resolve()
      }
      pendingLoadReject = (reason: string) => {
        prevReject?.(reason)
        resolve() // We treat reject as "load is no longer in flight"; caller checks isLoading.
      }
    })
  }

  function getSwapState(): SwapState {
    return {
      currentPath: deps.getCurrentPath(),
      listingId: deps.getListingId(),
      totalCount: deps.getTotalCount(),
      cursorIndex: deps.getCursorIndex(),
      selectedIndices: deps.selection.getSelectedIndices(),
      lastSequence: deps.getLastSequence(),
    }
  }

  function adoptListing(state: SwapState): void {
    // Cancel any in-flight loads
    loadGeneration++

    // Set currentPath first so the initialPath $effect sees newPath === curPath and skips reload
    deps.setCurrentPath(state.currentPath)

    // Adopt the listing identity
    deps.setListingId(state.listingId)
    deps.setTotalCount(state.totalCount)
    deps.setLastSequence(state.lastSequence)

    // Restore cursor and selection
    deps.setCursorIndexRaw(state.cursorIndex)
    deps.selection.setSelectedIndices(state.selectedIndices)

    // Force virtual list to re-fetch visible range from (now-swapped) cache
    deps.bumpCacheGeneration()

    // Clear loading/error state
    deps.setLoading(false)
    deps.setError(null)

    // Re-fetch entry under cursor and listing stats for SelectionInfo
    deps.fetchEntryUnderCursor()
    deps.fetchListingStats()

    // Sync state to MCP
    deps.syncMcp()

    // Scroll to cursor position
    void tick().then(() => {
      const listRef = deps.getViewMode() === 'brief' ? deps.getBriefListRef() : deps.getFullListRef()
      listRef?.scrollToIndex(state.cursorIndex)
    })
  }

  function cleanup(): void {
    // Stop the background Finder-tag sweep if one is mid-flight.
    isDestroyed = true
    // Clean up listing
    const listingId = deps.getListingId()
    if (listingId) {
      void cancelListing(listingId)
      void listDirectoryEnd(listingId)
      evictPerPathIconsForDir(loadedPath)
    }
    unlistenOpening?.()
    unlistenProgress?.()
    unlistenReadComplete?.()
    unlistenComplete?.()
    unlistenError?.()
    unlistenCancelled?.()
  }

  return {
    loadDirectory,
    navigateToParent,
    navigateToPath,
    navigateToFallback,
    handleCancelLoading,
    whenLoadSettles,
    resetLoadingState,
    getSwapState,
    adoptListing,
    cleanup,
  }
}
