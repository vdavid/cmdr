import { findFileIndex } from '$lib/tauri-commands'
import { resolveSearchableFolder } from '$lib/search/searchable-folder'
import { isTypeToJumpChar, isTypeToJumpResetKey } from './type-to-jump-keys'
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
  function confirmDialog(dialogType: string, onConflict?: string) {
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

  /** Returns the current directory path of the focused pane. */
  function getFocusedPanePath(): string {
    return access.getPanePath(access.getFocusedPane())
  }

  /**
   * Returns the "current folder" the Search dialog's `Search in → Use current folder`
   * action should act on. Round-2 D12: when the focused pane is a `search-results://`
   * snapshot, walks back through history for the most recent real folder; when none is
   * available, surfaces a disabled state with a tooltip. See
   * `lib/search/searchable-folder.ts` for the pure helper this delegates to.
   */
  function getFocusedPaneSearchableFolder(): {
    path: string | null
    disabled: boolean
    disabledReason: string
  } {
    const history = access.getPaneHistory(access.getFocusedPane())
    return resolveSearchableFolder({
      currentPath: access.getPanePath(access.getFocusedPane()),
      history: history.stack.map((e) => e.path),
    })
  }

  /** Volume id of the focused pane's active tab. Used by Quick Look's IPC gate. */
  function getFocusedPaneVolumeId(): string {
    return access.getPaneVolumeId(access.getFocusedPane())
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
   * Handle selection action from MCP.
   * @param action - The selection action (clear, selectAll, deselectAll, toggleAtCursor, toggleAtCursorAndMoveDown, selectRange)
   * @param startIndex - Start index for range selection
   * @param endIndex - End index for range selection
   */
  function handleSelectionAction(action: string, startIndex?: number, endIndex?: number) {
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
   * `isSnapshotPane` flags `search-results://` panes so the dialog renders
   * the banner ("Matching what is shown in the list…").
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
      isSnapshotPane: pane.getVolumeId() === 'search-results',
    }
  }

  /** Check if an MTP path matches the pane's current volume. Returns an error string if not. */
  function validateMtpNavigation(path: string, volumeId: string, volumeName: string | undefined): string | null {
    if (path.startsWith('mtp://')) {
      const mtpMatch = path.match(/^mtp:\/\/([^/]+)\/(\d+)/)
      const pathDeviceId = mtpMatch?.[1]
      const pathStorageId = mtpMatch?.[2]
      if (!pathDeviceId || !pathStorageId || volumeId !== `${pathDeviceId}:${pathStorageId}`) {
        return `Pane is not on this MTP volume \u2014 call select_volume first.`
      }
    } else if (volumeId.includes(':') && volumeId.startsWith('mtp-')) {
      return `Pane is on the ${volumeName ?? volumeId} MTP volume. Use select_volume to switch to a local volume first.`
    }
    return null
  }

  async function moveCursorByName(paneRef: FilePaneAPI, name: string) {
    const inNetwork: boolean = paneRef.isInNetworkView()
    if (inNetwork) {
      // Network views handle name lookup locally
      const idx: number = paneRef.findNetworkItemIndex(name)
      if (idx >= 0) {
        await paneRef.setCursorIndex(idx)
      }
    } else {
      await moveCursorByNameInFileListing(paneRef, name)
    }
  }

  async function moveCursorByNameInFileListing(paneRef: FilePaneAPI, name: string) {
    const listingId: string = paneRef.getListingId()
    if (!listingId) return

    const backendIndex = await findFileIndex(listingId, name, access.getShowHiddenFiles())
    if (backendIndex === null) return

    // Backend index doesn't include ".." entry, but frontend does
    const hasParent: boolean = paneRef.hasParentEntry()
    const frontendIndex = hasParent ? backendIndex + 1 : backendIndex
    await paneRef.setCursorIndex(frontendIndex)
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
   * Refresh the focused pane.
   * Used by MCP refresh tool.
   */
  function refreshPane() {
    const paneRef = access.getPaneRef(access.getFocusedPane())
    paneRef?.refreshView()
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

  /** Debug only: open the TransferErrorDialog with a synthetic error carrying the given FriendlyError. */
  function triggerTransferError(friendly: FriendlyError) {
    const error: WriteOperationError = {
      type: 'io_error',
      path: '/debug/preview',
      message: friendly.title,
    }
    dialogs.handleTransferError(error, friendly)
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
  function handleMcpSelect(pane: 'left' | 'right', start: number, count: number | 'all', mode: string) {
    const paneRef = access.getPaneRef(pane)
    if (!paneRef) return

    // Get current selection for add/subtract modes (local Set, not reactive state)

    const currentSelection = new Set<number>(paneRef.getSelectedIndices())

    if (count === 0) {
      // Clear selection
      paneRef.setSelectedIndices([])
      return
    }

    if (count === 'all') {
      // Select all
      paneRef.selectAll()
      return
    }

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

  return {
    confirmDialog,
    toggleVolumeChooser,
    openVolumeChooser,
    closeVolumeChooser,
    getFileAndPathUnderCursor,
    sendKeyToFocusedPane,
    openItemUnderCursor,
    getFocusedPane,
    getFocusedPanePath,
    getFocusedPaneSearchableFolder,
    getFocusedPaneVolumeId,
    routePanelKey,
    handleSelectionAction,
    applyIndicesToFocusedPane,
    getFocusedPaneEntries,
    validateMtpNavigation,
    moveCursorByName,
    moveCursorByNameInFileListing,
    scrollTo,
    refreshPane,
    injectError,
    resetError,
    triggerTransferError,
    refreshNetworkHosts,
    handleMcpSelect,
  }
}
