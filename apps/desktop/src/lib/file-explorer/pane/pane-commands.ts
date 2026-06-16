import { findFileIndex, findFileIndices, refreshListing } from '$lib/tauri-commands'
import type { McpSelectMode, ConfirmDialogType } from '$lib/commands'
import { isTypeToJumpChar, isTypeToJumpResetKey } from './type-to-jump-keys'
import { capabilitiesFor } from './volume-capabilities'
import type { SelectionAction } from '../../../routes/(main)/explorer-api'
import type { FilePaneAPI } from './types'
import type { FileEntry, FriendlyError, WriteOperationError } from '../types'
import type { createDialogState } from './dialog-state.svelte'
import type { PaneAccess } from './pane-access'

type DialogState = ReturnType<typeof createDialogState>

/**
 * Read-only / delegating command bodies for the MCP + palette surface, lifted
 * out of `DualPaneExplorer` so the action-routing, key-intercept, and snapshot
 * branches are headless-testable. Reads pane state through `PaneAccess`; the
 * component keeps one-line `export function` delegates for every member.
 *
 * State writers (navigation, focus, sort, volume change) stay in the component
 * this phase: extracting them would mean threading mutable state through
 * callbacks, which is the explorer-store phase's job, not this factoring.
 */
export function createPaneCommands(access: PaneAccess, dialogs: DialogState) {
  function confirmDialog(dialogType: ConfirmDialogType, onConflict?: string) {
    dialogs.confirmOpenDialog(dialogType, onConflict)
  }

  /**
   * Open/toggle volume chooser for the specified pane.
   * Closes the other pane's volume chooser to ensure only one is open at a time.
   */
  function toggleVolumeChooser(pane: 'left' | 'right') {
    access.getPaneRef(access.otherPane(pane))?.closeVolumeChooser()
    access.getPaneRef(pane)?.toggleVolumeChooser()
  }

  /**
   * Open volume chooser for the focused pane.
   * Closes the other pane's volume chooser first.
   */
  function openVolumeChooser() {
    access.getPaneRef(access.otherPane(access.getFocusedPane()))?.closeVolumeChooser()
    access.getPaneRef(access.getFocusedPane())?.openVolumeChooser()
  }

  /**
   * Close volume chooser on all panes.
   */
  function closeVolumeChooser() {
    for (const side of ['left', 'right'] as const) {
      access.getPaneRef(side)?.closeVolumeChooser()
    }
  }

  /**
   * Get the path and filename of the file under the cursor in the focused pane.
   *
   * For the search-results virtual pane the underlying entries carry their own absolute
   * `path`, and `currentPath` is the opaque `search-results://<id>` URL — concatenating
   * the two would produce `search-results://sr-1/test.md`, which downstream commands
   * (`showInFinder`, `openInEditor`, `copyToClipboard`) can't act on. Round-2 P8 / P9.
   * We prefer the pane-reported path under the cursor when present and fall back to the
   * legacy `${currentPath}/${filename}` form for regular panes.
   */
  function getFileAndPathUnderCursor(): { path: string; filename: string } | null {
    const paneRef = access.getPaneRef(access.getFocusedPane())
    const filename = paneRef?.getFilenameUnderCursor()
    if (!filename || filename === '..') return null
    const cursorPath = paneRef?.getPathUnderCursor()
    if (cursorPath) {
      return { path: cursorPath, filename }
    }
    const currentPath = access.getPanePath(access.getFocusedPane())
    return { path: `${currentPath}/${filename}`, filename }
  }

  /**
   * Simulate a key press on the focused pane (for commands like Enter to open).
   */
  function sendKeyToFocusedPane(key: string) {
    const paneRef = access.getPaneRef(access.getFocusedPane())
    const event = new KeyboardEvent('keydown', { key, bubbles: false })
    paneRef?.handleKeyDown(event)
  }

  /**
   *  Opens the entry under the focused pane's cursor and waits for it to complete.
   *  Unlike `sendKeyToFocusedPane('Enter')`, this returns a Promise so MCP can ack on
   *  real completion (directory listing finished, or OS open-with-default dispatched)
   *  rather than guessing via state-push heuristics that don't fire for OS-opened
   *  files. Used by the `open_under_cursor` round-trip in `mcp-listeners.ts`.
   */
  async function openItemUnderCursor(): Promise<void> {
    const paneRef = access.getPaneRef(access.getFocusedPane())
    if (!paneRef) throw new Error('Focused pane is not available')
    await paneRef.openCursorItem()
  }

  /**
   * Get the focused pane identifier.
   * Used by MCP context tools.
   */
  function getFocusedPane(): 'left' | 'right' {
    return access.getFocusedPane()
  }

  /**
   * Route a key event forwarded by the native Quick Look panel back into the
   * focused pane. The panel is the key window while it's open, so DOM keydowns
   * never reach our webview — `previewPanel:handleEvent:` in the Rust delegate
   * is the only path arrow keys / type-to-jump can travel.
   *
   * We synthesise a `KeyboardEvent` and hand it to the pane's `handleKeyDown`
   * directly. Plan note (specs/quick-look-plan.md "Why we forward keys via a
   * Tauri event"): re-dispatching via `paneEl.dispatchEvent` would create an
   * `isTrusted: false` event with suppressed defaults; calling the handler
   * function is the cleaner cut and matches how `sendKeyToFocusedPane` already
   * works for MCP-driven nav.
   *
   * Surface covered: ArrowUp/Down/Left/Right, PageUp/PageDown, Home/End,
   * Enter (open), Backspace (parent), and printable letters/digits for
   * type-to-jump. Shift+Space close is handled in the listener before this
   * method is called.
   *
   * Type-to-jump is intercepted by `handleKeyDown` _above_ FilePane in the
   * normal DOM path, so we mirror that intercept here: a printable letter/
   * digit goes straight to `handleJumpKeystroke`, and reset keys
   * (arrows/page/home/end/enter/tab/backspace) clear the buffer before
   * falling through to the navigation handler.
   */
  function routePanelKey(payload: {
    key: string
    code: string
    shiftKey: boolean
    metaKey: boolean
    altKey: boolean
    ctrlKey: boolean
  }) {
    const paneRef = access.getPaneRef(access.getFocusedPane())
    if (!paneRef) return
    const event = new KeyboardEvent('keydown', {
      key: payload.key,
      code: payload.code,
      shiftKey: payload.shiftKey,
      metaKey: payload.metaKey,
      altKey: payload.altKey,
      ctrlKey: payload.ctrlKey,
      bubbles: false,
    })
    // Mirror DualPaneExplorer's `handleKeyDown` type-to-jump intercept: the
    // panel-forwarded path bypasses the DOM listener, so without this the
    // pane's `handleKeyDown` would never see letters/digits as jump chars
    // (FilePane delegates type-to-jump to its parent — see the intercept
    // in this component's own `handleKeyDown` above).
    if (!paneRef.isRenaming()) {
      if (isTypeToJumpChar(event)) {
        paneRef.handleJumpKeystroke(event.key)
        return
      }
      if (isTypeToJumpResetKey(event)) {
        paneRef.clearJumpState()
        // Fall through to the navigation handler.
      }
    }
    paneRef.handleKeyDown(event)
  }

  /**
   * Handle selection action from the keyboard/palette dispatch and MCP.
   * @param action - The selection action (closed `SelectionAction` union)
   * @param startIndex - Start index for range selection
   * @param endIndex - End index for range selection
   */
  function handleSelectionAction(action: SelectionAction, startIndex?: number, endIndex?: number) {
    const paneRef = access.getPaneRef(access.getFocusedPane())
    if (!paneRef) return

    switch (action) {
      case 'clear':
      case 'deselectAll':
        paneRef.clearSelection()
        break
      case 'selectAll':
        paneRef.selectAll()
        break
      case 'toggleAtCursor':
        paneRef.toggleSelectionAtCursor()
        break
      case 'toggleAtCursorAndMoveDown':
        paneRef.toggleSelectionAndMoveDownAtCursor()
        break
      case 'selectRange':
        if (startIndex !== undefined && endIndex !== undefined) {
          paneRef.selectRange(startIndex, endIndex)
        }
        break
    }
  }

  /**
   * Bulk-apply matched indices to the focused pane's selection set. Called by
   * the Selection dialog on commit. Mode is `'add'` for "Select files…" and
   * `'remove'` for "Deselect files…". No-op when no pane is focused.
   */
  function applyIndicesToFocusedPane(idxs: number[], mode: 'add' | 'remove') {
    access.getPaneRef(access.getFocusedPane())?.applyIndices(idxs, mode)
  }

  /**
   * Returns a snapshot of the focused pane's entries + cursor index, for the
   * Selection dialog. The dialog uses this once at open-time; we
   * intentionally don't refresh on focused-pane change mid-dialog.
   * `isSnapshotPane` flags snapshot panes so the dialog renders the banner
   * ("Matching what is shown in the list…"). A snapshot pane is one whose kind
   * has no backend listing (`!caps.hasBackendListing`), read from the capability
   * table rather than a `volumeId === 'search-results'` string compare (A6).
   * The network kind is also `!hasBackendListing`, but its pane never opens the
   * Selection dialog (NetworkMountView has no file list), so this stays a
   * snapshot-only flag in practice.
   */
  async function getFocusedPaneEntries(): Promise<{
    entries: FileEntry[]
    cursorIndex: number
    isSnapshotPane: boolean
  }> {
    const pane = access.getPaneRef(access.getFocusedPane())
    if (!pane) return { entries: [], cursorIndex: 0, isSnapshotPane: false }
    const [entries, cursorIndex] = await Promise.all([
      pane.getEntriesSnapshot(),
      Promise.resolve(pane.getEntriesCursorIndex()),
    ])
    return {
      entries,
      cursorIndex,
      isSnapshotPane: !capabilitiesFor(pane.getVolumeId()).hasBackendListing,
    }
  }

  /** Returns true when the cursor landed on the named item, false when it wasn't found. */
  async function moveCursorByName(paneRef: FilePaneAPI, name: string): Promise<boolean> {
    const inNetwork: boolean = paneRef.isInNetworkView()
    if (inNetwork) {
      // Network views handle name lookup locally
      const idx: number = paneRef.findNetworkItemIndex(name)
      if (idx < 0) return false
      await paneRef.setCursorIndex(idx)
      return true
    }
    return moveCursorByNameInFileListing(paneRef, name)
  }

  /** Returns true when the cursor landed on the named item, false when it wasn't found. */
  async function moveCursorByNameInFileListing(paneRef: FilePaneAPI, name: string): Promise<boolean> {
    const listingId: string = paneRef.getListingId()
    if (!listingId) return false

    const backendIndex = await findFileIndex(listingId, name, access.getShowHiddenFiles())
    if (backendIndex === null) return false

    // Backend index doesn't include ".." entry, but frontend does
    const hasParent: boolean = paneRef.hasParentEntry()
    const frontendIndex = hasParent ? backendIndex + 1 : backendIndex
    await paneRef.setCursorIndex(frontendIndex)
    return true
  }

  /**
   * Scroll to load a region around a specific index in a large directory.
   * Used by MCP scroll_to tool.
   */
  function scrollTo(pane: 'left' | 'right', index: number) {
    const paneRef = access.getPaneRef(pane)
    // For now, just set cursor to that index - virtualization handles the rest
    void paneRef?.setCursorIndex(index)
  }

  /**
   * Refresh the focused pane: force a backend re-read of the listing, then
   * re-render. Used by the MCP refresh tool (a round-trip — throws on failure so
   * the adapter reports the real outcome). The re-read is the point: the whole
   * reason a caller refreshes is "I think the cache is stale", and a bare
   * `refreshView()` only re-renders the same stale cache. Local volumes always
   * re-read; watcher-backed MTP/SMB listings short-circuit in the backend (their
   * caches are kept fresh by `notify_mutation`, and a redundant MTP re-read costs
   * ~17 s) — see `refresh_listing` in `commands/file_system/listing.rs`.
   */
  async function refreshPane(): Promise<void> {
    const paneRef = access.getPaneRef(access.getFocusedPane())
    if (!paneRef) throw new Error('The focused pane is unavailable')
    const listingId = paneRef.getListingId()
    if (listingId) {
      const result = await refreshListing(listingId)
      if (result.timedOut) throw new Error('Refresh timed out — the volume may be unresponsive')
    }
    paneRef.refreshView()
  }

  /** Debug only: inject a FriendlyError into the specified pane. */
  function injectError(pane: 'left' | 'right', friendly: FriendlyError) {
    access.getPaneRef(pane)?.injectError(friendly)
  }

  /** Debug only: reset a pane's error state by re-navigating to its current path. */
  function resetError(pane: 'left' | 'right' | 'both') {
    if (pane === 'both' || pane === 'left') {
      void access.getPaneRef('left')?.navigateToPath(access.getPanePath('left'))
    }
    if (pane === 'both' || pane === 'right') {
      void access.getPaneRef('right')?.navigateToPath(access.getPanePath('right'))
    }
  }

  /**
   * Debug only: open the TransferErrorDialog with a synthetic typed error. The
   * dialog renders entirely from the typed error, so the preview shows the same
   * copy production does; `friendly.title` only seeds the synthetic raw message.
   */
  function triggerTransferError(friendly: FriendlyError) {
    const error: WriteOperationError = {
      type: 'io_error',
      path: '/debug/preview',
      message: friendly.title,
    }
    dialogs.handleTransferError(error)
  }

  /** Refresh network hosts in the focused pane (used by ⌘R shortcut). */
  function refreshNetworkHosts() {
    const paneRef = access.getPaneRef(access.getFocusedPane())
    paneRef?.refreshNetworkHosts()
  }

  /**
   * Handle unified select command from MCP.
   * @param pane - Which pane to select in
   * @param start - Start index (0-based)
   * @param count - Number of items to select, or 'all' for select all
   * @param mode - 'replace', 'add', or 'subtract'
   */
  async function handleMcpSelect(
    pane: 'left' | 'right',
    start: number,
    count: number | 'all',
    mode: McpSelectMode,
  ): Promise<void> {
    const paneRef = access.getPaneRef(pane)
    if (!paneRef) throw new Error(`The ${pane} pane is unavailable`)

    // Get current selection for add/subtract modes (local Set, not reactive state)
    const currentSelection = new Set<number>(paneRef.getSelectedIndices())

    if (count === 0) {
      // Clear selection
      paneRef.setSelectedIndices([])
    } else if (count === 'all') {
      // Select all
      paneRef.selectAll()
    } else {
      // Calculate the indices to select
      const endIndex = start + count - 1
      const targetIndices: number[] = []
      for (let i = start; i <= endIndex; i++) {
        targetIndices.push(i)
      }

      let newSelection: number[]
      if (mode === 'add') {
        // Add to current selection
        targetIndices.forEach((i) => currentSelection.add(i))
        newSelection = Array.from(currentSelection)
      } else if (mode === 'subtract') {
        // Remove from current selection
        targetIndices.forEach((i) => currentSelection.delete(i))
        newSelection = Array.from(currentSelection)
      } else {
        // Replace mode (default)
        newSelection = targetIndices
      }

      paneRef.setSelectedIndices(newSelection)
    }
    // Push the new selection to the backend's PaneStateStore BEFORE the round-trip
    // replies ok, so a follow-up tool call (select → copy) reads fresh state.
    await paneRef.syncStateToMcpNow()
  }

  /**
   * Select specific files by name (MCP `select` tool's `names` mode), so agents
   * don't have to map names → indexes themselves. Throws when the pane is
   * unavailable or any name isn't in the listing — the MCP adapter forwards the
   * message as the round-trip error.
   *
   * @param mode - 'replace', 'add', or 'subtract'
   */
  async function handleMcpSelectNames(pane: 'left' | 'right', names: string[], mode: McpSelectMode): Promise<void> {
    const paneRef = access.getPaneRef(pane)
    if (!paneRef) throw new Error(`The ${pane} pane is unavailable`)

    await paneRef.whenLoadSettles()
    const listingId = paneRef.getListingId()
    if (!listingId) throw new Error(`The ${pane} pane has no file listing`)

    const found = await findFileIndices(listingId, names, access.getShowHiddenFiles())
    const missing = names.filter((name) => !(name in found))
    if (missing.length > 0) {
      throw new Error(`Not found in the ${pane} pane: ${missing.join(', ')}`)
    }

    // Backend indices don't include the ".." row; frontend indices do
    const hasParent: boolean = paneRef.hasParentEntry()
    const targetIndices = names.map((name) => (hasParent ? found[name] + 1 : found[name]))

    const currentSelection = new Set<number>(paneRef.getSelectedIndices())
    let newSelection: number[]
    if (mode === 'add') {
      targetIndices.forEach((i) => currentSelection.add(i))
      newSelection = Array.from(currentSelection)
    } else if (mode === 'subtract') {
      targetIndices.forEach((i) => currentSelection.delete(i))
      newSelection = Array.from(currentSelection)
    } else {
      newSelection = targetIndices
    }
    paneRef.setSelectedIndices(newSelection)
    // Push the new selection to the backend's PaneStateStore BEFORE the round-trip
    // replies ok. Without this, select → copy reads stale (empty) selection in the
    // backend pre-check and the copy is wrongly rejected.
    await paneRef.syncStateToMcpNow()
  }

  return {
    confirmDialog,
    toggleVolumeChooser,
    openVolumeChooser,
    closeVolumeChooser,
    getFileAndPathUnderCursor,
    sendKeyToFocusedPane,
    openItemUnderCursor,
    getFocusedPane,
    routePanelKey,
    handleSelectionAction,
    applyIndicesToFocusedPane,
    getFocusedPaneEntries,
    moveCursorByName,
    moveCursorByNameInFileListing,
    scrollTo,
    refreshPane,
    injectError,
    resetError,
    triggerTransferError,
    refreshNetworkHosts,
    handleMcpSelect,
    handleMcpSelectNames,
  }
}
