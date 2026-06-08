import { findFileIndex, findFileIndices, getTotalCount, listen, onWriteSourceItemDone } from '$lib/tauri-commands'
import { resolveValidPath } from '../navigation/path-resolution'
import { adjustSelectionIndices } from '../operations/adjust-selection-indices'
import { buildFrontendIndices, extractFilename } from '../operations/selection-adjustment'
import { getAppLogger } from '$lib/logging/logger'
import type { DiffChange, DirectoryDeletedEvent, DirectoryDiff } from '../types'
import type { createSelectionState } from './selection-state.svelte'
import type { createRenameState } from '../rename/rename-state.svelte'

const log = getAppLogger('fileExplorer')

/**
 * Pure reconciliation of cursor + selection against a directory diff. Backend
 * indices live in the listing's own space; the frontend offsets by 1 for the
 * `..` row when `hasParent`. Off-by-one here lands the cursor a row off or
 * shifts the selection, so the `offset` bookkeeping is load-bearing.
 *
 * Returns the new frontend cursor index, and the new selection indices when
 * they need replacing (`null` = leave the selection untouched, which is the
 * case during an active operation or when nothing is selected).
 */
export function reconcileCursorAndSelection(input: {
  changes: DiffChange[]
  hasParent: boolean
  cursorIndex: number
  selectedIndices: number[]
  operationSelectedNames: string[] | 'all' | null
  count: number
}): { cursorIndex: number; selectedIndices: number[] | null } {
  const { changes, hasParent, cursorIndex, selectedIndices, operationSelectedNames, count } = input

  const hasStructuralChanges = changes.some((c) => c.type === 'add' || c.type === 'remove')
  if (!hasStructuralChanges) {
    return { cursorIndex, selectedIndices: null }
  }

  const removeIndices = changes.filter((c) => c.type === 'remove').map((c) => c.index)
  const addIndices = changes.filter((c) => c.type === 'add').map((c) => c.index)

  const offset = hasParent ? 1 : 0

  // Cursor: always adjust (no operation-specific cursor handling exists)
  const backendCursor = cursorIndex - offset
  const adjustedCursor = adjustSelectionIndices([backendCursor], removeIndices, addIndices)
  let newCursorIndex: number
  if (adjustedCursor.length > 0) {
    newCursorIndex = adjustedCursor[0] + offset
  } else {
    newCursorIndex = Math.max(0, Math.min(cursorIndex, count - 1 + offset))
  }

  // Selection: only adjust outside operations (operations handle via findFileIndices)
  let newSelectedIndices: number[] | null = null
  if (operationSelectedNames === null && selectedIndices.length > 0) {
    const backendSelected = selectedIndices.map((i) => i - offset)
    const adjusted = adjustSelectionIndices(backendSelected, removeIndices, addIndices)
    newSelectedIndices = adjusted.map((i) => i + offset)
  }

  return { cursorIndex: newCursorIndex, selectedIndices: newSelectedIndices }
}

export interface ListingDiffSyncDeps {
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
  getLastSequence: () => number
  setLastSequence: (sequence: number) => void
  getDiffGeneration: () => number
  bumpDiffGeneration: () => number
  setTotalCount: (count: number) => void
  bumpSoftRefreshTick: () => void
  scheduleColumnWidthRefetch: () => void
  fetchEntryUnderCursor: () => void
  fetchListingStats: () => void
  onRequestFocus?: () => void
  navigateToFallback: (validPath: string | null) => void
}

/**
 * Registers the three file-watcher listeners that keep a `FilePane` in sync with
 * external directory changes: the `directory-diff` reconciliation (cursor +
 * selection), the `write-source-item-done` gradual deselection during a write op,
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
    const listenerPromise = listen<DirectoryDiff>('directory-diff', (event) => {
      const diff = event.payload
      const listingId = deps.getListingId()
      // Only process diffs for our current listing
      if (diff.listingId !== listingId) return

      // Ignore out-of-order events
      if (diff.sequence <= deps.getLastSequence()) return
      deps.setLastSequence(diff.sequence)

      // If a rename is active and the file being renamed was removed
      // externally, cancel the rename gracefully
      if (deps.rename.active && deps.rename.target) {
        const targetName = deps.rename.target.originalName
        const wasRemoved = diff.changes.some((c) => c.type === 'remove' && c.entry.name === targetName)
        if (wasRemoved) {
          deps.rename.cancel()
          deps.onRequestFocus?.()
        }
      }

      const includeHidden = deps.getIncludeHidden()

      // Refetch total count, bump the soft-refresh tick (renames don't
      // change totalCount, so the tick is what guarantees a refresh),
      // and schedule a throttled column-width refetch in brief mode.
      // We deliberately DON'T bump `cacheGeneration` here: that'd cause
      // a destructive wipe on every diff event, flickering the source
      // pane empty mid-bulk-op.
      void getTotalCount(listingId, includeHidden).then(async (count) => {
        deps.setTotalCount(count)
        deps.bumpSoftRefreshTick()
        deps.scheduleColumnWidthRefetch()

        const hasParent = deps.getHasParent()

        // Post-rename cursor tracking: move cursor to the renamed file
        const nameToFind = deps.renameFlow.pendingCursorName
        if (nameToFind) {
          deps.renameFlow.pendingCursorName = null
          const foundIndex = await findFileIndex(listingId, nameToFind, includeHidden)
          if (foundIndex !== null) {
            const adjustedIndex = hasParent ? foundIndex + 1 : foundIndex
            await deps.setCursorIndex(adjustedIndex)
            return
          }
        }

        // Adjust cursor and selection BEFORE fetching entry under cursor,
        // otherwise fetchEntryUnderCursor uses the old index against the
        // new (shorter) listing, causing "index out of bounds" errors.
        const operationSelectedNames = deps.getOperationSelectedNames()
        const reconciled = reconcileCursorAndSelection({
          changes: diff.changes,
          hasParent,
          cursorIndex: deps.getCursorIndex(),
          selectedIndices: deps.selection.getSelectedIndices(),
          operationSelectedNames,
          count,
        })
        deps.applyCursorIndex(reconciled.cursorIndex)
        if (reconciled.selectedIndices !== null) {
          deps.selection.setSelectedIndices(reconciled.selectedIndices)
        }

        deps.fetchEntryUnderCursor()
        deps.fetchListingStats()

        // Diff-driven selection adjustment: re-resolve selected names to new indices
        if (operationSelectedNames !== null && operationSelectedNames !== 'all') {
          const myGeneration = deps.bumpDiffGeneration()
          void findFileIndices(listingId, operationSelectedNames, includeHidden).then((nameToIndexMap) => {
            if (myGeneration !== deps.getDiffGeneration()) return
            deps.selection.setSelectedIndices(buildFrontendIndices(nameToIndexMap, hasParent))
          })
        }
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

  // Listen for write-source-item-done events (gradual deselection as each source completes).
  // No operationId filter needed: only one write op runs at a time, and only the pane with
  // an active snapshot (operationSelectedNames) processes events.
  $effect(() => {
    const listenerPromise = onWriteSourceItemDone((payload) => {
      // Only process when we have an active operation with explicit name tracking
      if (!Array.isArray(deps.getOperationSelectedNames())) return

      const filename = extractFilename(payload.sourcePath)
      void findFileIndex(deps.getListingId(), filename, deps.getIncludeHidden()).then((backendIndex) => {
        if (backendIndex === null) return
        const frontendIndex = deps.getHasParent() ? backendIndex + 1 : backendIndex
        deps.selection.selectedIndices.delete(frontendIndex)
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

  // Listen for directory-deleted events (watched directory was removed externally)
  $effect(() => {
    const listenerPromise = listen<DirectoryDeletedEvent>('directory-deleted', (event) => {
      if (event.payload.listingId !== deps.getListingId()) return

      log.info('Directory deleted externally, navigating to nearest valid parent: {path}', {
        path: event.payload.path,
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
