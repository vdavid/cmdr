/**
 * Keyboard side-effects for the search-results snapshot pane. Routes a keydown
 * through the pure `computeSearchPaneKeyAction` dispatcher (in
 * `search-results-keys.ts`) and applies the returned action to pane state.
 * Lifted out of `FilePane.svelte`; splitting the dispatch (pure, already tested)
 * from the effects (this factory) keeps both unit-testable without spinning up
 * the component.
 *
 * The snapshot pane has no `..` row, so selection runs with `hasParent = false`
 * throughout (FilePane bakes that into the injected selection callbacks).
 */

import { openFileViewer } from '$lib/file-viewer/open-viewer'
import { openInEditor } from '$lib/tauri-commands'
import { computeSearchPaneKeyAction } from './search-results-keys'

export interface SearchPaneKeysDeps {
  getCursorIndex: () => number
  /** Move the cursor (scrolls + syncs MCP). */
  setCursorIndex: (index: number) => void
  /** Number of rows in the active snapshot. */
  getSearchResultsCount: () => number
  /** Visible row count for Page Up/Down math. */
  getVisibleItemsCount: () => number
  /** The snapshot entry at an index (for F3/F4 open), or undefined when out of range. */
  getSnapshotEntryAt: (index: number) => { path: string; isDirectory: boolean } | undefined
  /** Extend selection across a keyboard jump (toggle-and-fill), snapshot-pane semantics. */
  extendSelection: (fromIndex: number, toIndex: number, overflow: boolean) => void
  /** Toggle selection at an index, snapshot-pane semantics. */
  toggleSelectionAt: (index: number) => void
  /** Open the entry under the cursor (Enter). */
  openCursorItem: () => void
}

export interface SearchPaneKeys {
  /** Handle one keydown on the search-results pane. */
  handleSearchResultsKeyDown: (e: KeyboardEvent) => void
}

export function createSearchPaneKeys(deps: SearchPaneKeysDeps): SearchPaneKeys {
  /** Hand the cursor's file to the in-app viewer (F3) or default editor (F4). Directories are no-ops. */
  function openSnapshotFileWith(kind: 'viewer' | 'editor'): void {
    const entry = deps.getSnapshotEntryAt(deps.getCursorIndex())
    if (!entry || entry.isDirectory) return
    if (kind === 'viewer') {
      void openFileViewer(entry.path)
    } else {
      void openInEditor(entry.path)
    }
  }

  function applySearchPaneMove(index: number, overflow: boolean, shiftKey: boolean): void {
    if (shiftKey) {
      // Extend selection across the jump via the same toggle-and-fill helper the
      // regular pane uses. Snapshot panes carry no `..` row (hasParent = false).
      deps.extendSelection(deps.getCursorIndex(), index, overflow)
    }
    deps.setCursorIndex(index)
  }

  function handleSearchResultsKeyDown(e: KeyboardEvent): void {
    const action = computeSearchPaneKeyAction(e, {
      cursorIndex: deps.getCursorIndex(),
      count: deps.getSearchResultsCount(),
      visibleItems: deps.getVisibleItemsCount(),
    })
    if (action === null) return

    // Every action below "handles" the key. Prevent default + stop propagation so
    // the outer document-level dispatch doesn't double-fire (notably Space, which
    // the global selection.toggle case in `command-dispatch.ts` also listens for).
    e.preventDefault()
    e.stopPropagation()

    switch (action.kind) {
      case 'noop':
        return
      case 'open-cursor':
        deps.openCursorItem()
        return
      case 'view-file':
        openSnapshotFileWith('viewer')
        return
      case 'edit-file':
        openSnapshotFileWith('editor')
        return
      case 'toggle-selection-at-cursor':
        if (deps.getSearchResultsCount() > 0) deps.toggleSelectionAt(deps.getCursorIndex())
        return
      case 'toggle-selection-and-advance':
        if (deps.getSearchResultsCount() > 0) {
          const cursorIndex = deps.getCursorIndex()
          deps.toggleSelectionAt(cursorIndex)
          deps.setCursorIndex(Math.min(cursorIndex + 1, Math.max(0, deps.getSearchResultsCount() - 1)))
        }
        return
      case 'move-cursor':
        applySearchPaneMove(action.index, action.overflow, action.shiftKey)
        return
    }
  }

  return { handleSearchResultsKeyDown }
}
