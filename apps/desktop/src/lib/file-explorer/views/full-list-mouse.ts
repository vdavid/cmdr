/**
 * What a mousedown on a Full-view row means: ignore it, just move the cursor, or
 * arm a drag (and with which payload).
 *
 * It's a pure decision so the payload rules can be pinned by tests rather than by
 * dragging files around. Three of them matter:
 *
 * - **The `..` row never drags.** It isn't a real entry, so it only moves the cursor.
 * - **With nothing selected, selection is DEFERRED** until the drag threshold is
 *   crossed (or the drag is cancelled). Selecting on press instead would make a
 *   click-and-hold look like a selection the user didn't ask for.
 * - **With a selection, the drag always carries the whole selection**, whichever row
 *   was pressed, and the preview icon comes from the FIRST selected row.
 */

import type { FileEntry } from '../types'
// Type-only: keeping the drag module out of this one's runtime graph is what lets
// the planner be unit-tested without a Tauri environment.
import type { startSelectionDragTracking, DragFileInfo } from '../drag/drag-drop'

/** The drag context union `startSelectionDragTracking` accepts (single / selection / paths). */
export type DragContext = Parameters<typeof startSelectionDragTracking>[1]

export type RowMouseDownPlan =
  /** Not ours: a non-primary button, a click inside the inline rename input, or no entry. */
  | { kind: 'ignore' }
  /** The `..` row: move the cursor, no drag tracking. */
  | { kind: 'select' }
  | {
      kind: 'drag'
      /** `true` starts the click-to-rename timer, `false` cancels any pending one. */
      startClickToRename: boolean
      /** `true` selects on press; `false` defers it to the drag start / cancel callbacks. */
      selectNow: boolean
      context: DragContext
    }

export interface RowMouseDownInput {
  event: MouseEvent
  /** UI index of the pressed row. */
  index: number
  cursorIndex: number
  selectedIndices: Set<number>
  /** Resolves a UI index to its entry; rows outside the fetched window return `undefined`. */
  getEntryAt: (globalIndex: number) => FileEntry | undefined
  listingId: string
  volumeId: string
  includeHidden: boolean
  hasParent: boolean
  /** True on a search-results pane: no backend listing to resolve indices against. */
  usingStaticEntries: boolean
  /** True while the inline rename editor is open (suppresses click-to-rename). */
  isRenaming: boolean
  /** False when the host pane didn't wire click-to-rename at all. */
  canStartRename: boolean
}

/** The drag preview needs a name + kind + icon per file, for the cached rows we have. */
function collectFileInfos(
  selectedIndices: Set<number>,
  getEntryAt: (index: number) => FileEntry | undefined,
): DragFileInfo[] {
  const infos: DragFileInfo[] = []
  for (const index of selectedIndices) {
    const entry = getEntryAt(index)
    if (entry) infos.push({ name: entry.name, isDirectory: entry.isDirectory, iconId: entry.iconId })
  }
  return infos
}

export function planRowMouseDown(input: RowMouseDownInput): RowMouseDownPlan {
  const { event, index, selectedIndices, getEntryAt, volumeId } = input

  if (event.button !== 0) return { kind: 'ignore' }

  // Let clicks inside the inline rename input pass through without
  // triggering selection/drag; the input handles its own focus.
  const target = event.target as HTMLElement
  if (target.closest('.rename-input')) return { kind: 'ignore' }

  const entry = getEntryAt(index)
  if (!entry) return { kind: 'ignore' }
  if (entry.name === '..') return { kind: 'select' }

  // Click-to-rename: pressing the entry already under the cursor, with no modifiers,
  // arms an 800 ms timer. Pressing any other row cancels a pending one. Drag tracking
  // still runs either way, so the cursor row stays draggable; crossing the drag
  // threshold cancels the timer.
  const startClickToRename =
    index === input.cursorIndex && !event.shiftKey && !event.metaKey && !input.isRenaming && input.canStartRename

  if (selectedIndices.size === 0) {
    const fileInfo: DragFileInfo = { name: entry.name, isDirectory: entry.isDirectory, iconId: entry.iconId }
    return {
      kind: 'drag',
      startClickToRename,
      selectNow: false,
      context: {
        type: 'single',
        path: entry.path,
        iconId: entry.iconId,
        index,
        sourceVolumeId: volumeId,
        fileInfo,
      },
    }
  }

  // Find the first selected file's icon for the drag preview.
  const firstSelectedEntry = getEntryAt(Math.min(...selectedIndices))
  const iconId = firstSelectedEntry?.iconId ?? entry.iconId
  const fileInfos = collectFileInfos(selectedIndices, getEntryAt)

  // Search-results / static-entries panes have no backend listing, so
  // `start_selection_drag` (which resolves indices against LISTING_CACHE)
  // would fail. The entries already carry absolute paths, so we route
  // through the paths-by-value drag flavour.
  if (input.usingStaticEntries) {
    const paths: string[] = []
    for (const selectedIndex of selectedIndices) {
      const selectedEntry = getEntryAt(selectedIndex)
      if (selectedEntry) paths.push(selectedEntry.path)
    }
    return {
      kind: 'drag',
      startClickToRename,
      selectNow: true,
      context: { type: 'paths', paths, sourceVolumeId: volumeId, iconId, fileInfos },
    }
  }

  return {
    kind: 'drag',
    startClickToRename,
    selectNow: true,
    context: {
      type: 'selection',
      listingId: input.listingId,
      indices: [...selectedIndices],
      includeHidden: input.includeHidden,
      hasParent: input.hasParent,
      sourceVolumeId: volumeId,
      iconId,
      fileInfos,
    },
  }
}
