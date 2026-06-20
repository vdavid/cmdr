import type { FileEntry, FriendlyError, NetworkHost, ShareInfo } from '../types'
import type { DragAutoScrollFrameResult, DragAutoScrollPointer } from '../drag/drag-auto-scroll'

/** State snapshot for swapping panes without backend calls. */
export interface SwapState {
  currentPath: string
  listingId: string
  totalCount: number
  cursorIndex: number
  selectedIndices: number[]
  lastSequence: number
}

/** Typed interface for FilePane's exported methods. */
export interface FilePaneAPI {
  toggleVolumeChooser(): void
  isVolumeChooserOpen(): boolean
  closeVolumeChooser(): void
  openVolumeChooser(): void
  handleVolumeChooserKeyDown(e: KeyboardEvent): boolean

  getListingId(): string
  isLoading(): boolean
  /**
   * Resolves when the current load (if any) settles. Used by callers that need
   * a stable `listingId` for a backend call (for example, the MCP `move_cursor`
   * tool) to avoid the race where the FE has set a fresh `listingId` but
   * `list_directory_start_streaming` hasn't yet inserted the listing into the
   * backend's `LISTING_CACHE`. Resolves immediately when no load is pending.
   */
  whenLoadSettles(): Promise<void>
  getFilenameUnderCursor(): string | undefined
  /** Reactive: reads the entry-under-cursor `$state`, so `$effect`s tracking this stay subscribed. */
  getPathUnderCursor(): string | undefined
  /** Full FileEntry under the cursor (incl. `..` synthetic entry), or null. */
  getCursorEntry(): FileEntry | null
  /** Cursor target inside the network view (host or share), or null. */
  getNetworkCursorEntry(): NetworkCursorEntry | null
  setCursorIndex(index: number): Promise<void>
  getCursorIndex(): number
  /** Total cursor-addressable rows (incl. the `..` row; snapshot count for snapshot panes). */
  getEffectiveTotalCount(): number
  /** Awaitable, immediate MCP state push (skips the debounce). See FilePane.svelte. */
  syncStateToMcpNow(): Promise<void>
  /**
   * Queues a "land the cursor on this filename once the next directory-diff
   * applies" intent. Used by mkdir/mkfile/rename to defeat the structural
   * cursor-shift the diff handler would otherwise apply when an entry is
   * inserted at or above the cursor's index.
   */
  setPendingCursorName(name: string | null): void
  isInNetworkView(): boolean
  hasParentEntry(): boolean
  getCurrentPath(): string
  getVolumeId(): string
  isMtp(): boolean
  getSwapState(): SwapState
  adoptListing(state: SwapState): void

  findNetworkItemIndex(name: string): number
  refreshNetworkHosts(): void
  setNetworkHost(host: NetworkHost | null): void
  /** Queue a share name to auto-mount once the share browser is ready. */
  setNetworkAutoMount(shareName: string | undefined): void

  getSelectedIndices(): number[]
  isAllSelected(): boolean
  setSelectedIndices(indices: number[]): void
  clearSelection(): void
  selectAll(): void
  toggleSelectionAtCursor(): void
  toggleSelectionAndMoveDownAtCursor(): void
  selectRange(startIndex: number, endIndex: number): void
  /** Bulk-add or bulk-remove indices (used by the Selection dialog at commit time). */
  applyIndices(idxs: number[], mode: 'add' | 'remove'): void
  /**
   * Snapshot of the pane's entries for the Selection dialog. Indices in the
   * returned array match the pane's selection-state indices (`..` row included
   * at index 0 when `hasParent`).
   */
  getEntriesSnapshot(): Promise<import('../types').FileEntry[]>
  /** Cursor index inside the entries-snapshot returned by `getEntriesSnapshot()`. */
  getEntriesCursorIndex(): number
  snapshotSelectionForOperation(): Promise<void>
  clearOperationSnapshot(): string[] | 'all' | null

  isRenaming(): boolean
  startRename(): void
  cancelRename(): void

  refreshView(): void
  refreshVolumeSpace(): Promise<void>
  refreshIndexSizes(): void

  navigateToParent(): Promise<boolean>
  navigateToPath(path: string, selectName?: string): Promise<void>
  handleCancelLoading(): void

  handleKeyDown(e: KeyboardEvent): void
  handleKeyUp(e: KeyboardEvent): void

  /** Opens the entry under the cursor; awaits directory load or OS handoff. */
  openCursorItem(): Promise<void>

  /** Type-to-jump: route one printable keystroke into the pane's buffer. */
  handleJumpKeystroke(char: string): void
  /** Type-to-jump: true while the buffer has content (before the reset timeout empties it). */
  isJumpActive(): boolean
  /** Type-to-jump: clear the buffer + hide the indicator immediately. */
  clearJumpState(): void

  /** Debug only: inject a FriendlyError into this pane's error state. */
  injectError(friendly: FriendlyError): void
  /** Reactive: true when the pane is rendering a full-pane error (FriendlyError or `unreachable` banner). */
  isInErrorState(): boolean
  /** Native drag auto-scroll: scrolls one animation frame when the pointer is in this pane's edge band. */
  autoScrollDuringDrag(position: DragAutoScrollPointer, elapsedMs: number): DragAutoScrollFrameResult
}

/** Typed interface for BriefList/FullList exported methods used by FilePane. */
export interface ListViewAPI {
  scrollToIndex(index: number): void
  refreshIndexSizes(): void
  getEntryAt(globalIndex: number): FileEntry | undefined
  /** BriefList only */
  handleKeyNavigation?(key: string, event?: KeyboardEvent): { newIndex: number; overflow: boolean } | undefined
  /** BriefList only: refetch per-column text widths after a listing change. */
  refetchColumnWidths?(): void
  /** FullList only */
  getVisibleItemsCount?(): number
  /** Native drag auto-scroll: scrolls one animation frame when the pointer is in this list's edge band. */
  autoScrollDuringDrag?(position: DragAutoScrollPointer, elapsedMs: number): DragAutoScrollFrameResult
}

/**
 * Typed interface for VolumeBreadcrumb's exported methods.
 * @public consumed via `import type` from FilePane.svelte; knip's Svelte parser misses type-only imports
 */
export interface VolumeBreadcrumbAPI {
  toggle(): void
  getIsOpen(): boolean
  close(): void
  open(): void
  handleKeyDown(e: KeyboardEvent): boolean
}

/** Typed interface for NetworkBrowser/ShareBrowser shared methods. */
export interface BrowserAPI {
  handleKeyDown(e: KeyboardEvent): boolean
  setCursorIndex(index: number): void
  findItemIndex(name: string): number
  openCursorItem(): void
}

/** Typed interface for NetworkBrowser's exported methods (extends BrowserAPI with refresh). */
export interface NetworkBrowserAPI extends BrowserAPI {
  refresh(): void
  /** Host under cursor; `null` when cursor sits on "Connect to server…" or list is empty. */
  getHostUnderCursor(): NetworkHost | null
}

/** Typed interface for ShareBrowser. */
export interface ShareBrowserAPI extends BrowserAPI {
  /** Share under cursor; `null` when login form is up or list is empty. */
  getShareUnderCursor(): ShareInfo | null
}

/** Typed interface for SearchResultsView's exported methods. */
export interface SearchResultsViewAPI {
  setCursorIndex(index: number): void
  findItemIndex(name: string): number
  openCursorItem(): void
  isMissing(): boolean
}

/**
 * Typed interface for NetworkMountView's exported methods.
 * @public consumed via `import type` from FilePane.svelte; knip's Svelte parser misses type-only imports
 */
export interface NetworkMountViewAPI {
  handleKeyDown(e: KeyboardEvent): void
  setCursorIndex(index: number): void
  findItemIndex(name: string): number
  openCursorItem(): void
  refreshNetworkHosts(): void
  setNetworkHost(host: NetworkHost | null): void
  /**
   * Returns what the cursor is on inside the network view:
   * - `'host'` (host list, cursor on a real host),
   * - `'share'` (share list, cursor on a share),
   * - `null` (anywhere else: connect row, login form, mounting state, error).
   */
  getNetworkCursorEntry(): NetworkCursorEntry | null
}

/** Cursor target inside the network browser stack, returned by NetworkMountView. */
export type NetworkCursorEntry = { kind: 'host'; host: NetworkHost } | { kind: 'share'; share: ShareInfo }
