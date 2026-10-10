/**
 * Shared interface for DualPaneExplorer's exported methods.
 * Used by +page.svelte, command-dispatch.ts, and mcp-listeners.ts.
 */

import type { CompareDirectoriesMode } from '$lib/tauri-commands'
import type { ViewMode } from '$lib/app-status-store'
import type { McpSelectMode, McpTabAction, ConfirmDialogType } from '$lib/commands'
import type { TabMoveRequest } from '$lib/file-explorer/pane/tab-operations'
import type { MoveTabResult } from '$lib/file-explorer/tabs/tab-state-manager.svelte'
import type { QuickLookKeyEventPayload } from '$lib/file-explorer/quick-look/quick-look-state.svelte'
import type { FileEntry, FriendlyError, NetworkHost, TransferOperationType } from '$lib/file-explorer/types'
import type { AdoptedOperationData, ForegroundOperationVerdict } from '$lib/file-explorer/pane/dialog-props'
import type { NavigateIntent, NavigateResult } from '$lib/file-explorer/pane/navigate'
import type { VolumeSelectOutcome } from '$lib/file-explorer/pane/volume-selection'
import type { FavoriteOpenedEvent } from '$lib/file-explorer/navigation/favorites-analytics'
import type {
  CopyPathBetweenPanesArgs,
  OpenDeleteDialogArgs,
  OpenTransferDialogArgs,
  StartRenameOptions,
} from '$lib/file-explorer/pane/types'
import type { Initiator } from '$lib/tauri-commands'
import type { HubRow } from '$lib/file-explorer/network/servers-hub-rows'
import type { SmbHandOff } from '$lib/servers/open-sign-in'

/**
 * Closed action set for `handleSelectionAction` (the selection sub-dispatcher).
 * `clear` and `deselectAll` both clear; `selectRange` uses the index args.
 */
export type SelectionAction =
  | 'clear'
  | 'deselectAll'
  | 'selectAll'
  | 'invert'
  | 'selectSameKind'
  | 'toggleAtCursor'
  | 'toggleAtCursorAndMoveDown'
  | 'selectRange'

/** Args for `handleSelectionAction`. `startIndex`/`endIndex` only matter for `'selectRange'`. */
export interface SelectionActionArgs {
  action: SelectionAction
  startIndex?: number
  endIndex?: number
}

export interface ExplorerAPI {
  refocus: () => void
  switchPane: () => void
  swapPanes: () => void
  /** Compare directories (⇧F2): mark what differs between the two panes. */
  compareDirectories: (mode: CompareDirectoriesMode) => Promise<void>
  /** Calculate folder sizes in the focused pane (⌥⇧⏎); resolves when the count ends. */
  calculateFolderSizes: () => Promise<void>
  copyPathBetweenPanes: (args: CopyPathBetweenPanesArgs) => void
  toggleVolumeChooser: (pane: 'left' | 'right') => void
  openVolumeChooser: () => void
  /** Open/toggle the focused pane's favorites menu (⌃D), the `favorites.open` command. */
  toggleFavoritesMenu: () => void
  closeHeaderMenus: () => void
  setViewMode: (mode: ViewMode, pane?: 'left' | 'right') => void
  /**
   * Sets a specific pane's view mode in response to a native-menu click
   * (`view-mode-changed`). Same set + persist as `setViewMode`, but WITHOUT
   * `pushViewMenuState`: the menu already toggled its own CheckMenuItem on click,
   * so pushing the state back would double-sync against Rust's
   * `sync_view_mode_check_states`. Focus-preserving (the target pane changes even
   * when the other pane is focused).
   */
  setViewModeFromMenu: (pane: 'left' | 'right', mode: ViewMode) => void
  /**
   * The single coordinator-level navigation entry. Replaces the old
   * `navigate(action)` + `navigateToPath(pane, path)` pair: pass a typed
   * `NavigateIntent` (volume/path change, history walk, or snapshot open) and get
   * back a `NavigateResult` — `{ status: 'started', settled }` or
   * `{ status: 'refused', reason }` whose `reason.message` is the exact refusal
   * string the MCP adapter forwards verbatim (L12).
   */
  navigate: (intent: NavigateIntent) => NavigateResult
  /**
   * Opens the home folder in `pane` (the focused pane when omitted), resolving
   * the default volume first and clearing any `unreachable` banner on the way.
   * Shares the `handleOpenHome` edge-flow with the unreachable banner and the
   * error pane's "Go to home folder" button, so all three land identically.
   */
  goHome: (pane?: 'left' | 'right') => Promise<void>
  /**
   * Opens the root of what `pane` shows (the focused pane when omitted): its
   * volume's root, or its archive's when it's inside one. Rules:
   * `navigation/root-folder.ts`.
   */
  goToRoot: (pane?: 'left' | 'right') => Promise<void>
  getFileAndPathUnderCursor: () => { path: string; filename: string } | null
  /**
   * The path the "copy path" command copies: the cursor entry's path, or the pane's
   * own directory when the cursor sits on `..`. Null when no row is under the cursor.
   * Separate from `getFileAndPathUnderCursor` on purpose: every other under-cursor arm
   * (open, rename, Get Info, …) must keep treating `..` as "no entry".
   */
  getPathToCopyUnderCursor: () => string | null
  /**
   * The focused pane's cursor row as "Open terminal here" reads it: name, path, and
   * whether it's a folder. `..` comes back as a real row (the command treats it as
   * "the pane's own folder"), which is why this isn't `getFileAndPathUnderCursor`.
   * Null when no row is under the cursor. Rules: `$lib/open-terminal/terminal-target.ts`.
   *
   * Async because it RE-READS the row: the displayed cursor entry is one IPC behind
   * a cursor move, and a command that acts on the row can't be.
   */
  getCursorRowForTerminal: () => Promise<{ name: string; path: string; isDirectory: boolean } | null>
  /**
   * Toggles a Finder system color tag (index 1..=7, grey…orange) on the focused
   * pane's selection, or on the cursor entry when nothing is selected. Resolves the
   * paths + the pane's listing id and calls the `toggle_tags` IPC, which writes and
   * refreshes the cache. macOS-only in effect (the backend no-ops elsewhere). The
   * context-menu circles use a separate Rust-side path on the right-clicked set.
   */
  toggleTagOnFocusedSelection: (color: number) => Promise<void>
  sendKeyToFocusedPane: (key: string) => void
  /**
   * Routes a key event received from the native Quick Look panel back into the
   * focused pane's navigation primitives. Used while the panel is key (the panel
   * delegate forwards keys it didn't want via the `quick-look-key` Tauri event).
   * Implementation keeps this narrow: arrow / page / home / end / type-to-jump
   * letters; everything else is ignored. Shift+Space close is handled by the
   * listener directly, not via this method.
   */
  routePanelKey: (payload: QuickLookKeyEventPayload) => void
  openItemUnderCursor: () => Promise<void>
  /**
   * Opens the focused pane's context menu on its CURSOR row (`⌃⏎`), anchored there
   * instead of at the pointer. Same menu and the same
   * "inside the selection → act on the selection" rule as a right-click, which it
   * shares a code path with.
   */
  openContextMenuAtCursor: () => Promise<void>
  setSortColumn: (column: 'name' | 'extension' | 'size' | 'modified' | 'created', pane?: 'left' | 'right') => void
  setSortOrder: (order: 'asc' | 'desc' | 'toggle', pane?: 'left' | 'right') => void
  setSort: (
    column: 'name' | 'extension' | 'size' | 'modified' | 'created',
    order: 'asc' | 'desc',
    pane: 'left' | 'right',
  ) => Promise<void>
  getFocusedPane: () => 'left' | 'right'
  /**
   * Shifts keyboard focus to `pane`. Same store update the pane-switch paths use,
   * without re-anchoring DOM focus. The downloads "jump to file" flow uses it to
   * focus a pane that already shows the target dir instead of navigating.
   */
  setFocusedPane: (pane: 'left' | 'right') => void
  /**
   * The pane's ACTIVE-tab location: its volume id, the volume's mount path, and
   * the current directory. Lets a caller decide whether a pane already shows a
   * given dir (volume-safe: a local path on a real local volume, not an MTP or
   * network volume reporting a same-looking string). Background tabs are ignored.
   */
  getPaneLocation: (pane: 'left' | 'right') => { volumeId: string; volumePath: string; path: string }
  /**
   * The pane's live backend listing handle, or `null` before its first listing
   * settles (and on a snapshot pane, which has no backend listing). It's
   * pane-owned, not something a directory produces, so anything that needs one —
   * conflict lookups, directory-diff filtering, `refreshListing` — has to take it
   * from the pane showing that directory.
   */
  getPaneListingId: (pane: 'left' | 'right') => string | null
  /**
   * Whether the pane's listing is mid-load. Paired with `getPaneListingId` it tells a
   * navigation that has come to rest from one still in flight, which the MCP
   * `nav_to_path` adapter needs after a volume switch: that arm commits the destination
   * optimistically, so the pane reports the target long before it has been there.
   */
  isPaneLoading: (pane: 'left' | 'right') => boolean
  /**
   * Whether the pane's folder stopped answering mid-read (`listing-stalled`). Its load
   * stays in flight and retries, so the MCP navigation adapters answer on this rather
   * than waiting for a rest that may be minutes away.
   */
  isPaneStalled: (pane: 'left' | 'right') => boolean
  /**
   * Switch a pane to a volume by name. Resolves once the switch has committed, with the
   * volume it chose and `navigate()`'s result; the pane's final folder is decided when
   * that result's `corrected` resolves.
   */
  selectVolumeByName: (pane: 'left' | 'right', name: string) => Promise<VolumeSelectOutcome>
  /**
   * The identity-safe twin of `selectVolumeByName`, used when names can collide. `picked`
   * names the surface when the id is a favorite's (the Dock tile menu); a command by default.
   */
  selectVolumeById: (
    pane: 'left' | 'right',
    volumeId: string,
    picked?: FavoriteOpenedEvent,
  ) => Promise<VolumeSelectOutcome>
  handleSelectionAction: (args: SelectionActionArgs) => void
  handleMcpSelect: (pane: 'left' | 'right', start: number, count: number | 'all', mode: McpSelectMode) => Promise<void>
  /**
   * By-name selection for the MCP `select` tool's `names` mode. Throws when the
   * pane is unavailable or any name isn't in the listing (the MCP adapter
   * forwards the message as the round-trip error).
   */
  handleMcpSelectNames: (pane: 'left' | 'right', names: string[], mode: McpSelectMode) => Promise<void>
  /**
   * Flush a pane's state to the backend `PaneStateStore` without moving the
   * cursor or changing the selection (the freshness `handleMcpSelect` /
   * `moveCursor` push as a side effect). Name-resolving MCP tools (`tag`) call
   * this so a bare `nav` doesn't leave them resolving against stale state.
   */
  syncPaneStateToMcp: (pane: 'left' | 'right') => Promise<void>
  /**
   * Per-pane tab action from the MCP `tab` tool. Targets a SPECIFIC pane (and
   * optionally a specific tab), unlike the focused-pane `newTab`/`cycleTab`/etc.
   */
  handleMcpTabAction: (pane: 'left' | 'right', action: McpTabAction, tabId?: string, pinned?: boolean) => void
  /**
   * Moves a tab to another slot or to the other pane, under the same rules as a tab
   * drag, and says what happened. The MCP `tab move` action; the mouse goes through the
   * tab drag controller instead.
   */
  moveTab: (request: TabMoveRequest) => MoveTabResult
  /** Push both panes' tab lists to the backend now, past the mirror's debounce. */
  syncTabsToMcp: () => Promise<void>
  startRename: (options?: StartRenameOptions) => void
  openCopyDialog: (args?: OpenTransferDialogArgs) => Promise<void>
  /** Copies the focused pane's selection (or cursor item) into the folder it
   *  already lives in. No dialog, no rename editor: `file-operation-commands.ts`. */
  duplicateInPlace: () => Promise<void>
  openMoveDialog: (args?: OpenTransferDialogArgs) => Promise<void>
  openCompressDialog: (args?: OpenTransferDialogArgs) => Promise<void>
  copyToClipboard: () => Promise<void>
  cutToClipboard: () => Promise<void>
  pasteFromClipboard: (forceMove: boolean) => Promise<void>
  openNewFolderDialog: (name?: string, pane?: 'left' | 'right', initiator?: Initiator) => Promise<void>
  openNewFileDialog: (name?: string, pane?: 'left' | 'right', initiator?: Initiator) => Promise<void>
  createFolderDirect: (name: string, pane?: 'left' | 'right', initiator?: Initiator) => Promise<void>
  createFileDirect: (name: string, pane?: 'left' | 'right', initiator?: Initiator) => Promise<void>
  openDeleteDialog: (args: OpenDeleteDialogArgs) => Promise<void>
  /**
   * Shows an operation that is already running in the main window's progress
   * dialog (the queue row's Show button, over `foreground-operation`). The
   * verdict says whether it landed: `busy` means this window is showing another
   * operation's dialog and said so in a toast.
   */
  foregroundOperation: (operation: AdoptedOperationData) => ForegroundOperationVerdict
  closeConfirmationDialog: () => void
  confirmDialog: (dialogType: ConfirmDialogType, onConflict?: string) => void
  isConfirmationDialogOpen: () => boolean
  isRenaming: () => boolean
  /**
   * Whether a header menu — the volume switcher or the favorites menu — is open on either
   * pane. The favorites menu hosts the inline rename input, so `dialogsOnScreen()` reads
   * this and the dialog gate suppresses pane/global shortcuts while one is open (so
   * text-editing keys reach the textbox instead of the panes).
   */
  isHeaderMenuOpen: () => boolean
  openViewerForCursor: () => Promise<void>
  /**
   * Open a search-results snapshot in the target pane (defaults to focused).
   * The snapshot must already exist in `$lib/search/snapshot-store.svelte`; the
   * caller is responsible for `getOrCreate` + `setLastAttemptId` (the
   * SearchDialog's "Open in pane" handler does both). Routes through
   * `navigate({ to: { snapshot } })` so pinned-tab fork, focus, and history push
   * all apply.
   */
  openSearchSnapshotInPane: (snapshotId: string, pane?: 'left' | 'right') => void
  moveCursor: (pane: 'left' | 'right', to: number | string) => Promise<void>
  scrollTo: (pane: 'left' | 'right', index: number) => void
  refreshPane: () => Promise<void>
  refreshNetworkHosts: () => void
  /** Takes the focused pane to the servers hub (the `servers.show` command). */
  showServersInFocusedPane: () => void
  /** An SMB add's hand-off: the saved host's share list in the focused pane, mounting the share it named. */
  openSmbHandOffInFocusedPane: (handOff: SmbHandOff) => void
  /**
   * The hub row under the focused pane's cursor, or `null` when that pane isn't
   * on the hub. What the servers commands aim at before they fall back to the
   * pane's own volume.
   */
  getFocusedPaneServerRow: () => HubRow | null
  /**
   * Any hub row under the focused pane's cursor, an SMB host's included, or `null`
   * off the hub. Edit and Rename there are "Edit server…" (`editHubRow`).
   */
  getFocusedPaneHubRow: () => HubRow | null
  /** The host whose share list the focused pane shows, or `null`. */
  getFocusedPaneNetworkHost: () => NetworkHost | null
  injectError: (pane: 'left' | 'right', friendly: FriendlyError) => void
  resetError: (pane: 'left' | 'right' | 'both') => void
  /** E2E only: drive the native drag-and-drop drop entry programmatically (real
   *  OS drag can't be synthesized in Playwright). Wired only behind the E2E gate
   *  in `+page.svelte`; never reachable in production.
   *
   *  `recordedIdentity` models an IN-APP self-drag: the drop builds its transfer
   *  from the recorded source volume + the paths the volume knows (volume-relative
   *  for MTP), exactly as a real self-drag does, instead of resolving the
   *  pasteboard paths. Omit it to model a genuine EXTERNAL drop (local absolute
   *  paths through the resolver). */
  triggerFileDrop: (
    paths: string[],
    targetPane: 'left' | 'right',
    targetFolderPath?: string,
    operation?: TransferOperationType,
    recordedIdentity?: { sourceVolumeId: string; sourcePaths: string[] },
  ) => void
  newTab: () => boolean
  closeActiveTabWithConfirmation: () => Promise<'closed' | 'last-tab' | 'cancelled'>
  reopenLastClosedTab: () => 'reopened' | 'empty' | 'cap'
  cycleTab: (direction: 'next' | 'prev') => void
  togglePinActiveTab: () => void
  closeOtherTabs: () => void
  /**
   * Bulk-applies matched indices to the focused pane's selection set. Used by the
   * Selection dialog on commit.
   */
  applyIndicesToFocusedPane: (idxs: number[], mode: 'add' | 'remove') => void
  /**
   * Returns a snapshot of the focused pane's entries + cursor index for the Selection
   * dialog. Captured ONCE at dialog open; the dialog does not refresh on mid-dialog
   * focused-pane change.
   */
  getFocusedPaneEntries: () => Promise<{
    entries: FileEntry[]
    cursorIndex: number
    isSnapshotPane: boolean
  }>
}
