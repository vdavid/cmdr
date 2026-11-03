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
 *
 * Only the text view owns a selection model. The formatted Markdown document and the
 * Binary / Hex views keep the browser's own, and there the menu IS the ⌘C path (the page's
 * keydown leaves the chord alone, so the Edit menu takes it). A Copy that finds nothing
 * selected says so in every one of them: a quiet no-op left people pasting their old
 * clipboard (ERR-ZNNZH).
 */
import type { ViewerContentKind, ViewerEditActionKind } from '#lib/ipc/bindings.js'
import { inputHasSelection, isSearchInputFocused, type SearchFocusState } from './viewer-keyboard'
import type { ViewerDisplayMode } from './viewer-view-mode'

/**
 * What Edit > Copy and Select all act on in the view on screen: the viewer's own text
 * selection, a formatted document's browser selection, the Binary / Hex view's browser
 * selection, or nothing of the page's (a picture; a PDF, whose selection lives in its embed).
 */
export type ViewerEditSurface = 'text' | 'formattedDocument' | 'bytes' | 'other'

export function editSurfaceFor(viewMode: ViewerDisplayMode, mediaKind: ViewerContentKind | null): ViewerEditSurface {
  if (viewMode === 'text') return 'text'
  if (viewMode === 'binary' || viewMode === 'hex') return 'bytes'
  return mediaKind === 'markdown' ? 'formattedDocument' : 'other'
}

export interface ViewerEditActionDeps {
  /** The search bar's visibility and input ref, the same pair the keyboard router reads. */
  search: SearchFocusState
  /** Selects the whole file: `createViewerKeyboard`'s `handleSelectAllShortcut`. */
  selectAllContent: () => void
  /** Copies the viewer's own selection: the copy orchestrator's `handleCopy`. */
  copyContent: () => void
  /** Puts `text` on the system clipboard: the search query's selected part. */
  writeClipboardText: (text: string) => void
  /** Selects the whole formatted document, not the window's chrome. */
  selectAllDocument: () => void
  /** The browser selection's text, empty when nothing is selected. */
  readDomSelection: () => string
  /** Copies text the page holds and confirms it, like a text-view copy. */
  copyDomText: (text: string) => void
  /** Says the Copy found nothing selected, naming the surface for the log. */
  reportNothingToCopy: (surface: ViewerEditSurface) => void
}

/** Runs the Edit-menu action the backend forwarded, over the query or over what `surface` shows. */
export function runViewerEditAction(
  action: ViewerEditActionKind,
  surface: ViewerEditSurface,
  deps: ViewerEditActionDeps,
): void {
  const input = deps.search.searchInputRef
  const typingInSearch = isSearchInputFocused(deps.search) && Boolean(input)

  if (action === 'selectAll') {
    if (typingInSearch && input) input.select()
    else if (surface === 'text') deps.selectAllContent()
    else if (surface === 'formattedDocument') deps.selectAllDocument()
    return
  }

  // Copy. A bare caret in the query means there's nothing in the input to copy, so the
  // gesture belongs to the view.
  if (typingInSearch && input && inputHasSelection(input)) {
    deps.writeClipboardText(input.value.slice(input.selectionStart ?? 0, input.selectionEnd ?? 0))
    return
  }
  if (surface === 'text') {
    deps.copyContent()
    return
  }
  if (surface === 'other') return
  const selected = deps.readDomSelection()
  if (selected) deps.copyDomText(selected)
  else deps.reportNothingToCopy(surface)
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
