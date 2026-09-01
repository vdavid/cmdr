/**
 * Where a keystroke goes once a file pane has focus. `DualPaneExplorer` forwards
 * every `keydown` here; this decides whether the pane consumes it and, if so,
 * which handler runs.
 *
 * The order is the contract:
 * 1. an active inline rename swallows everything (the editor owns the keyboard),
 * 2. the network and search-results views take the whole event,
 * 3. open / parent (Enter, ⌘↓, Backspace, ⌘↑) run above the view-mode split, so
 *    every view gets them,
 * 4. the Selection dialog's bare `+` / `-`,
 * 5. the four selection commands,
 * 6. whatever is left goes to the Brief or Full cursor handler.
 *
 * The classifiers themselves live in siblings (`selection-dialog-keys.ts`,
 * `selection-keys.ts`, `cursor-nav-keys.ts`); this module owns the routing and
 * the `preventDefault` / `stopPropagation` calls that go with each arm.
 */

import type { FileEntry } from '../types'
import type { ViewMode } from '$lib/app-status-store'
import type { CommandId } from '$lib/commands'
import { classifySelectionDialogKey } from './selection-dialog-keys'
import { classifySelectionKey } from './selection-keys'
import { eventMatchesCommand } from '$lib/shortcuts'
import { maybeShowQuickLookHint } from '../quick-look/quick-look-hint'
import { cancelClickToRename } from '../rename/rename-activation'

export interface PaneKeyRouterDeps {
  getRenameActive: () => boolean
  getIsNetworkView: () => boolean
  getIsSearchResultsView: () => boolean
  getViewMode: () => ViewMode
  /** Whether the synthetic `..` row exists (no parent to go to without it). */
  getHasParent: () => boolean
  /** The entry the cursor sits on, read off the active list view. */
  getEntryUnderCursor: () => FileEntry | undefined
  /** Forward to `NetworkMountView`, which owns its own cursor. */
  handleNetworkKeyDown: (event: KeyboardEvent) => void
  /** Forward to the search-results pane's key handling. */
  handleSearchResultsKeyDown: (event: KeyboardEvent) => void
  handleBriefModeKeys: (event: KeyboardEvent) => void
  handleFullModeKeys: (event: KeyboardEvent) => void
  /** Open an entry exactly like Enter does (navigate in, or hand to the OS). */
  openEntry: (entry: FileEntry) => void
  navigateToParent: () => void
  /** Bubble a high-level command id out of the pane (the Selection dialog). */
  onCommand?: (commandId: CommandId) => void
  toggleSelectionAtCursor: () => void
  toggleSelectionAndMoveDown: () => void
  selectAll: () => void
  deselectAll: () => void
  invertSelection: () => void
  /** End the mouse Shift+click anchor gesture. */
  clearRangeState: () => void
}

export interface PaneKeyRouter {
  handleKeyDown: (event: KeyboardEvent) => void
  handleKeyUp: (event: KeyboardEvent) => void
}

export function createPaneKeyRouter(deps: PaneKeyRouterDeps): PaneKeyRouter {
  /**
   * Bare `+` / `-` open the Selection dialog. Dispatch lives at the pane keyboard
   * level (not menu-driven on macOS, since menu accelerators always carry ⌘). The
   * pure classifier in `selection-dialog-keys.ts` pins the exact event filter: no
   * `metaKey` / `altKey` / `ctrlKey`; `shiftKey` is intentionally NOT filtered
   * (Shift+= on US QWERTY produces `event.key === '+'`).
   */
  function handleSelectionDialogKey(e: KeyboardEvent): boolean {
    const action = classifySelectionDialogKey(e)
    if (!action) return false
    e.preventDefault()
    e.stopPropagation()
    deps.onCommand?.(action === 'open-add' ? 'selection.selectFiles' : 'selection.deselectFiles')
    return true
  }

  // The keys come from the registry (`classifySelectionKey`), so they stay
  // customizable and the match is exact — ⇧Space is Quick Look and ⌥⌘A is Ask
  // Cmdr, neither touches the selection. Every arm stops propagation so the
  // document-level dispatcher doesn't re-fire the same command (its cases there
  // exist for the palette and MCP).
  function handleSelectionKeys(e: KeyboardEvent): boolean {
    const command = classifySelectionKey(e)
    if (!command) return false

    e.preventDefault()
    e.stopPropagation()

    switch (command) {
      case 'selection.toggle':
        deps.toggleSelectionAtCursor()
        // Finder-convert education: the first time the user presses Space
        // in the file list, explain that Cmdr uses Space for selection and
        // ⇧Space for Quick Look. The selection toggle above still applies
        // normally — the toast is purely additive. Subsequent presses are
        // no-ops (the helper reads its own "shown once" persisted flag).
        maybeShowQuickLookHint()
        break
      case 'selection.toggleAndDown':
        deps.toggleSelectionAndMoveDown()
        break
      case 'selection.selectAll':
        deps.selectAll()
        break
      case 'selection.deselectAll':
        deps.deselectAll()
        break
      case 'selection.invert':
        deps.invertSelection()
        break
    }
    return true
  }

  /**
   * Open / parent keys, view-independent (handled before the Brief/Full split).
   * Returns true if the key was consumed.
   *
   * - Enter / ⌘↓ → open the entry under the cursor (Finder parity, mirror of ⌘↑).
   * - Backspace / ⌘↑ → go to the parent directory.
   *
   * Both keys resolve against the registry (`eventMatchesCommand`), so they follow a
   * rebind and match the whole combo. ⌘Backspace therefore falls out naturally: it's
   * `file.delete`'s combo, not `nav.parent`'s, so it passes through to the document
   * dispatcher and deletes.
   *
   * `stopPropagation` is load-bearing for the ⌘-variants: ⌘↓ (`nav.open`) and ⌘↑
   * (`nav.parent`) are ALSO in the dispatch map, so without stopping here the
   * document-level dispatcher would run the command a second time (⌘↑ → grandparent,
   * ⌘↓ → double-open).
   */
  function handleOpenOrParentKey(e: KeyboardEvent): boolean {
    if (eventMatchesCommand(e, 'nav.open')) {
      const entry = deps.getEntryUnderCursor()
      if (entry) {
        e.preventDefault()
        if (e.metaKey) e.stopPropagation()
        deps.openEntry(entry)
        return true
      }
      // ⌘↓ with nothing under the cursor: swallow it so it can't fall through
      // to cursor-move or the document dispatcher.
      if (e.metaKey) {
        e.preventDefault()
        e.stopPropagation()
        return true
      }
      return false
    }

    if (eventMatchesCommand(e, 'nav.parent') && deps.getHasParent()) {
      e.preventDefault()
      e.stopPropagation()
      deps.navigateToParent()
      return true
    }

    return false
  }

  function handleKeyDown(e: KeyboardEvent): void {
    // When rename is active, suppress all app-level shortcuts.
    // The InlineRenameEditor handles its own keyboard events via stopPropagation.
    // This guard handles any edge cases where events still bubble.
    if (deps.getRenameActive()) return

    // Any keyboard action cancels a pending click-to-rename timer
    cancelClickToRename()

    if (deps.getIsNetworkView()) {
      deps.handleNetworkKeyDown(e)
      return
    }

    // Search-results pane: route Enter to the cursor row's activation, arrow keys
    // through the SearchResultsView's setCursorIndex. The view embeds FullList but
    // owns its own bind ref; FilePane's `fullListRef` doesn't apply here. The
    // cursor state itself still lives on `cursorIndex` so we can clamp uniformly.
    if (deps.getIsSearchResultsView()) {
      deps.handleSearchResultsKeyDown(e)
      return
    }

    // Open (Enter / ⌘↓) and parent (Backspace / ⌘↑) — handled above the
    // view-mode split so every view gets them. See `handleOpenOrParentKey`.
    if (handleOpenOrParentKey(e)) return

    // Bare `+` / `-` open the Selection dialog (Total Commander parity).
    if (handleSelectionDialogKey(e)) return

    // Handle selection keys
    if (handleSelectionKeys(e)) return

    // Delegate to view-mode-specific handler
    if (deps.getViewMode() === 'brief') {
      deps.handleBriefModeKeys(e)
    } else {
      deps.handleFullModeKeys(e)
    }
  }

  // Terminates the mouse Shift+click anchor gesture so the next gesture starts
  // fresh. Keyboard Shift+nav is stateless and doesn't need this.
  function handleKeyUp(e: KeyboardEvent): void {
    if (e.key === 'Shift') {
      deps.clearRangeState()
    }
  }

  return { handleKeyDown, handleKeyUp }
}
