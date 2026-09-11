/**
 * `createListingLoader`'s navigation branches: the walk-up fallback, a cancelled
 * load, and the parent step. The token-model suite and its notes:
 * `listing-loader.test.ts`.
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

import { makeHarness } from './listing-loader.test-fixtures'
import { NavigationCancelled } from './listing-loader'

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

describe('createListingLoader — navigateToFallback / handleCancelLoading / navigateToParent branches', () => {
  it('navigateToFallback switches to the root volume when the target is outside a non-root volume', () => {
    const { loader, state, spies } = makeHarness({ volumeId: 'smb-host', volumePath: '/Volumes/x' })
    loader.navigateToFallback('~')
    expect(spies.onVolumeChange).toHaveBeenCalledWith({ volumeId: 'root', volumePath: '/', targetPath: '~' })
    expect(state.currentPath).toBe('/a') // outside-volume path handed to onVolumeChange, currentPath left as-is
  })

  it('navigateToFallback loads the target in-place when it is inside the volume', async () => {
    const { loader, state, spies } = makeHarness({ volumeId: 'root' })
    h.resolvePathVolume.mockResolvedValueOnce({ volume: { id: 'root', path: '/' }, timedOut: false })
    loader.navigateToFallback('/some/dir')
    await vi.waitFor(() => {
      expect(state.currentPath).toBe('/some/dir')
    })
    expect(spies.onVolumeChange).not.toHaveBeenCalled()
    await vi.waitFor(() => {
      expect(h.listDirectoryStart).toHaveBeenCalled()
    })
  })

  it('navigateToFallback switches to the volume that owns the target when the walk-up crossed a boundary', async () => {
    // The share unmounted under the pane, so the walk-up climbed out of
    // `/Volumes/naspi` into `/Volumes` — which belongs to the ROOT volume.
    // Listing it under the pane's now-dangling volume id fails with
    // "Volume not found", which is what left a pane stuck for hours.
    const { loader, state, spies } = makeHarness({
      volumeId: 'smb-192-168-1-111-445-naspi',
      volumePath: '/',
      currentPath: '/Volumes/naspi/_todo_pics',
    })
    h.resolvePathVolume.mockResolvedValueOnce({ volume: { id: 'root', path: '/' }, timedOut: false })
    loader.navigateToFallback('/Volumes')
    await vi.waitFor(() => {
      expect(spies.onVolumeChange).toHaveBeenCalledWith({ volumeId: 'root', volumePath: '/', targetPath: '/Volumes' })
    })
    // The switch owns the landing, so the loader must not also list `/Volumes`
    // on the dead volume.
    expect(h.listDirectoryStart).not.toHaveBeenCalled()
    expect(state.currentPath).toBe('/Volumes/naspi/_todo_pics')
  })

  it('navigateToFallback stays put when the target owner cannot be resolved', async () => {
    // An unresolvable owner (dead mount, timeout) must not trigger a volume
    // switch: falling back to the pane's own volume is the honest guess.
    const { loader, state, spies } = makeHarness({ volumeId: 'smb-host', volumePath: '/Volumes/x' })
    h.resolvePathVolume.mockResolvedValueOnce({ volume: null, timedOut: true })
    loader.navigateToFallback('/Volumes/x/sub')
    await vi.waitFor(() => {
      expect(state.currentPath).toBe('/Volumes/x/sub')
    })
    expect(spies.onVolumeChange).not.toHaveBeenCalled()
  })

  it('the deleted-path walk-up switches volumes end to end', async () => {
    const { loader, state, spies } = makeHarness({
      volumeId: 'smb-192-168-1-111-445-naspi',
      volumePath: '/',
      currentPath: '/Volumes/naspi/gone',
    })
    h.pathExistsChecked.mockResolvedValueOnce({ data: false, timedOut: false })
    h.resolveValidPath.mockResolvedValueOnce('/Volumes')
    h.resolvePathVolume.mockResolvedValueOnce({ volume: { id: 'root', path: '/' }, timedOut: false })
    await loader.loadDirectory({ path: '/Volumes/naspi/gone' })
    h.listeners.error[0]({ listingId: state.listingId, message: 'no such dir' })
    await vi.waitFor(() => {
      expect(spies.onVolumeChange).toHaveBeenCalledWith({ volumeId: 'root', volumePath: '/', targetPath: '/Volumes' })
    })
  })

  it('handleCancelLoading is a no-op when not loading or without a listing', () => {
    const { loader, spies } = makeHarness({ loading: false, listingId: '' })
    loader.handleCancelLoading()
    expect(h.cancelListing).not.toHaveBeenCalled()
    expect(spies.onCancelLoading).not.toHaveBeenCalled()
  })

  it('handleCancelLoading cancels the active listing and reports the load it stopped, with nothing shown yet', async () => {
    const { loader, state, spies } = makeHarness({ volumeId: 'root' })
    await loader.loadDirectory({ path: '/a/sub', selectName: 'pick.txt' })
    loader.handleCancelLoading()
    expect(h.cancelListing).toHaveBeenCalledWith(state.listingId)
    expect(spies.onCancelLoading).toHaveBeenCalledWith({
      cancelled: { volumeId: 'root', path: '/a/sub', selectName: 'pick.txt' },
      lastShown: null,
    })
  })

  it('handleCancelLoading reports the last landed location, error screen included, never a superseded load', async () => {
    const { loader, state, spies } = makeHarness({ volumeId: 'root' })
    await loader.loadDirectory({ path: '/a' })
    completeCb(0)({ listingId: state.listingId, totalCount: 1, volumeRoot: '/' })
    await vi.waitFor(() => {
      expect(state.loading).toBe(false)
    })
    await loader.loadDirectory({ path: '/a/locked' })
    h.listeners.error[1]({ listingId: state.listingId, message: 'no', error: { reason: { reason: 'notConnected' } } })
    expect(spies.onPathChange).toHaveBeenCalledWith('/a/locked')

    await loader.loadDirectory({ path: '/b' }) // superseded before it lands
    await loader.loadDirectory({ path: '/c' })
    loader.handleCancelLoading()

    expect(spies.onCancelLoading).toHaveBeenCalledWith({
      cancelled: { volumeId: 'root', path: '/c' },
      lastShown: { volumeId: 'root', path: '/a/locked' },
    })
  })

  it('handleCancelLoading rejects an awaiting navigateToPath with NavigationCancelled', async () => {
    const { loader, state } = makeHarness({ loading: false })
    const outcome = loader.navigateToPath({ path: '/a/sub' }).then(
      () => 'landed',
      (e: unknown) => e,
    )
    await vi.waitFor(() => {
      expect(state.listingId).not.toBe('')
    })
    loader.handleCancelLoading()
    expect(await outcome).toBeInstanceOf(NavigationCancelled)
  })

  it('a cancelled navigateToPath that nobody awaits raises no unhandled rejection', async () => {
    // The cancel flow's own return trip fires and forgets, and a second Escape cancels it.
    const unhandled = vi.fn()
    process.on('unhandledRejection', unhandled)
    try {
      const { loader, state } = makeHarness({ loading: false })
      void loader.navigateToPath({ path: '/a' })
      await vi.waitFor(() => {
        expect(state.listingId).not.toBe('')
      })
      loader.handleCancelLoading()
      await new Promise((resolve) => setTimeout(resolve, 0))
      expect(unhandled).not.toHaveBeenCalled()
    } finally {
      process.off('unhandledRejection', unhandled)
    }
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
