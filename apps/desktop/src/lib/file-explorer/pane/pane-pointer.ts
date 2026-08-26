/**
 * What the mouse does to a file pane: selecting a row, the context menu, the
 * click that focuses the pane, and the background double-click that goes up a
 * folder.
 *
 * Keyboard equivalents live in `pane-key-router.ts`; the two stay separate
 * because the mouse carries state the keyboard doesn't (a range anchor, a
 * click target that may be inside the inline rename editor) and because a
 * pointer gesture can land on a row, on the background, or on the `..` row,
 * each with its own rule.
 */

import { getPathsAtIndices, showFileContextMenu, showParentRowContextMenu } from '$lib/tauri-commands'
import type { FileEntry, SelectPayload } from '../types'
import { getSetting, setSetting } from '$lib/settings'
import { addToast } from '$lib/ui/toast'
import { isFileListBackgroundClick } from './pane-background-dblclick'
import DoubleClickPaneHintToastContent from './DoubleClickPaneHintToastContent.svelte'

export interface PanePointerDeps {
  getCursorIndex: () => number
  /** Move the cursor without the scroll + MCP round-trip a keyboard move does. */
  setCursorIndex: (index: number) => void
  getHasParent: () => boolean
  getListingId: () => string
  getIncludeHidden: () => boolean
  getVolumeId: () => string
  /** The pane's selected indices, for the "right-clicked inside the selection" test. */
  getSelectedIndices: () => number[]
  onRequestFocus: () => void
  fetchCursorEntry: () => void
  /** Shift+click: extend the range from the cursor to the clicked row. */
  extendSelectionFromMouse: (index: number, cursorIndex: number, hasParent: boolean) => void
  /** Cmd+click: toggle the clicked row (a no-op on `..`). */
  toggleSelectionAt: (index: number, hasParent: boolean) => void
  /** End the Shift+click anchor gesture. */
  clearRangeState: () => void
  /** Cancel an in-flight type-to-jump. */
  clearJump: () => void
  navigateToParent: () => void
}

export interface PanePointer {
  handleSelect: (args: SelectPayload) => void
  handleContextMenu: (entry: FileEntry) => Promise<void>
  handlePaneClick: (event: MouseEvent) => void
  handlePaneBackgroundDblClick: (event: MouseEvent) => void
}

export function createPanePointer(deps: PanePointerDeps): PanePointer {
  function handleSelect({ index, shiftKey = false, metaKey = false }: SelectPayload): void {
    const hasParent = deps.getHasParent()
    if (shiftKey) {
      // Shift wins over Cmd when both are held (matches Finder).
      deps.extendSelectionFromMouse(index, deps.getCursorIndex(), hasParent)
    } else if (metaKey) {
      // Cmd+click toggles the clicked item. `..` is a no-op inside toggleAt.
      deps.toggleSelectionAt(index, hasParent)
      deps.clearRangeState()
    } else {
      deps.clearRangeState()
    }
    deps.setCursorIndex(index)
    deps.onRequestFocus()
    deps.fetchCursorEntry()
  }

  async function handleContextMenu(entry: FileEntry): Promise<void> {
    if (entry.name === '..') {
      // The `..` row gets its own one-item menu: "Add to favorites" (favorites the
      // parent dir `entry.path`). The full file menu (Copy / Move / Delete) makes no
      // sense on `..`. On a snapshot pane there's no real parent to favorite, so skip.
      deps.clearJump()
      if (deps.getVolumeId() === 'search-results') return
      await showParentRowContextMenu(entry.path)
      return
    }
    // Spec: opening a context menu cancels in-flight type-to-jump.
    deps.clearJump()
    // Match Finder: if the right-clicked entry is part of the current selection,
    // actions apply to the whole selection. Otherwise they apply to just this entry.
    let paths = [entry.path]
    const listingId = deps.getListingId()
    const indices = deps.getSelectedIndices()
    if (listingId && indices.length > 0) {
      try {
        const selectedPaths = await getPathsAtIndices(listingId, indices, deps.getIncludeHidden(), deps.getHasParent())
        if (selectedPaths.includes(entry.path)) {
          paths = selectedPaths
        }
      } catch {
        // Selection lookup failed: fall back to single-file action.
      }
    }
    await showFileContextMenu(entry.path, entry.name, entry.isDirectory, paths, false, listingId)
  }

  function handlePaneClick(event: MouseEvent): void {
    // Clicks inside the inline rename editor are the user placing the caret or
    // selecting text. Focusing the pane here would blur the input and end the
    // rename, making the field unusable with the mouse.
    const target = event.target
    if (target instanceof Element && target.closest('.rename-input')) return
    deps.onRequestFocus()
  }

  /**
   * Double-clicking the empty file-list background navigates up one folder
   * (Directory Opus-style), gated by `behavior.doubleClickPaneNavigatesToParent`.
   * On the very first trigger we raise a one-time INFO toast explaining it, and
   * flip the hidden `behavior.doubleClickOnPaneNotificationSeen` so it shows once.
   */
  function handlePaneBackgroundDblClick(event: MouseEvent): void {
    if (!getSetting('behavior.doubleClickPaneNavigatesToParent')) return
    if (!isFileListBackgroundClick(event.target)) return
    if (!deps.getHasParent()) return // nothing above (volume root / search-results pane)
    deps.navigateToParent()
    if (!getSetting('behavior.doubleClickOnPaneNotificationSeen')) {
      setSetting('behavior.doubleClickOnPaneNotificationSeen', true)
      addToast(DoubleClickPaneHintToastContent, {
        level: 'info',
        dismissal: 'persistent',
        id: 'double-click-pane-hint',
      })
    }
  }

  return { handleSelect, handleContextMenu, handlePaneClick, handlePaneBackgroundDblClick }
}
