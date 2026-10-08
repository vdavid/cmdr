/**
 * What the viewer menu bar's Edit > Copy and Edit > Select all do, and the same pair in the
 * native right-click menu over the text.
 *
 * Neither can be a native menu item. AppKit's `copy:` / `selectAll:` selectors act on the DOM
 * selection, and the viewed file is out of its reach: `.file-content` is `user-select: none`
 * because the viewer owns an offset-based selection model of its own, so the only selectable
 * text left in the window is the status-bar footer. A native Select all would highlight the
 * footer and a native Copy would copy it. Rust routes both to this window as a typed
 * `ViewerEditActionKind` (`window_events::ViewerEditAction`) and the dispatch happens here.
 *
 * The branch is the same one ⌘C / ⌘A take in `viewer-keyboard.ts`, over the same
 * `isSearchInputFocused` / `inputHasSelection` helpers, and both paths end in the same two
 * functions. That's deliberate: whether a ⌘-chord reaches the webview before the menu's key
 * equivalent is unsettled here, and with one destination it stops mattering.
 */
import type { ViewerEditActionKind } from '$lib/ipc/bindings'
import { inputHasSelection, isSearchInputFocused, type SearchFocusState } from './viewer-keyboard'

export interface ViewerEditActionDeps {
  /** The search bar's visibility and input ref, the same pair the keyboard router reads. */
  search: SearchFocusState
  /** Selects the whole file: `createViewerKeyboard`'s `handleSelectAllShortcut`. */
  selectAllContent: () => void
  /** Copies the viewer's own selection: the copy orchestrator's `handleCopy`. */
  copyContent: () => void
  /** Puts `text` on the system clipboard. */
  writeClipboardText: (text: string) => void
}

/** Runs the Edit-menu action the backend forwarded, over the query or over the file. */
export function runViewerEditAction(action: ViewerEditActionKind, deps: ViewerEditActionDeps): void {
  const input = deps.search.searchInputRef
  const typingInSearch = isSearchInputFocused(deps.search) && Boolean(input)

  if (action === 'selectAll') {
    if (typingInSearch && input) input.select()
    else deps.selectAllContent()
    return
  }

  // Copy. A bare caret in the query means there's nothing in the input to copy, so the
  // gesture belongs to the file selection.
  if (typingInSearch && input && inputHasSelection(input)) {
    deps.writeClipboardText(input.value.slice(input.selectionStart ?? 0, input.selectionEnd ?? 0))
    return
  }
  deps.copyContent()
}

/**
 * Runs the pick from the native right-click menu over the text (`showViewerContextMenu`).
 * Always the file's, never the query's: the right-click leaves focus wherever it was, so the
 * search box may still hold it, but the menu was opened over the file.
 */
export function runViewerContextMenuAction(
  action: ViewerEditActionKind,
  deps: Pick<ViewerEditActionDeps, 'selectAllContent' | 'copyContent'>,
): void {
  if (action === 'selectAll') deps.selectAllContent()
  else deps.copyContent()
}
