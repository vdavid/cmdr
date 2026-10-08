/**
 * `createListingLoader`: a listing whose volume stopped answering. The pane must
 * say so (`stalled`) instead of sitting on a spinner the user reads as "empty",
 * clear it the moment anything else is heard for that load, and never let a load
 * that ends in an error render as an empty list.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

const h = vi.hoisted(() => ({
  listeners: {
    opening: [] as ((p: unknown) => void)[],
    stalled: [] as ((p: unknown) => void)[],
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
  trackLiveListing: vi.fn(),
  untrackLiveListing: vi.fn(),
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
  onListingStalled: register(h.listeners.stalled),
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
vi.mock('./listing-liveness', () => ({
  trackLiveListing: h.trackLiveListing,
  untrackLiveListing: h.untrackLiveListing,
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

import { makeHarness } from './listing-loader.test-fixtures'

beforeEach(() => {
  vi.clearAllMocks()
  for (const b of Object.values(h.listeners)) b.length = 0
  h.listDirectoryStart.mockImplementation(
    (_v: unknown, _p: unknown, _hid: unknown, _sb: unknown, _so: unknown, listingId: string) =>
      Promise.resolve({ listingId, status: { kind: 'ok' } }),
  )
  h.findFileIndex.mockResolvedValue(0)
  h.pathExistsChecked.mockResolvedValue({ data: true, timedOut: false })
  h.resolvePathVolume.mockResolvedValue({ volume: null, timedOut: false })
  h.resolveValidPath.mockResolvedValue('/valid')
  h.getSetting.mockReturnValue(false)
})

/** Starts a load and reports it stalled, the way a hung network mount does. */
async function stalledLoad() {
  const harness = makeHarness()
  await harness.loader.loadDirectory({ path: '/Volumes/nas' })
  h.listeners.stalled[0]({ listingId: harness.state.listingId, stalledOn: 'server' })
  return harness
}

describe('createListingLoader: a stalled listing', () => {
  it('marks the pane stalled while it keeps loading, and tells MCP', async () => {
    const { state, spies } = await stalledLoad()

    // What the folder is waiting on rides along, for the screen's wording.
    expect(state.stalled).toBe('server')
    expect(state.loading).toBe(true)
    expect(spies.syncMcp).toHaveBeenCalled()
  })

  it('ignores a stall reported for a listing the pane already left', async () => {
    const { loader, state } = makeHarness()
    await loader.loadDirectory({ path: '/Volumes/nas' })
    const oldId = state.listingId
    await loader.loadDirectory({ path: '/Users/me' })

    h.listeners.stalled[1]({ listingId: oldId, stalledOn: 'server' })

    expect(state.stalled).toBeNull()
  })

  it('drops the notice as soon as entries start arriving', async () => {
    const { state } = await stalledLoad()

    h.listeners.progress[0]({ listingId: state.listingId, loadedCount: 12 })

    expect(state.stalled).toBeNull()
    expect(state.loadingCount).toBe(12)
  })

  it('drops the notice when the listing lands on its own', async () => {
    const { state } = await stalledLoad()

    h.listeners.complete[0]({ listingId: state.listingId, totalCount: 3, volumeRoot: '/Volumes/nas' })
    await vi.waitFor(() => {
      expect(state.loading).toBe(false)
    })

    expect(state.stalled).toBeNull()
    expect(state.totalCount).toBe(3)
  })

  it('shows a listing that ends in an error as the error, never as an empty list', async () => {
    const { state, spies } = await stalledLoad()

    h.listeners.error[0]({
      listingId: state.listingId,
      message: 'Socket is not connected',
      error: { reason: { reason: 'couldntReadUnknown', path: '/Volumes/nas' } },
    })
    await vi.waitFor(() => {
      expect(state.loading).toBe(false)
    })

    expect(state.stalled).toBeNull()
    expect(state.friendlyError).not.toBeNull()
    expect(state.error).toBe('Socket is not connected')
    expect(spies.syncMcp).toHaveBeenCalled()
  })

  it('starts the next load unstalled', async () => {
    const { loader, state } = await stalledLoad()

    await loader.loadDirectory({ path: '/Users/me' })

    expect(state.stalled).toBeNull()
  })
})
