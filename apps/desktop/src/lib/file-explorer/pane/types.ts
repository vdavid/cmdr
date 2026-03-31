import type { FileEntry, NetworkHost } from '../types'

/** State snapshot for swapping panes without backend calls. */
export interface SwapState {
  currentPath: string
  listingId: string
  totalCount: number
  maxFilenameWidth: number | undefined
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
  getFilenameUnderCursor(): string | undefined
  setCursorIndex(index: number): Promise<void>
  getCursorIndex(): number
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

  getSelectedIndices(): number[]
  isAllSelected(): boolean
  setSelectedIndices(indices: number[]): void
  clearSelection(): void
  selectAll(): void
  toggleSelectionAtCursor(): void
  selectRange(startIndex: number, endIndex: number): void
  snapshotSelectionForOperation(): Promise<void>
  clearOperationSnapshot(): string[] | 'all' | null

  isRenaming(): boolean
  startRename(): void
  cancelRename(): void

  refreshView(): void
  refreshVolumeSpace(): Promise<void>
  refreshIndexSizes(): void

  navigateToParent(): Promise<boolean>
  navigateToPath(path: string, selectName?: string): void
  handleCancelLoading(): void

  handleKeyDown(e: KeyboardEvent): void
  handleKeyUp(e: KeyboardEvent): void
}

/** Typed interface for BriefList/FullList exported methods used by FilePane. */
export interface ListViewAPI {
  scrollToIndex(index: number): void
  refreshIndexSizes(): void
  getEntryAt(globalIndex: number): FileEntry | undefined
  /** BriefList only */
  handleKeyNavigation?(key: string, event?: KeyboardEvent): number | undefined
  /** FullList only */
  getVisibleItemsCount?(): number
}

/** Typed interface for VolumeBreadcrumb's exported methods. */
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
}

/** Typed interface for NetworkBrowser's exported methods (extends BrowserAPI with refresh). */
export interface NetworkBrowserAPI extends BrowserAPI {
  refresh(): void
}

/** Typed interface for NetworkMountView's exported methods. */
export interface NetworkMountViewAPI {
  handleKeyDown(e: KeyboardEvent): void
  setCursorIndex(index: number): void
  findItemIndex(name: string): number
  refreshNetworkHosts(): void
  setNetworkHost(host: NetworkHost | null): void
}
