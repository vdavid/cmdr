/**
 * `createListingLoader`: handing a listing over in a pane swap, the unmount cleanup,
 * and tearing down listings the pane walks away from. The token-model suite and its
 * notes: `listing-loader.test.ts`.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

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
  resolvePathVolume: vi.fn(),
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
  resolvePathVolume: h.resolvePathVolume,
  trackEvent: h.trackEvent,
}))
vi.mock('./tag-sweep', () => ({ sweepListingTags: vi.fn() }))
vi.mock('../navigation/path-resolution', () => ({ resolveValidPath: h.resolveValidPath }))
vi.mock('$lib/error-messages/listing-error', () => ({ renderListingError: (e: unknown) => ({ rendered: e }) }))
vi.mock('$lib/icon-cache', () => ({ evictPerPathIconsForDir: vi.fn() }))
vi.mock('../rename/rename-activation', () => ({ cancelClickToRename: vi.fn() }))
vi.mock('$lib/ui/toast', () => ({ dismissTransientToastsForPane: vi.fn() }))
vi.mock('$lib/settings', () => ({ getSetting: h.getSetting }))
vi.mock('$lib/benchmark', () => ({ resetEpoch: vi.fn(), logEvent: vi.fn(), logEventValue: vi.fn() }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))

import { deferred, makeHarness } from './listing-loader.test-fixtures'

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
  // Default: the fallback target's owner is unresolvable, so the loader keeps the
  // pane's own volume. Tests that cross a volume boundary override this.
  h.resolvePathVolume.mockResolvedValue({ volume: null, timedOut: false })
  h.resolveValidPath.mockResolvedValue('/valid')
  h.getSetting.mockReturnValue(false) // 'listing.showTags' off → skip the tag sweep
})

describe('createListingLoader — swap state + cleanup', () => {
  it('getSwapState captures the live pane state', async () => {
    const { loader, state } = makeHarness()
    await loader.loadDirectory({ path: '/a' })
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
    await loader.loadDirectory({ path: '/a' })
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

  it('leaves a chain’s running reports behind with the directory they name', async () => {
    const { loader, spies } = makeHarness()
    await loader.loadDirectory({ path: '/a' })
    spies.renameForgetChainReports.mockClear()

    await loader.loadDirectory({ path: '/b' })

    expect(spies.renameForgetChainReports).toHaveBeenCalled()
  })

  it('keeps them through a re-list of the same directory', async () => {
    const { loader, spies } = makeHarness()
    await loader.loadDirectory({ path: '/a' })
    spies.renameForgetChainReports.mockClear()

    // An SMB reconnect, a retry after an error: the files the reports name are
    // still the ones on screen.
    await loader.loadDirectory({ path: '/a' })

    expect(spies.renameForgetChainReports).not.toHaveBeenCalled()
  })

  it('cleanup cancels the active listing and unlistens', async () => {
    const { loader, state } = makeHarness()
    await loader.loadDirectory({ path: '/a' })
    const idA = state.listingId
    loader.cleanup()
    expect(h.cancelListing).toHaveBeenCalledWith(idA)
    expect(h.listDirectoryEnd).toHaveBeenCalledWith(idA)
  })
})

describe('createListingLoader — abandoned listings are torn down, not just forgotten', () => {
  // `cancelListing` only flips a cancel flag on the backend's STREAMING_STATE
  // (`streaming.rs`); ONLY `listDirectoryEnd` drops the LISTING_CACHE entry and
  // stops its OS watcher. Every path that abandons a listing id must therefore
  // call `listDirectoryEnd`, or the entry and an armed file watch survive until
  // the 6 h orphan reaper. Observed: an E2E run carried 11 concurrent orphans,
  // each re-reading a deleted archive every debounce cycle for the whole run.

  it('ends the listing when a listing error abandons it', async () => {
    const { loader, state } = makeHarness()
    await loader.loadDirectory({ path: '/a' })
    const idA = state.listingId
    h.listDirectoryEnd.mockClear()

    h.listeners.error[0]({ listingId: idA, message: 'permission denied' })
    await vi.waitFor(() => {
      expect(state.error).toBe('permission denied')
    })

    expect(h.listDirectoryEnd).toHaveBeenCalledWith(idA)
  })

  it('ends the listing when a cancelled event abandons it', async () => {
    const { loader, state } = makeHarness()
    await loader.loadDirectory({ path: '/a' })
    const idA = state.listingId
    h.listDirectoryEnd.mockClear()

    h.listeners.cancelled[0]({ listingId: idA })
    await Promise.resolve()

    expect(h.listDirectoryEnd).toHaveBeenCalledWith(idA)
  })

  it('ends the listing a superseded load started', async () => {
    const { loader, state } = makeHarness()
    // Hold load A's start open so load B supersedes it BEFORE A's id is visible
    // to B's own cleanup — the window where nothing else would ever end A.
    const startA = deferred<{ listingId: string; status: { kind: string } }>()
    h.listDirectoryStart.mockImplementationOnce(() => startA.promise)

    const loadA = loader.loadDirectory({ path: '/a' })
    // A's id lands after its listeners register, which is after the first tick.
    await vi.waitFor(() => {
      expect(state.listingId).not.toBe('')
    })
    const idA = state.listingId
    await loader.loadDirectory({ path: '/b' })
    h.listDirectoryEnd.mockClear()

    startA.resolve({ listingId: idA, status: { kind: 'ok' } })
    await loadA

    expect(h.listDirectoryEnd).toHaveBeenCalledWith(idA)
  })
})
