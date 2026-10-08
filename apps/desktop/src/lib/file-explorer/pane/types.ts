import type { FileEntry, FriendlyError, NetworkHost, ShareInfo } from '../types'
import type { DragAutoScrollFrameResult, DragAutoScrollPointer } from '../drag/drag-auto-scroll'
import type { Initiator, ListingIndexSizesChanged, Location } from '$lib/tauri-commands'
import type { HubRow } from '../network/servers-hub-rows'
import type { FavoritesMenuOpenTrigger } from '../navigation/favorites-analytics'
import type { HistoryCursor } from '../navigation/navigation-history'
import type { PaneRowState } from './pane-row-state'
import type { ResortResult } from '../types'

/** Options for `startRename`. */
export interface StartRenameOptions {
  /**
   * Suppresses the extension-change confirmation for THIS auto-started rename
   * only (used by paste-clipboard-as-file, where renaming a fresh `pasted.txt`
   * to `notes.md` must not pop the dialog). User-initiated renames (F2) omit it
   * and keep the warning.
   */
  suppressExtensionWarning?: boolean
  /**
   * Seeds the editor with this value instead of the entry's current name. The
   * MCP `rename` tool passes the proposed `newName` so the user reviews it before
   * pressing Enter (the human-review affordance). Omitted for user-initiated
   * renames (F2), which seed the current name.
   */
  initialName?: string
  /**
   * The exact name the rename MUST activate on. For an auto-started rename
   * (paste-clipboard-as-file), the optimistic cursor move can resolve before the
   * FE row array applies the new file's synthetic diff, so the entry under the
   * cursor may still be a DIFFERENT file — activating on it would let the user's
   * next keystroke rename the wrong file. When set, `startRename` refuses to
   * activate unless the entry under the cursor is exactly this name, retries
   * briefly while the diff lands, then gives up silently (file kept, no rename).
   * Omit it for user-initiated renames (F2), which activate on the cursor entry.
   */
  expectedName?: string
}

/**
 * A deliberate volume (re)select: the target volume, its mount path, and the
 * path to land on (which may differ from the volume root for a favorite).
 * Shared by every `onVolumeChange` carrier from `VolumeBreadcrumb` down
 * through the pane's connection views to the breadcrumb-bar handlers.
 */
export interface VolumeChangePayload {
  volumeId: string
  volumePath: string
  targetPath: string
}

/**
 * A directory to load, with the entry name (if any) that should land under the
 * cursor once the listing settles. Shared by the loader's `loadDirectory` /
 * `navigateToPath` and `EntryActivationDeps.loadDirectory`: both took
 * `(path, selectName?)`, two same-typed strings a caller could drop or swap.
 */
export interface LoadDirectoryArgs {
  path: string
  selectName?: string
}

/** A directory load: the volume and path it lists, and the entry it puts under the cursor on landing. */
export interface ListingLoad extends LoadDirectoryArgs {
  volumeId: string
  /** A Back / Forward landing's remembered cursor, restored when `selectName` is absent. */
  historyCursor?: HistoryCursor
}

/** Where a history walk lands, and the cursor its entry remembers (none on a first visit's entry). */
export interface HistoryCursorTarget {
  path: string
  cursor: HistoryCursor | undefined
}

/**
 * What a cancel stopped, and what the pane last showed, bubbled from the loader up
 * through `FilePane`'s `onCancelLoading` prop. `lastShown` is the pane's last landed
 * listing (the error screen included), `null` before its first landing.
 */
export interface CancelLoadingPayload {
  cancelled: ListingLoad
  lastShown: Location | null
}

/**
 * A pane-to-pane copy: the path comes from `source` and lands in `target`.
 * Shared by `PaneMirror.copyPathBetweenPanes` and its `ExplorerAPI` re-declaration:
 * both took `(source, target)`, two same-typed `'left' | 'right'` values a caller
 * could swap.
 */
export interface CopyPathBetweenPanesArgs {
  source: 'left' | 'right'
  target: 'left' | 'right'
  /**
   * When the source pane is focused, let the cursor refine the destination (a
   * folder under the cursor opens instead of the pane's own folder). Default
   * `true`, the ⌘→ / ⌘← behavior; `pane.clone` passes `false` to copy the pane's
   * location exactly.
   */
  followCursor?: boolean
}

/**
 * Shared args for the copy / move / compress dialog openers: an optional
 * pre-answered conflict policy and MCP round-trip id, both `string`. Used by
 * `ExplorerAPI.openCopyDialog` / `.openMoveDialog` / `.openCompressDialog`, each
 * of which took `(autoConfirm?, onConflict?, mcpRequestId?, initiator?)` as four
 * positional params with `onConflict` and `mcpRequestId` sharing `string`.
 */
export interface OpenTransferDialogArgs {
  autoConfirm?: boolean
  onConflict?: string
  mcpRequestId?: string
  initiator?: Initiator
}

/**
 * Args for `ExplorerAPI.openDeleteDialog`, which took
 * `(permanent, autoConfirm?, mcpRequestId?, initiator?)` with `permanent` and
 * `autoConfirm` sharing `boolean`.
 */
export interface OpenDeleteDialogArgs {
  permanent: boolean
  autoConfirm?: boolean
  mcpRequestId?: string
  initiator?: Initiator
}

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
  openVolumeChooser(): void
  toggleFavoritesMenu(): void
  /** Whether the volume switcher OR the favorites menu is up over this pane's header. */
  isHeaderMenuOpen(): boolean
  closeHeaderMenu(): void

  getListingId(): string
  isLoading(): boolean
  /** Whether the folder stopped answering mid-read (`listing-stalled`); the load stays in flight. */
  isStalled(): boolean
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
  /**
   * Re-reads the cursor entry and returns it. `getCursorEntry` is one IPC behind a
   * cursor move, which is fine to DISPLAY and not fine to ACT on.
   */
  refreshCursorEntry(): Promise<FileEntry | null>
  /** Cursor target inside the network view (host or share), or null. */
  getNetworkCursorEntry(): NetworkCursorEntry | null
  /** The host whose share list the network view shows, or null (the Servers list, or not the network view). */
  getNetworkHost(): NetworkHost | null
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
  /**
   * A Back / Forward landing: put the cursor where it last sat in the destination
   * history entry once that entry's rows are on screen (`pane/history-cursor-sync.svelte.ts`).
   */
  restoreHistoryCursor(target: HistoryCursorTarget): void
  isInNetworkView(): boolean
  hasParentEntry(): boolean
  /**
   * The last `directory-diff` sequence applied: which state of the listing the rows
   * show. Row numbers the backend read at another sequence don't fit them.
   */
  getLastSequence(): number
  getViewGeneration(): number
  isRowStateReady(): boolean
  getRowState(): PaneRowState
  /** Installs count, cursor, and selection synchronously; row-state owns the revision. */
  applyRowResult(result: ResortResult): () => void
  getCurrentPath(): string
  getVolumeId(): string
  isMtp(): boolean
  getSwapState(): SwapState
  adoptListing(state: SwapState): void

  findNetworkItemIndex(name: string): number
  /** Cursor-addressable rows in the network view (hosts or shares), `0` outside it. */
  getNetworkItemCount(): number
  refreshNetworkHosts(): void
  setNetworkHost(host: NetworkHost | null): void
  /** Queue a share name to auto-mount once the share browser is ready. */
  setNetworkAutoMount(shareName: string | undefined): void

  getSelectedIndices(): number[]
  isAllSelected(): boolean
  setSelectedIndices(indices: number[]): void
  clearSelection(): void
  selectAll(): void
  /** Flip every selectable row; `..` stays untouched. */
  invertSelection(): void
  /**
   * ADD every row of the same kind as the one under the cursor (every folder, or
   * every file sharing its extension) to the selection, without clearing it and
   * without moving the cursor. No-op on the `..` row. Async: it re-reads the
   * cursor row and the pane's whole-listing snapshot.
   */
  selectSameKind(): Promise<void>
  /**
   * Open the native context menu on the CURSOR row (`⌃⏎`), anchored just under it
   * rather than at the pointer. Acts on the whole selection when the cursor sits
   * inside it, on that one row otherwise — the same rule a right-click follows,
   * through the same code. No-op on the servers hub, which has no file rows.
   * Async: it re-reads the cursor row first.
   */
  openContextMenuAtCursor(): Promise<void>
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
  startRename(options?: StartRenameOptions): void
  cancelRename(): void

  refreshView(): void
  refreshVolumeSpace(): Promise<void>
  refreshIndexSizes(): void
  /** Applies a pushed `listing-index-sizes-changed` for this pane's listing. */
  applyIndexSizes(change: ListingIndexSizesChanged): void

  navigateToParent(): Promise<boolean>
  /**
   * Resolves when the listing lands. Rejects on a listing error, with
   * `NavigationCancelled` when its load is cancelled, or with `NavigationSuperseded`
   * when a newer navigation takes over; only those last two are safe to drop unawaited.
   */
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

  /** Quick filter: true when typing in this pane narrows the list instead of jumping (the setting). */
  isQuickFilterMode(): boolean
  /** Quick filter: true while a pattern narrows the list. */
  isQuickFilterActive(): boolean
  /** Quick filter: append one printable character to the pattern. */
  appendQuickFilter(char: string): void
  /** Quick filter: drop the pattern's last character. */
  backspaceQuickFilter(): void
  /** Quick filter: clear the pattern and show every row again. */
  clearQuickFilter(): void

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
  /** Applies a pushed `listing-index-sizes-changed` to the cached rows (no IPC). */
  applyIndexSizes(change: ListingIndexSizesChanged): void
  getEntryAt(globalIndex: number): FileEntry | undefined
  /** The UI index of a loaded row, or `undefined` when it isn't in the window. */
  indexOfEntry(path: string): number | undefined
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
/**
 * The chip's commands, over BOTH menus it hosts (the volume switcher and the favorites
 * menu). ❗ No key handler: each menu is a house `Menu` and catches keys itself, on a
 * document capture listener that lives only while it's open.
 */
export interface VolumeBreadcrumbAPI {
  toggleVolumeChooser(): void
  openVolumeChooser(): void
  toggleFavoritesMenu(): void
  /** Whether EITHER menu is up, which is what suppresses the panes' keys behind it. */
  isHeaderMenuOpen(): boolean
  closeHeaderMenu(): void
}

/**
 * The volume switcher's own four commands, as the chip drives them.
 * @public consumed via `import type` from VolumeBreadcrumb.svelte; knip's Svelte parser misses type-only imports
 */
export interface VolumeChooserMenuAPI {
  toggle(): void
  open(): void
  close(): void
  getIsOpen(): boolean
}

/**
 * The favorites menu's, which differ in one way: opening it records WHAT brought it up,
 * so the analytics can say whether the switcher's row is how people find it.
 * @public consumed via `import type` from VolumeBreadcrumb.svelte; knip's Svelte parser misses type-only imports
 */
export interface FavoritesMenuAPI {
  toggle(trigger: FavoritesMenuOpenTrigger): void
  open(trigger: FavoritesMenuOpenTrigger): void
  close(): void
  getIsOpen(): boolean
}

/** Typed interface for ServersHub/PlacesBrowser shared methods. */
export interface BrowserAPI {
  /**
   * No "handled" return: both browsers sit below the document-level dispatcher, so a
   * claimed key is one that got `preventDefault()` + `stopPropagation()`. `NetworkMountView`
   * and `pane-key-router` hand the network view every key and return either way.
   */
  handleKeyDown(e: KeyboardEvent): void
  setCursorIndex(index: number): void
  findItemIndex(name: string): number
  openCursorItem(): void
  /** How many rows the cursor can sit on, so a caller can range-check an index. */
  getItemCount(): number
}

/** Typed interface for ServersHub's exported methods (extends BrowserAPI with refresh). */
export interface ServersHubAPI extends BrowserAPI {
  refresh(): void
  /** SMB host under the cursor; `null` on a one-place server or the "Add server…" row. */
  getHostUnderCursor(): NetworkHost | null
  /**
   * The whole row under the cursor; `null` on the "Add server…" row.
   *
   * ❗ How a command reaches what the user is looking at: the hub IS a pane, so
   * "act on the focused pane's volume" would otherwise act on the synthetic hub
   * row rather than the server the cursor is on.
   */
  getRowUnderCursor(): HubRow | null
  /** Selects a server by its saved id (or a host's discovery id), now or once it's listed. */
  selectServer(id: string): void
  /** ⌃⏎: the cursor row's menu, opened from the keyboard just under the row. */
  openContextMenuAtCursor(): Promise<void>
}

/** Typed interface for PlacesBrowser. */
export interface PlacesBrowserAPI extends BrowserAPI {
  /** Share under cursor; `null` when login form is up or list is empty. */
  getShareUnderCursor(): ShareInfo | null
}

/** Typed interface for SearchResultsView's exported methods. */
/**
 * Which view a pane renders, as a function of its capability KIND. `FilePane`'s
 * alt-view `{#if}` chain branches on it, and `pane-footer.ts` decides the status
 * footer from it.
 */
export type PaneViewKind = 'network' | 'search-results' | 'normal'

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
  /**
   * Rows the cursor can sit on in whichever browser is up, and `0` while a
   * mount is running or its failure is on screen (neither browser is mounted then).
   */
  getItemCount(): number
  refreshNetworkHosts(): void
  setNetworkHost(host: NetworkHost | null): void
  /**
   * Returns what the cursor is on inside the network view:
   * - `'host'` (host list, cursor on a real host),
   * - `'share'` (share list, cursor on a share),
   * - `null` (anywhere else: connect row, login form, mounting state, error).
   */
  getNetworkCursorEntry(): NetworkCursorEntry | null
  /** ⌃⏎ in the servers hub: the cursor row's menu. A host's places list has none yet. */
  openContextMenuAtCursor(): Promise<void>
}

/** Cursor target inside the network browser stack, returned by NetworkMountView. */
export type NetworkCursorEntry =
  /** An SMB host in the hub, with the hub row it is on (Edit server… acts on the row). */
  | { kind: 'host'; host: NetworkHost; row: HubRow }
  | { kind: 'share'; share: ShareInfo }
  /** A hub row with no SMB host: an SFTP or WebDAV server, or an S3 account or one of its places. */
  | { kind: 'server'; row: HubRow }
