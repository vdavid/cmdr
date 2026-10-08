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
 *
 * `handleContextMenu` is the one deliberate exception: `⌃⏎` calls it too, passing an
 * anchor. Which rows the menu acts on is a rule nobody should own twice.
 */

import { getPathsAtIndices, showFileContextMenu, showParentRowContextMenu, type MenuAnchor } from '$lib/tauri-commands'
import { contextMenuCountText, contextMenuSizeBytes, contextMenuSizeText } from '../selection/context-menu-target'
import { boundShortcuts } from '$lib/shortcuts'
import type { FileEntry, SelectPayload } from '../types'
import { getSetting, setSetting } from '$lib/settings'
import { addToast } from '$lib/ui/toast'
import { capabilitiesFor, paneFolderCanBeFavorited, rowCanShareLink, rowIsOsVisible } from './volume-capabilities'
import { canOpenTerminalIn } from '$lib/open-terminal/terminal-target'
import { isFileListBackgroundClick } from './pane-background-dblclick'
import { sameKindTargetFor } from './select-same-kind'
import DoubleClickPaneHintToastContent from './DoubleClickPaneHintToastContent.svelte'

/** Shift+click args: extend the range from the cursor to the clicked row. */
export interface ExtendSelectionFromMouseArgs {
  index: number
  cursorIndex: number
  hasParent: boolean
}

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
  /**
   * The selection's total size for the context menu's header line, or `null` when
   * there isn't an honest one (no listing stats yet, or a folder in the selection —
   * see `selection/context-menu-target.ts` for why a folder disqualifies it).
   * Only read when the right-click landed inside the selection.
   */
  getSelectedFilesTotalSize: () => number | null
  onRequestFocus: () => void
  fetchCursorEntry: () => void
  /** Shift+click: extend the range from the cursor to the clicked row. */
  extendSelectionFromMouse: (args: ExtendSelectionFromMouseArgs) => void
  /** Cmd+click: toggle the clicked row (a no-op on `..`). */
  toggleSelectionAt: (index: number, hasParent: boolean) => void
  /** End the Shift+click anchor gesture. */
  clearRangeState: () => void
  /** Cancel an in-flight type-to-jump. */
  clearJump: () => void
  navigateToParent: () => void
}

export interface PanePointer {
  /**
   * The context menu for `entry`, acting on the whole selection when `entry` is inside
   * it and on that one row otherwise.
   *
   * ❗ The keyboard (`⌃⏎`) comes through HERE too, with an `anchor`, rather than
   * re-deriving the selection-vs-row rule: two paths that decide it separately are two
   * paths that drift apart. `anchor` is the ONLY difference between them — omitted, the
   * OS uses the pointer, which is what a right-click wants.
   */
  handleContextMenu: (entry: FileEntry, anchor?: MenuAnchor | null) => Promise<void>
  handleSelect: (args: SelectPayload) => void
  handlePaneClick: (event: MouseEvent) => void
  handlePaneBackgroundDblClick: (event: MouseEvent) => void
}

export function createPanePointer(deps: PanePointerDeps): PanePointer {
  function handleSelect({ index, shiftKey = false, metaKey = false }: SelectPayload): void {
    const hasParent = deps.getHasParent()
    if (shiftKey) {
      // Shift wins over Cmd when both are held (matches Finder).
      deps.extendSelectionFromMouse({ index, cursorIndex: deps.getCursorIndex(), hasParent })
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

  async function handleContextMenu(entry: FileEntry, anchor: MenuAnchor | null = null): Promise<void> {
    if (entry.name === '..') {
      // The `..` row gets its own one-item menu: "Add to favorites" (favorites the
      // parent dir `entry.path`). The full file menu (Copy / Move / Delete) makes no
      // sense on `..`. Where that parent isn't somewhere a favorite could point back to
      // — a snapshot pane, an archive's insides, a `.git`-portal folder, a phone — the
      // menu would hold nothing but an item `add_favorite` refuses, so none pops at all.
      deps.clearJump()
      if (!paneFolderCanBeFavorited(deps.getVolumeId(), entry.path)) return
      await showParentRowContextMenu(entry.path, anchor)
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
    const volumeId = deps.getVolumeId()
    const rowsAreOsPaths = rowIsOsVisible(volumeId, entry.path)
    // The header at the top of the menu says what the menu will act on, so its size has
    // to be the size of THAT: the whole selection when the click landed inside it (the
    // pane already totals it for the status bar), this one row otherwise.
    const sizeBytes = paths.length > 1 ? deps.getSelectedFilesTotalSize() : contextMenuSizeBytes([entry])
    await showFileContextMenu(
      entry.path,
      entry.name,
      entry.isDirectory,
      paths,
      {
        listingId,
        // The terminal item acts on the PANE's folder, so it asks the pane's volume,
        // not the right-clicked row.
        canOpenTerminalHere: canOpenTerminalIn(capabilitiesFor(volumeId).kind),
        // "Share…" acts on the ROWS, so it asks about the row: a snapshot pane has no
        // folder to `cd` into but lists real files, and an archive's insides are the
        // other way round.
        canShare: rowsAreOsPaths,
        // The tag colors write an xattr through the row's path, so they ask the same
        // question: a phone or a protocol-only server would take the click and store nothing.
        canTag: rowsAreOsPaths,
        // "Add to favorites" acts on the right-clicked FOLDER, and asks a stricter
        // question than sharing does: not just whether the OS can read it now, but
        // whether it's still there next launch. A snapshot row fails exactly there.
        canFavorite: paneFolderCanBeFavorited(volumeId, entry.path),
        // A FILE on a volume that mints links (S3), not inside an archive.
        canShareLink: rowCanShareLink(volumeId, entry),
      },
      { countText: contextMenuCountText(paths.length), sizeText: contextMenuSizeText(sizeBytes) },
      boundShortcuts(),
      anchor,
      // The `Selection >` submenu's live label, read off the row the menu is opening over.
      // Both paths here put the cursor on that row first (the view's `oncontextmenu` selects it,
      // `⌃⏎` passes the cursor row), so this is what `selectSameKind` would act on. ❗ Computed
      // fresh, ❌ never the menu bar's 200 ms-debounced value.
      sameKindTargetFor(entry),
    )
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
