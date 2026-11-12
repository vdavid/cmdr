import {
  findFileIndex,
  findFileIndices,
  getSelectionSnapshot,
  onDirectoryDeleted,
  onDirectoryDiff,
  onListingRespelled,
  onWriteSourceItemDone,
  type WriteSourceItemDoneEvent,
} from '#lib/tauri-commands/index.js'
import { resolveValidPath } from '../navigation/path-resolution'
import { movesByPreviousIndex, remapIndicesAcrossDiff } from '../operations/adjust-selection-indices'
import { buildFrontendIndices, extractFilename } from '../operations/selection-adjustment'
import { getAppLogger } from '#lib/logging/logger.js'
import { getForegroundOperationId, isForegroundClaimPending } from '#lib/file-operations/foreground-operation.svelte.js'
import { untrack } from 'svelte'
import { createThrottle } from '#lib/utils/timing.js'
import type { DiffChange, SortColumn } from '../types'
import type { createSelectionState } from './selection-state.svelte'
import type { createRenameState } from '../rename/rename-state.svelte'
import type { PaneRowState } from './pane-row-state'

const log = getAppLogger('fileExplorer')

/**
 * Minimum interval between visible-listing refetches driven by streaming
 * `directory-diff` events — the ≤4 updates/sec (250 ms) cap on the demand side.
 * Under heavy churn (a clonefile loop, a scan of a live filesystem) the backend
 * `diff_emitter` already collapses events to ~20/sec, but each frontend refetch
 * re-enriches and re-renders the visible range, allocating fresh WebKit
 * compositor surfaces (1+ GB GPU under a storm). A leading + trailing throttle
 * keeps the first update instant and caps the rest here. Tweak this one knob if
 * the reveal ever feels laggy.
 */
export const INDEX_LISTING_UPDATE_MIN_INTERVAL_MS = 250

/**
 * Pure reconciliation of cursor + selection against a directory diff. Backend
 * indices are this pane's rows (the backend leaves out what the pane hides); the
 * frontend offsets by 1 for the `..` row when `hasParent`. Off-by-one here lands
 * the cursor a row off or shifts the selection, so the `offset` bookkeeping is
 * load-bearing.
 *
 * The cursor and the selection FOLLOW their rows across a reorder: watch a big
 * folder being deleted in a date-sorted pane and it keeps bumping itself to the
 * top, and a cursor that stayed on its old index would sit on a neighbour,
 * reading as if the wrong folder were vanishing.
 *
 * Returns the new frontend cursor index, whether it got there by following its
 * row rather than by sliding with the listing (the caller scrolls that row back
 * into view), and the new selection indices when they need replacing (`null` =
 * leave the selection untouched when nothing is selected).
 */
export function reconcileCursorAndSelection(input: {
  changes: DiffChange[]
  hasParent: boolean
  cursorIndex: number
  selectedIndices: number[]
  operationSelectedNames: string[] | 'all' | null
  count: number
}): { cursorIndex: number; cursorFollowedMove: boolean; selectedIndices: number[] | null } {
  const { changes, hasParent, cursorIndex, selectedIndices, count } = input

  const moves = movesByPreviousIndex(changes)
  const hasStructuralChanges = moves.size > 0 || changes.some((c) => c.type === 'add' || c.type === 'remove')
  if (!hasStructuralChanges) {
    return { cursorIndex, cursorFollowedMove: false, selectedIndices: null }
  }

  // A move vacates its old position and fills its new one, so it shifts the rows
  // it jumped over exactly like a removal plus an insertion would.
  const removeIndices = [...changes.filter((c) => c.type === 'remove').map((c) => c.index), ...moves.keys()]
  const addIndices = [...changes.filter((c) => c.type === 'add').map((c) => c.index), ...moves.values()]

  const offset = hasParent ? 1 : 0

  // Cursor: always adjust (no operation-specific cursor handling exists)
  const backendCursor = cursorIndex - offset
  const cursorFollowedMove = moves.has(backendCursor)
  const adjustedCursor = remapIndicesAcrossDiff([backendCursor], removeIndices, addIndices, moves)
  let newCursorIndex: number
  if (adjustedCursor.length > 0) {
    newCursorIndex = adjustedCursor[0] + offset
  } else {
    newCursorIndex = Math.max(0, Math.min(cursorIndex, count - 1 + offset))
  }

  // Name tracking may refine an operation's selection later, but even while that
  // lookup waits the indices must belong to the applied revision (including for sort).
  let newSelectedIndices: number[] | null = null
  if (selectedIndices.length > 0) {
    const backendSelected = selectedIndices.map((i) => i - offset)
    newSelectedIndices = remapIndicesAcrossDiff(backendSelected, removeIndices, addIndices, moves).map(
      (i) => i + offset,
    )
  }

  return { cursorIndex: newCursorIndex, cursorFollowedMove, selectedIndices: newSelectedIndices }
}

export interface ListingDiffSyncDeps {
  rowState: PaneRowState
  selection: ReturnType<typeof createSelectionState>
  rename: ReturnType<typeof createRenameState>
  renameFlow: { pendingCursorName: string | null }
  getListingId: () => string
  getIncludeHidden: () => boolean
  getHasParent: () => boolean
  getCursorIndex: () => number
  setCursorIndex: (index: number) => Promise<void>
  /** Direct cursor write (no scroll/fetch side effects), mirrors the inline assignment. */
  applyCursorIndex: (index: number) => void
  getCurrentPath: () => string
  getVolumePath: () => string
  getOperationSelectedNames: () => string[] | 'all' | null
  setTotalCount: (count: number) => void
  /** The hidden items an empty pane hides; a batch may carry only this (a dotfile came or went). */
  setHiddenCount: (count: number) => void
  /** Installs the column a re-read re-sorted the rows into, with the batch that lands them. */
  setEffectiveSortBy: (column: SortColumn) => void
  bumpSoftRefreshTick: () => void
  scheduleColumnWidthRefetch: () => void
  fetchEntryUnderCursor: () => void
  fetchListingStats: () => void
  onRequestFocus?: () => void
  navigateToFallback: (validPath: string | null) => void
  /** Takes the volume's own spelling of the shown directory as the pane's path. */
  adoptStoredPath: (path: string) => void
  /** Makes the list refetch its visible rows from the (rewritten) cache. */
  bumpCacheGeneration: () => void
}

/**
 * Registers the three file-watcher listeners that keep a `FilePane` in sync with
 * external directory changes: the `directory-diff` reconciliation (cursor +
 * selection), the `write-source-item-done` gradual deselection during the write op
 * this pane started (filtered by its id),
 * and the `directory-deleted` navigate-to-parent fallback.
 *
 * Each registers its `listen` once inside a `$effect` and returns the unsubscribe
 * cleanup, so this factory MUST be called synchronously during component init
 * (it needs Svelte's effect-tracking context). Mirrors the `initAiToastSync`
 * pattern.
 */
export function initListingDiffSync(deps: ListingDiffSyncDeps): void {
  // Listen for file watcher diff events
  $effect(() => {
    // Leading + trailing throttle for the visible-listing refetch. The first
    // diff fires it immediately (instant feedback); a burst then coalesces to
    // ≤4/sec. Count, cursor, and selection reconciliation stay synchronous.
    const throttledListingRefresh = createThrottle(() => {
      // Bump the soft-refresh tick (renames don't change totalCount, so the
      // tick is what guarantees a refresh), and schedule the brief-mode
      // column-width refetch. We deliberately DON'T bump `cacheGeneration`:
      // that'd cause a destructive wipe on every diff event, flickering the
      // source pane empty mid-bulk-op.
      deps.bumpSoftRefreshTick()
      deps.scheduleColumnWidthRefetch()
      deps.fetchListingStats()
    }, INDEX_LISTING_UPDATE_MIN_INTERVAL_MS)

    let scrollCursor = false
    deps.rowState.setBatchApplier((diff) => {
      const listingId = deps.getListingId()

      // If a rename is active and the file being renamed was removed
      // externally, cancel the rename gracefully
      if (deps.rename.active && deps.rename.target) {
        const targetPath = deps.rename.target.path
        const wasRemoved = diff.changes.some((c) => c.type === 'remove' && c.entry.path === targetPath)
        if (wasRemoved) {
          deps.rename.cancel()
          deps.onRequestFocus?.()
        }
      }

      const includeHidden = deps.getIncludeHidden()
      const hasParent = deps.getHasParent()
      const operationSelectedNames = deps.getOperationSelectedNames()
      const reconciled = reconcileCursorAndSelection({
        changes: diff.changes,
        hasParent,
        cursorIndex: deps.getCursorIndex(),
        selectedIndices: deps.selection.getSelectedIndices(),
        operationSelectedNames,
        count: diff.totalCount,
      })
      if (reconciled.selectedIndices !== null) deps.selection.setSelectedIndices(reconciled.selectedIndices)
      deps.applyCursorIndex(reconciled.cursorIndex)
      deps.setTotalCount(diff.totalCount)
      deps.setHiddenCount(diff.hiddenCount)
      if (diff.effectiveSortBy !== null) deps.setEffectiveSortBy(diff.effectiveSortBy)
      scrollCursor ||= reconciled.cursorFollowedMove
      return () => {
        // All logical row writes above are synchronous. Scroll/fetch are effects, never prerequisites.
        if (scrollCursor) void deps.setCursorIndex(deps.getCursorIndex())
        scrollCursor = false
        deps.fetchEntryUnderCursor()
        throttledListingRefresh.call()

        const nameToFind = deps.renameFlow.pendingCursorName
        if (nameToFind || (operationSelectedNames !== null && operationSelectedNames !== 'all')) {
          const endWork = deps.rowState.beginAsyncWork()
          const token = { ...deps.rowState.capture(), sequence: diff.sequence }
          void Promise.resolve()
            .then(async () => {
              if (!deps.rowState.matches(token)) return
              if (nameToFind) {
                const found = await findFileIndex(listingId, nameToFind, includeHidden)
                await getSelectionSnapshot(listingId, includeHidden, [], token.sequence)
                if (!deps.rowState.matches(token)) return
                if (deps.renameFlow.pendingCursorName !== nameToFind) return
                deps.renameFlow.pendingCursorName = null
                if (found !== null) void deps.setCursorIndex(found + (hasParent ? 1 : 0))
              }
              if (Array.isArray(operationSelectedNames)) {
                const indices = await findFileIndices(listingId, operationSelectedNames, includeHidden)
                await getSelectionSnapshot(listingId, includeHidden, [], token.sequence)
                if (!deps.rowState.matches(token) || deps.getOperationSelectedNames() !== operationSelectedNames) return
                deps.selection.setSelectedIndices(buildFrontendIndices(indices, hasParent))
              }
            })
            .catch(() => {})
            .finally(endWork)
        }
      }
    })
    const listenerPromise = onDirectoryDiff((diff) => {
      if (diff.listingId === deps.getListingId()) deps.rowState.receive(diff.batches)
    })

    return () => {
      throttledListingRefresh.cancel()
      void listenerPromise
        .then((unsub) => {
          unsub()
        })
        .catch(() => {})
    }
  })

  // Gradual deselection: each source of the operation THIS pane started leaves the
  // selection as `write-source-item-done` reports it finished. Only the pane holding an
  // operation snapshot listens, and that operation is the one its foreground progress
  // dialog owns, so its id is `getForegroundOperationId()`. ❌ Never unfiltered: the queue
  // runs operations on other lanes at once, and one finishing a same-named file would
  // deselect this pane's row. An event that beats the start command's answer (the claim
  // is still pending, so the operation has no name here yet) waits for it.
  //
  // A ⌘A snapshot (`'all'`) deselects the same way: each finished source finds its own
  // row by file name, so the snapshot's names are never needed, only its presence. The
  // selection itself is always an explicit index set, so leaving "all" is one O(1) delete.
  let unnamed: WriteSourceItemDoneEvent[] = []

  const hasOperationSnapshot = () => deps.getOperationSelectedNames() !== null

  function deselectFinished(payload: WriteSourceItemDoneEvent): void {
    if (!hasOperationSnapshot()) return
    const owner = getForegroundOperationId()
    if (payload.operationId !== owner) {
      if (isForegroundClaimPending()) unnamed.push(payload)
      return
    }
    deselectSource(payload)
  }

  // The claim settled: what waited for the name either was this pane's or wasn't.
  $effect(() => {
    const owner = getForegroundOperationId()
    if (isForegroundClaimPending()) return
    const waiting = unnamed
    unnamed = []
    untrack(() => {
      for (const payload of waiting) {
        if (payload.operationId === owner && hasOperationSnapshot()) deselectSource(payload)
      }
    })
  })

  $effect(() => {
    const listenerPromise = onWriteSourceItemDone(deselectFinished)

    return () => {
      void listenerPromise
        .then((unsub) => {
          unsub()
        })
        .catch(() => {})
    }
  })

  function deselectSource(payload: WriteSourceItemDoneEvent): void {
    const token = deps.rowState.capture()
    const names = deps.getOperationSelectedNames()
    const endWork = deps.rowState.beginAsyncWork()
    const filename = extractFilename(payload.sourcePath)
    const includeHidden = deps.getIncludeHidden()
    void findFileIndex(token.listingId, filename, includeHidden)
      .then(async (backendIndex) => {
        await getSelectionSnapshot(token.listingId, includeHidden, [], token.sequence)
        if (backendIndex === null || !deps.rowState.matches(token) || deps.getOperationSelectedNames() !== names) return
        const frontendIndex = deps.getHasParent() ? backendIndex + 1 : backendIndex
        deps.selection.selectedIndices.delete(frontendIndex)
      })
      .catch(() => {})
      .finally(endWork)
  }

  // Listen for listing-respelled events: after the kernel mount hands a share to a
  // direct SMB connection, the backend re-reads open listings in the share's own
  // spelling. Same folder, new bytes in the path and every row, so the pane adopts
  // the path and refetches its rows.
  $effect(() => {
    const listenerPromise = onListingRespelled((payload) => {
      if (payload.listingId !== deps.getListingId()) return
      deps.adoptStoredPath(payload.path)
      deps.bumpCacheGeneration()
      deps.fetchEntryUnderCursor()
    })

    return () => {
      void listenerPromise
        .then((unsub) => {
          unsub()
        })
        .catch(() => {})
    }
  })

  // Listen for directory-deleted events (watched directory was removed externally)
  $effect(() => {
    const listenerPromise = onDirectoryDeleted((payload) => {
      if (payload.listingId !== deps.getListingId()) return

      log.info('Directory deleted externally, navigating to nearest valid parent: {path}', {
        path: payload.path,
      })

      void resolveValidPath(deps.getCurrentPath(), { volumeRoot: deps.getVolumePath() }).then((validPath) => {
        deps.navigateToFallback(validPath)
      })
    })

    return () => {
      void listenerPromise
        .then((unsub) => {
          unsub()
        })
        .catch(() => {})
    }
  })
}
