/**
 * Shared harness for the `listing-loader*.test.ts` suites. The factory is a plain
 * module (no runes), so no reactive root is needed: the pane's lifecycle `$state` is
 * simulated by a plain `state` object behind the injected getters/setters, exactly as
 * `FilePane` wires them.
 *
 * No tests of its own. The `vi.mock` blocks stay DUPLICATED in each suite: vitest
 * hoists them per module, so they can't move here.
 */
import { vi } from 'vitest'
import { createListingLoader, type ListingLoaderDeps } from './listing-loader'

export interface Deferred<T> {
  promise: Promise<T>
  resolve: (value: T) => void
}
export function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((r) => {
    resolve = r
  })
  return { promise, resolve }
}

interface PaneState {
  volumeId: string
  volumePath: string
  currentPath: string
  canonicalPath: string | null
  includeHidden: boolean
  caps: { kind: string; hasBackendListing: boolean; hasParentRow: boolean }
  hasParent: boolean
  isMtpView: boolean
  viewMode: string
  listingId: string
  loading: boolean
  totalCount: number
  lastSequence: number
  error: string | null
  friendlyError: unknown
  openingFolder: boolean
  loadingCount: number | undefined
  finalizingCount: number | undefined
  volumeRootFromEvent: string | undefined
  cursorIndex: number
  selectedIndices: number[]
}

export function makeHarness(over: Partial<PaneState> = {}) {
  const state: PaneState = {
    volumeId: 'root',
    volumePath: '/',
    currentPath: '/a',
    canonicalPath: '/a',
    includeHidden: false,
    caps: { kind: 'local', hasBackendListing: true, hasParentRow: true },
    hasParent: false,
    isMtpView: false,
    viewMode: 'full',
    listingId: '',
    loading: true,
    totalCount: 0,
    lastSequence: 0,
    error: null,
    friendlyError: null,
    openingFolder: false,
    loadingCount: undefined,
    finalizingCount: undefined,
    volumeRootFromEvent: undefined,
    cursorIndex: 0,
    selectedIndices: [],
    ...over,
  }
  const spies = {
    onPathChange: vi.fn(),
    onVolumeChange: vi.fn(),
    onMtpFatalError: vi.fn(),
    onArchiveNeedsPassword: vi.fn(),
    onCancelLoading: vi.fn(),
    renameCancel: vi.fn(),
    renameForgetChainReports: vi.fn(),
    jumpClear: vi.fn(),
    syncMcp: vi.fn(),
    fetchEntryUnderCursor: vi.fn(),
    fetchListingStats: vi.fn(),
    clearEntryUnderCursor: vi.fn(),
    clearSyncStatusMap: vi.fn(),
    clearIndexStatusMap: vi.fn(),
    clearFolderCoverageMap: vi.fn(),
    clearSyncRetryTimer: vi.fn(),
    bumpCacheGeneration: vi.fn(),
    setSelectedIndices: vi.fn((idxs: number[]) => {
      state.selectedIndices = idxs
    }),
    clearSelection: vi.fn(() => {
      state.selectedIndices = []
    }),
    scrollToIndex: vi.fn(),
  }
  const deps: ListingLoaderDeps = {
    paneId: 'left',
    getVolumeId: () => state.volumeId,
    getVolumePath: () => state.volumePath,
    getCurrentPath: () => state.currentPath,
    setCurrentPath: (p) => {
      state.currentPath = p
    },
    getCanonicalPath: () => state.canonicalPath as never,
    getIncludeHidden: () => state.includeHidden,
    getSortBy: () => 'name',
    getSortOrder: () => 'ascending',
    getDirectorySortMode: () => 'likeFiles',
    getCaps: () => state.caps as never,
    getHasParent: () => state.hasParent,
    getIsMtpView: () => state.isMtpView,
    getViewMode: () => state.viewMode,
    getBriefListRef: () => undefined,
    getFullListRef: () => ({ scrollToIndex: spies.scrollToIndex }) as never,
    getListingId: () => state.listingId,
    setListingId: (id) => {
      state.listingId = id
    },
    getLoading: () => state.loading,
    setLoading: (v) => {
      state.loading = v
    },
    getTotalCount: () => state.totalCount,
    setTotalCount: (c) => {
      state.totalCount = c
    },
    getLastSequence: () => state.lastSequence,
    setLastSequence: (s) => {
      state.lastSequence = s
    },
    setError: (e) => {
      state.error = e
    },
    setFriendlyError: (f) => {
      state.friendlyError = f
    },
    setOpeningFolder: (v) => {
      state.openingFolder = v
    },
    setLoadingCount: (c) => {
      state.loadingCount = c
    },
    setFinalizingCount: (c) => {
      state.finalizingCount = c
    },
    setVolumeRootFromEvent: (r) => {
      state.volumeRootFromEvent = r
    },
    getCursorIndex: () => state.cursorIndex,
    setCursorIndexRaw: (i) => {
      state.cursorIndex = i
    },
    clearEntryUnderCursor: spies.clearEntryUnderCursor,
    clearSyncStatusMap: spies.clearSyncStatusMap,
    clearIndexStatusMap: spies.clearIndexStatusMap,
    clearFolderCoverageMap: spies.clearFolderCoverageMap,
    clearSyncRetryTimer: spies.clearSyncRetryTimer,
    bumpCacheGeneration: spies.bumpCacheGeneration,
    selection: {
      clearSelection: spies.clearSelection,
      getSelectedIndices: () => state.selectedIndices,
      setSelectedIndices: spies.setSelectedIndices,
    },
    renameCancel: spies.renameCancel,
    renameForgetChainReports: spies.renameForgetChainReports,
    jumpClear: spies.jumpClear,
    syncMcp: spies.syncMcp,
    fetchEntryUnderCursor: spies.fetchEntryUnderCursor,
    fetchListingStats: spies.fetchListingStats,
    onPathChange: spies.onPathChange,
    onVolumeChange: spies.onVolumeChange,
    onMtpFatalError: spies.onMtpFatalError,
    onArchiveNeedsPassword: spies.onArchiveNeedsPassword,
    onCancelLoading: spies.onCancelLoading,
  }
  const loader = createListingLoader(deps)
  return { loader, state, spies }
}
