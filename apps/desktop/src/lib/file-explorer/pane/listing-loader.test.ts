/**
 * Integration tests for `createListingLoader` — the streaming directory-load
 * pipeline + the generation / listingId drop-foreign-listings token model.
 *
 * The crown jewel: once a newer `loadDirectory` has run, the OLDER load's still-
 * registered streaming listeners must no-op. These tests fire an older load's
 * captured callbacks AFTER a newer load has started and assert nothing lands.
 * Proven red-by-mutation: deleting the generation half of the predicate in
 * `listing-token.ts` (`captured.generation === liveGeneration`) makes the
 * "foreign * dropped" cases fail; restoring it greens them.
 *
 * Two async tails are deliberately UNGUARDED (current behavior, preserved): the
 * `onListingError` `pathExistsChecked` continuation and `handleListingComplete`'s
 * post-`await findFileIndex` cursor write. The "async tail runs unguarded" test
 * is a behavior-lock, not a correctness assertion — it pins today's shape so a
 * later tidy-up can't silently re-guard it.
 *
 * The factory is a plain module (no runes), so no reactive root is needed: the
 * pane's lifecycle `$state` is simulated by a plain `state` object behind the
 * injected getters/setters, exactly as `FilePane` wires them.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

interface Deferred<T> {
  promise: Promise<T>
  resolve: (value: T) => void
}
function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((r) => {
    resolve = r
  })
  return { promise, resolve }
}

const h = vi.hoisted(() => ({
  listeners: {
    opening: [] as ((p: unknown) => void)[],
    progress: [] as ((p: unknown) => void)[],
    readComplete: [] as ((p: unknown) => void)[],
    complete: [] as ((p: unknown) => void)[],
    error: [] as ((p: unknown) => void)[],
    cancelled: [] as ((p: unknown) => void)[],
  },
  cancelListing: vi.fn(),
  listDirectoryEnd: vi.fn(),
  listDirectoryStart: vi.fn(),
  findFileIndex: vi.fn(),
  pathExistsChecked: vi.fn(),
  trackEvent: vi.fn(),
  resolveValidPath: vi.fn(),
  getSetting: vi.fn(),
}))

function register(bucket: ((p: unknown) => void)[]) {
  return (cb: (p: unknown) => void) => {
    bucket.push(cb)
    return Promise.resolve(vi.fn())
  }
}

vi.mock('$lib/tauri-commands', () => ({
  onListingOpening: register(h.listeners.opening),
  onListingProgress: register(h.listeners.progress),
  onListingReadComplete: register(h.listeners.readComplete),
  onListingComplete: register(h.listeners.complete),
  onListingError: register(h.listeners.error),
  onListingCancelled: register(h.listeners.cancelled),
  cancelListing: h.cancelListing,
  listDirectoryEnd: h.listDirectoryEnd,
  listDirectoryStart: h.listDirectoryStart,
  findFileIndex: h.findFileIndex,
  pathExistsChecked: h.pathExistsChecked,
  trackEvent: h.trackEvent,
}))
vi.mock('./tag-sweep', () => ({ sweepListingTags: vi.fn() }))
vi.mock('../navigation/path-resolution', () => ({ resolveValidPath: h.resolveValidPath }))
vi.mock('$lib/errors/listing-error', () => ({ renderListingError: (e: unknown) => ({ rendered: e }) }))
vi.mock('$lib/icon-cache', () => ({ evictPerPathIconsForDir: vi.fn() }))
vi.mock('../rename/rename-activation', () => ({ cancelClickToRename: vi.fn() }))
vi.mock('$lib/ui/toast', () => ({ dismissTransientToasts: vi.fn() }))
vi.mock('$lib/settings', () => ({ getSetting: h.getSetting }))
vi.mock('$lib/benchmark', () => ({ resetEpoch: vi.fn(), logEvent: vi.fn(), logEventValue: vi.fn() }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))

import { createListingLoader, type ListingLoaderDeps } from './listing-loader'

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

function makeHarness(over: Partial<PaneState> = {}) {
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
    onCancelLoading: vi.fn(),
    renameCancel: vi.fn(),
    jumpClear: vi.fn(),
    syncMcp: vi.fn(),
    fetchEntryUnderCursor: vi.fn(),
    fetchListingStats: vi.fn(),
    clearEntryUnderCursor: vi.fn(),
    clearSyncStatusMap: vi.fn(),
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
    clearSyncRetryTimer: spies.clearSyncRetryTimer,
    bumpCacheGeneration: spies.bumpCacheGeneration,
    selection: {
      clearSelection: spies.clearSelection,
      getSelectedIndices: () => state.selectedIndices,
      setSelectedIndices: spies.setSelectedIndices,
    },
    renameCancel: spies.renameCancel,
    jumpClear: spies.jumpClear,
    syncMcp: spies.syncMcp,
    fetchEntryUnderCursor: spies.fetchEntryUnderCursor,
    fetchListingStats: spies.fetchListingStats,
    onPathChange: spies.onPathChange,
    onVolumeChange: spies.onVolumeChange,
    onMtpFatalError: spies.onMtpFatalError,
    onCancelLoading: spies.onCancelLoading,
  }
  const loader = createListingLoader(deps)
  return { loader, state, spies }
}

/** The listingId the factory generated for the most recent (Nth) registered listener set. */
function completeCb(n: number) {
  return h.listeners.complete[n]
}

beforeEach(() => {
  vi.clearAllMocks()
  for (const b of Object.values(h.listeners)) b.length = 0
  h.listDirectoryStart.mockImplementation(
    (_v: unknown, _p: unknown, _hid: unknown, _sb: unknown, _so: unknown, listingId: string) =>
      Promise.resolve({ listingId, status: { kind: 'ok' } }),
  )
  h.findFileIndex.mockResolvedValue(0)
  h.pathExistsChecked.mockResolvedValue({ data: true, timedOut: false })
  h.resolveValidPath.mockResolvedValue('/valid')
  h.getSetting.mockReturnValue(false) // 'listing.showTags' off → skip the tag sweep
})

describe('createListingLoader — generation / drop-foreign token model', () => {
  it('accepts the current load’s complete event and commits its listing', async () => {
    const { loader, state, spies } = makeHarness()
    await loader.loadDirectory('/a')
    const idA = state.listingId

    completeCb(0)({ listingId: idA, totalCount: 5, volumeRoot: '/' })
    await vi.waitFor(() => { expect(state.totalCount).toBe(5); })
    expect(state.loading).toBe(false)
    expect(spies.onPathChange).toHaveBeenCalledWith('/a')
  })

  it('drops a foreign complete event once a newer load has advanced the generation', async () => {
    const { loader, state, spies } = makeHarness()
    await loader.loadDirectory('/a')
    const idA = state.listingId
    const cbA = completeCb(0)

    await loader.loadDirectory('/b')
    const idB = state.listingId
    expect(idB).not.toBe(idA)
    spies.onPathChange.mockClear()

    // Load A's complete arrives late — it must be inert.
    cbA({ listingId: idA, totalCount: 999, volumeRoot: '/' })
    await Promise.resolve()
    await Promise.resolve()
    expect(state.totalCount).not.toBe(999)
    expect(spies.onPathChange).not.toHaveBeenCalledWith('/a')

    // Load B's complete is accepted.
    completeCb(1)({ listingId: idB, totalCount: 7, volumeRoot: '/' })
    await vi.waitFor(() => { expect(state.totalCount).toBe(7); })
    expect(spies.onPathChange).toHaveBeenCalledWith('/b')
  })

  it('drops a complete event tagged with a foreign listingId even at the current generation', async () => {
    const { loader, state, spies } = makeHarness()
    await loader.loadDirectory('/a')
    completeCb(0)({ listingId: 'not-the-current-id', totalCount: 999, volumeRoot: '/' })
    await Promise.resolve()
    await Promise.resolve()
    expect(state.totalCount).not.toBe(999)
    expect(spies.onPathChange).not.toHaveBeenCalled()
  })

  it('drops foreign opening / progress / read-complete / error / cancelled events after a newer load', async () => {
    const { loader, state, spies } = makeHarness()
    await loader.loadDirectory('/a')
    const idA = state.listingId
    const opening = h.listeners.opening[0]
    const progress = h.listeners.progress[0]
    const readComplete = h.listeners.readComplete[0]
    const error = h.listeners.error[0]
    const cancelled = h.listeners.cancelled[0]

    await loader.loadDirectory('/b')
    // Reset the state fields these would touch, so a leak is visible.
    state.openingFolder = false
    state.loadingCount = undefined
    state.finalizingCount = undefined
    spies.onMtpFatalError.mockClear()

    opening({ listingId: idA })
    progress({ listingId: idA, loadedCount: 42 })
    readComplete({ listingId: idA, totalCount: 42 })
    error({ listingId: idA, message: 'boom' })
    cancelled({ listingId: idA })
    await Promise.resolve()

    expect(state.openingFolder).toBe(false)
    expect(state.loadingCount).toBeUndefined()
    expect(state.finalizingCount).toBeUndefined()
    expect(spies.onMtpFatalError).not.toHaveBeenCalled()
  })

  it('cancels the abandoned listing when the generation advances during listDirectoryStart (post-await guard)', async () => {
    const { loader } = makeHarness()
    const startA = deferred<{ listingId: string; status: unknown }>()
    h.listDirectoryStart.mockImplementationOnce(() => startA.promise)

    const loadA = loader.loadDirectory('/a')
    // Load A is parked at `await listDirectoryStart`. Grab its listingId from the call.
    await vi.waitFor(() => { expect(h.listDirectoryStart).toHaveBeenCalled(); })
    const idA = String(h.listDirectoryStart.mock.calls[0][5])

    // Supersede via adoptListing: it advances the generation but does NOT itself
    // cancel idA, so the post-await guard is the ONLY thing that can cancel it.
    // (Deleting the `thisGeneration !== loadGeneration` post-await check fails this.)
    loader.adoptListing({
      currentPath: '/x',
      listingId: 'swapped',
      totalCount: 0,
      cursorIndex: 0,
      selectedIndices: [],
      lastSequence: 0,
    })

    // Now let A's listDirectoryStart resolve — the post-await guard must cancel idA.
    startA.resolve({ listingId: idA, status: { kind: 'ok' } })
    await loadA

    expect(h.cancelListing).toHaveBeenCalledWith(idA)
  })
})

describe('createListingLoader — async-tail behavior lock (deliberately unguarded)', () => {
  it('runs the onListingError pathExistsChecked tail even after a newer load started', async () => {
    // Behavior-lock, NOT a correctness assertion: the error tail is intentionally
    // not re-guarded on generation. If this ever changes, revisit the extraction.
    const { loader, state, spies } = makeHarness()
    const existsA = deferred<{ data: boolean; timedOut: boolean }>()
    h.pathExistsChecked.mockReturnValueOnce(existsA.promise)

    await loader.loadDirectory('/a')
    const idA = state.listingId
    const errorCbA = h.listeners.error[0]

    // Fire A's error while A is still current — its tail starts and awaits pathExistsChecked.
    errorCbA({ listingId: idA, message: 'gone', error: { code: 'x' } })
    await Promise.resolve()

    // A newer load supersedes A.
    await loader.loadDirectory('/b')
    spies.onPathChange.mockClear()

    // The path "exists", so the tail shows the original error + pushes history — unguarded.
    existsA.resolve({ data: true, timedOut: false })
    await vi.waitFor(() => { expect(spies.onPathChange).toHaveBeenCalledWith('/a'); })
  })
})

describe('createListingLoader — error / MTP / cancel handling', () => {
  it('routes MTP listing errors to onMtpFatalError and resets loading', async () => {
    const { loader, state, spies } = makeHarness({ isMtpView: true, volumeId: 'mtp-1:2' })
    await loader.loadDirectory('/DCIM')
    const idA = state.listingId
    h.listeners.error[0]({ listingId: idA, message: 'device gone' })
    await Promise.resolve()
    expect(spies.onMtpFatalError).toHaveBeenCalledWith('device gone')
    expect(state.loading).toBe(false)
    expect(state.error).toBe('device gone')
    expect(h.pathExistsChecked).not.toHaveBeenCalled()
  })

  it('walks up to the nearest valid parent when the listing path was deleted', async () => {
    const { loader, state } = makeHarness()
    h.pathExistsChecked.mockResolvedValueOnce({ data: false, timedOut: false })
    h.resolveValidPath.mockResolvedValueOnce('/a')
    await loader.loadDirectory('/a/gone')
    const idA = state.listingId
    h.listeners.error[0]({ listingId: idA, message: 'no such dir' })
    await vi.waitFor(() => { expect(h.resolveValidPath).toHaveBeenCalled(); })
    await vi.waitFor(() => { expect(state.currentPath).toBe('/a'); })
  })

  it('shows the friendly error (and pushes history) when the path still exists', async () => {
    const { loader, state, spies } = makeHarness()
    await loader.loadDirectory('/a')
    const idA = state.listingId
    h.listeners.error[0]({ listingId: idA, message: 'permission denied', error: { code: 'EACCES' } })
    await vi.waitFor(() => { expect(state.error).toBe('permission denied'); })
    expect(state.friendlyError).toEqual({ rendered: { code: 'EACCES' } })
    expect(spies.onPathChange).toHaveBeenCalledWith('/a')
  })

  it('a cancelled event resets loading but preserves the count', async () => {
    const { loader, state } = makeHarness()
    await loader.loadDirectory('/a')
    const idA = state.listingId
    state.totalCount = 12
    h.listeners.cancelled[0]({ listingId: idA })
    await Promise.resolve()
    expect(state.loading).toBe(false)
    expect(state.totalCount).toBe(12) // preserveTotalCount
  })
})

describe('createListingLoader — pendingLoad / navigateToPath / whenLoadSettles', () => {
  it('navigateToPath rejects a prior pending load and resolves on complete', async () => {
    const { loader, state } = makeHarness()
    const first = loader.navigateToPath('/a')
    const firstRejected = vi.fn()
    first.catch(firstRejected)

    // A second navigation supersedes the first: loadDirectory rejects the prior pending load.
    const second = loader.navigateToPath('/b')
    await vi.waitFor(() => { expect(firstRejected).toHaveBeenCalled(); })
    expect(firstRejected.mock.calls[0][0]).toBeInstanceOf(Error)
    expect((firstRejected.mock.calls[0][0] as Error).message).toBe('Superseded by new navigation')

    // Wait for the second load to finish registering, then complete it.
    await vi.waitFor(() => { expect(h.listeners.complete.length).toBeGreaterThan(1); })
    completeCb(h.listeners.complete.length - 1)({ listingId: state.listingId, totalCount: 3, volumeRoot: '/' })
    await expect(second).resolves.toBeUndefined()
  })

  it('resetLoadingState rejects the pending load with its message', async () => {
    const { loader } = makeHarness()
    const p = loader.navigateToPath('/a')
    const rejected = vi.fn()
    p.catch(rejected)
    await Promise.resolve()

    loader.resetLoadingState('kaboom')
    await vi.waitFor(() => { expect(rejected).toHaveBeenCalled(); })
    expect((rejected.mock.calls[0][0] as Error).message).toBe('kaboom')
  })

  it('whenLoadSettles resolves immediately when not loading', async () => {
    const { loader } = makeHarness({ loading: false })
    await expect(loader.whenLoadSettles()).resolves.toBeUndefined()
  })

  it('whenLoadSettles chains onto a pending navigateToPath without disturbing it', async () => {
    const { loader, state } = makeHarness({ loading: true })
    const nav = loader.navigateToPath('/a')
    await vi.waitFor(() => { expect(h.listeners.complete.length).toBeGreaterThan(0); })
    const settles = loader.whenLoadSettles()
    const navResolved = vi.fn()
    const settlesResolved = vi.fn()
    void nav.then(navResolved)
    void settles.then(settlesResolved)

    completeCb(h.listeners.complete.length - 1)({ listingId: state.listingId, totalCount: 1, volumeRoot: '/' })
    await vi.waitFor(() => {
      expect(navResolved).toHaveBeenCalled()
      expect(settlesResolved).toHaveBeenCalled()
    })
  })
})

describe('createListingLoader — navigateToFallback / handleCancelLoading / navigateToParent branches', () => {
  it('navigateToFallback switches to the root volume when the target is outside a non-root volume', () => {
    const { loader, state, spies } = makeHarness({ volumeId: 'smb-host', volumePath: '/Volumes/x' })
    loader.navigateToFallback('~')
    expect(spies.onVolumeChange).toHaveBeenCalledWith('root', '/', '~')
    expect(state.currentPath).toBe('/a') // outside-volume path handed to onVolumeChange, currentPath left as-is
  })

  it('navigateToFallback loads the target in-place when it is inside the volume', async () => {
    const { loader, state, spies } = makeHarness({ volumeId: 'root' })
    loader.navigateToFallback('/some/dir')
    expect(spies.onVolumeChange).not.toHaveBeenCalled()
    expect(state.currentPath).toBe('/some/dir')
    await vi.waitFor(() => { expect(h.listDirectoryStart).toHaveBeenCalled(); })
  })

  it('handleCancelLoading is a no-op when not loading or without a listing', () => {
    const { loader, spies } = makeHarness({ loading: false, listingId: '' })
    loader.handleCancelLoading()
    expect(h.cancelListing).not.toHaveBeenCalled()
    expect(spies.onCancelLoading).not.toHaveBeenCalled()
  })

  it('handleCancelLoading cancels the active listing and bubbles the folder name', () => {
    const { loader, spies } = makeHarness({ loading: true, listingId: 'abc', currentPath: '/a/sub' })
    loader.handleCancelLoading()
    expect(h.cancelListing).toHaveBeenCalledWith('abc')
    expect(spies.onCancelLoading).toHaveBeenCalledWith('/a/sub', 'sub')
  })

  it('navigateToParent returns false at the volume root', async () => {
    const { loader } = makeHarness({ currentPath: '/', volumePath: '/' })
    await expect(loader.navigateToParent()).resolves.toBe(false)
    expect(h.listDirectoryStart).not.toHaveBeenCalled()
  })

  it('navigateToParent returns false when the canonical path is unresolved', async () => {
    const { loader } = makeHarness({ currentPath: '/a/b', canonicalPath: null })
    await expect(loader.navigateToParent()).resolves.toBe(false)
    expect(h.listDirectoryStart).not.toHaveBeenCalled()
  })

  it('navigateToParent loads the parent and selects the child folder', async () => {
    const { loader, state } = makeHarness({ currentPath: '/a/b', canonicalPath: '/a/b' })
    await expect(loader.navigateToParent()).resolves.toBe(true)
    expect(state.currentPath).toBe('/a')
    expect(h.listDirectoryStart).toHaveBeenCalled()
  })
})

describe('createListingLoader — swap state + cleanup', () => {
  it('getSwapState captures the live pane state', async () => {
    const { loader, state } = makeHarness()
    await loader.loadDirectory('/a')
    state.totalCount = 9
    state.cursorIndex = 3
    state.lastSequence = 4
    state.selectedIndices = [1, 2]
    const swap = loader.getSwapState()
    expect(swap).toEqual({
      currentPath: '/a',
      listingId: state.listingId,
      totalCount: 9,
      cursorIndex: 3,
      selectedIndices: [1, 2],
      lastSequence: 4,
    })
  })

  it('adoptListing installs the swapped listing and advances the generation', async () => {
    const { loader, state, spies } = makeHarness()
    // A load is in flight; its complete must be dropped after adoption bumps the generation.
    await loader.loadDirectory('/a')
    const cbA = completeCb(0)
    const idA = state.listingId

    loader.adoptListing({
      currentPath: '/adopted',
      listingId: 'swapped',
      totalCount: 6,
      cursorIndex: 2,
      selectedIndices: [0],
      lastSequence: 8,
    })
    expect(state.currentPath).toBe('/adopted')
    expect(state.listingId).toBe('swapped')
    expect(state.totalCount).toBe(6)
    expect(state.cursorIndex).toBe(2)
    expect(state.lastSequence).toBe(8)
    expect(state.loading).toBe(false)
    expect(spies.bumpCacheGeneration).toHaveBeenCalled()

    // The pre-adoption load's complete is now foreign → dropped.
    spies.onPathChange.mockClear()
    cbA({ listingId: idA, totalCount: 999, volumeRoot: '/' })
    await Promise.resolve()
    await Promise.resolve()
    expect(state.totalCount).toBe(6)
    expect(spies.onPathChange).not.toHaveBeenCalledWith('/a')
  })

  it('cleanup cancels the active listing and unlistens', async () => {
    const { loader, state } = makeHarness()
    await loader.loadDirectory('/a')
    const idA = state.listingId
    loader.cleanup()
    expect(h.cancelListing).toHaveBeenCalledWith(idA)
    expect(h.listDirectoryEnd).toHaveBeenCalledWith(idA)
  })
})
