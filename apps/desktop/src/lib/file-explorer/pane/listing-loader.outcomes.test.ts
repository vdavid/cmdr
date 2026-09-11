/**
 * `createListingLoader`: how a load ends (a listing error, an MTP failure, a cancel)
 * and what a caller waiting on it sees (`navigateToPath`, `whenLoadSettles`). The
 * token-model suite and its notes: `listing-loader.test.ts`.
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
import { NavigationSuperseded } from './listing-loader'

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

describe('createListingLoader — error / MTP / cancel handling', () => {
  it('routes MTP listing errors to onMtpFatalError and resets loading', async () => {
    const { loader, state, spies } = makeHarness({ isMtpView: true, volumeId: 'mtp-1:2' })
    await loader.loadDirectory({ path: '/DCIM' })
    const idA = state.listingId
    h.listeners.error[0]({ listingId: idA, message: 'device gone' })
    await Promise.resolve()
    expect(spies.onMtpFatalError).toHaveBeenCalledWith('device gone')
    expect(state.loading).toBe(false)
    expect(state.error).toBe('device gone')
    expect(h.pathExistsChecked).not.toHaveBeenCalled()
  })

  it('prompts for an encrypted archive instead of probing whether it exists', async () => {
    // Pre-fix this walked up out of the archive: the existence probe resolves
    // through the same archive volume, which can't answer without the password
    // either, so it read "gone" and the pane left before anyone was asked.
    const { loader, state, spies } = makeHarness()
    h.pathExistsChecked.mockResolvedValue({ data: false, timedOut: false })
    h.resolveValidPath.mockResolvedValue('/a')
    await loader.loadDirectory({ path: '/a/locked.7z' })
    h.listeners.error[0]({
      listingId: state.listingId,
      message: 'needs a password',
      error: { reason: { reason: 'archiveNeedsPassword', wrongAttempt: false } },
    })
    await vi.waitFor(() => {
      expect(spies.onArchiveNeedsPassword).toHaveBeenCalled()
    })

    expect(spies.onArchiveNeedsPassword.mock.calls[0][0]).toMatchObject({ archivePath: '/a/locked.7z' })
    // No probe, so no walk-up: the pane stays put, showing the fallback the
    // prompt sits on top of, with the failed path in history so Back goes one
    // step rather than two.
    expect(h.pathExistsChecked).not.toHaveBeenCalled()
    expect(h.resolveValidPath).not.toHaveBeenCalled()
    expect(state.error).toBe('needs a password')
    expect(spies.onPathChange).toHaveBeenCalledWith('/a/locked.7z')
  })

  it('shows a not-connected refusal without asking whether the path exists', async () => {
    // A phone or server nobody has connected yet can't answer the existence probe
    // either (it says "couldn't tell"), so asking costs a round trip and ends in the
    // same refusal. The typed reason is definitive, and nothing was deleted.
    const { loader, state, spies } = makeHarness({ volumeId: 'adb-phone' })
    h.pathExistsChecked.mockResolvedValue({ data: false, timedOut: true })
    await loader.loadDirectory({ path: 'adb://R58M/sdcard' })
    h.listeners.error[0]({
      listingId: state.listingId,
      message: 'not connected yet',
      error: { reason: { reason: 'notConnected', path: 'adb://R58M/sdcard' } },
    })
    await vi.waitFor(() => {
      expect(state.error).toBe('not connected yet')
    })

    expect(h.pathExistsChecked).not.toHaveBeenCalled()
    expect(h.resolveValidPath).not.toHaveBeenCalled()
    expect(spies.onPathChange).toHaveBeenCalledWith('adb://R58M/sdcard')
  })

  it('walks up to the nearest valid parent when the listing path was deleted', async () => {
    const { loader, state } = makeHarness()
    h.pathExistsChecked.mockResolvedValueOnce({ data: false, timedOut: false })
    h.resolveValidPath.mockResolvedValueOnce('/a')
    await loader.loadDirectory({ path: '/a/gone' })
    const idA = state.listingId
    h.listeners.error[0]({ listingId: idA, message: 'no such dir' })
    await vi.waitFor(() => {
      expect(h.resolveValidPath).toHaveBeenCalled()
    })
    await vi.waitFor(() => {
      expect(state.currentPath).toBe('/a')
    })
  })

  it('asks whether the path exists on the volume the load was started on', async () => {
    // Pre-fix the check went out with no volume id, so the backend asked the
    // Mac's boot disk about `adb://…`, which always says "gone".
    const { loader, state } = makeHarness({ volumeId: 'adb-phone', volumePath: 'adb://R58M' })
    await loader.loadDirectory({ path: 'adb://R58M/sdcard' })
    // The pane's live id moves on before the error lands; the load's own id wins.
    state.volumeId = 'root'
    h.listeners.error[0]({ listingId: state.listingId, message: 'not connected' })
    await vi.waitFor(() => {
      expect(h.pathExistsChecked).toHaveBeenCalled()
    })
    expect(h.pathExistsChecked).toHaveBeenCalledWith('adb://R58M/sdcard', 'adb-phone')
  })

  it('shows the error, and lists nothing again, when the walk-up lands on the path that just failed', async () => {
    // Pre-fix a failing volume ROOT walked up to itself and re-listed it, which
    // failed again: a phone's pane re-listed `adb://<serial>` ~15 times a second.
    const { loader, state, spies } = makeHarness({ volumeId: 'adb-phone', volumePath: 'adb://R58M' })
    h.pathExistsChecked.mockResolvedValue({ data: false, timedOut: false })
    h.resolveValidPath.mockResolvedValue('adb://R58M')
    h.resolvePathVolume.mockResolvedValue({ volume: { id: 'adb-phone', path: 'adb://R58M' }, timedOut: false })
    await loader.loadDirectory({ path: 'adb://R58M' })
    h.listeners.error[0]({ listingId: state.listingId, message: 'not connected' })
    await vi.waitFor(() => {
      expect(h.resolveValidPath).toHaveBeenCalledWith('adb://R58M', { volumeRoot: 'adb://R58M', volumeId: 'adb-phone' })
    })
    await vi.waitFor(() => {
      expect(state.error).toBe('not connected')
    })

    expect(h.listDirectoryStart).toHaveBeenCalledTimes(1)
    expect(spies.onVolumeChange).not.toHaveBeenCalled()
    expect(spies.onPathChange).toHaveBeenCalledWith('adb://R58M')
  })

  it('shows the error rather than leaving the volume when the walk-up finds nowhere to land', async () => {
    const { loader, state, spies } = makeHarness({ volumeId: 'smb-host', volumePath: '/Volumes/x' })
    h.pathExistsChecked.mockResolvedValue({ data: false, timedOut: false })
    h.resolveValidPath.mockResolvedValue(null)
    await loader.loadDirectory({ path: '/Volumes/x/gone' })
    h.listeners.error[0]({ listingId: state.listingId, message: 'no such dir' })
    await vi.waitFor(() => {
      expect(state.error).toBe('no such dir')
    })

    expect(h.listDirectoryStart).toHaveBeenCalledTimes(1)
    expect(spies.onVolumeChange).not.toHaveBeenCalled()
  })

  it('shows the friendly error (and pushes history) when the path still exists', async () => {
    const { loader, state, spies } = makeHarness()
    await loader.loadDirectory({ path: '/a' })
    const idA = state.listingId
    h.listeners.error[0]({ listingId: idA, message: 'permission denied', error: { code: 'EACCES' } })
    await vi.waitFor(() => {
      expect(state.error).toBe('permission denied')
    })
    expect(state.friendlyError).toEqual({ rendered: { code: 'EACCES' } })
    expect(spies.onPathChange).toHaveBeenCalledWith('/a')
  })

  it('a cancelled event resets loading but preserves the count', async () => {
    const { loader, state } = makeHarness()
    await loader.loadDirectory({ path: '/a' })
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
    const first = loader.navigateToPath({ path: '/a' })
    const firstRejected = vi.fn()
    first.catch(firstRejected)

    // A second navigation supersedes the first: loadDirectory rejects the prior pending load.
    const second = loader.navigateToPath({ path: '/b' })
    await vi.waitFor(() => {
      expect(firstRejected).toHaveBeenCalled()
    })
    expect(firstRejected.mock.calls[0][0]).toBeInstanceOf(NavigationSuperseded)

    // Wait for the second load to finish registering, then complete it.
    await vi.waitFor(() => {
      expect(h.listeners.complete.length).toBeGreaterThan(1)
    })
    completeCb(h.listeners.complete.length - 1)({ listingId: state.listingId, totalCount: 3, volumeRoot: '/' })
    await expect(second).resolves.toBeUndefined()
  })

  it('a superseded navigateToPath that nobody awaits raises no unhandled rejection', async () => {
    // The cancel-loading flow and every `navigate()` caller that drops `settled`
    // fire and forget. A newer navigation taking over is expected, not a failure.
    const unhandled = vi.fn()
    process.on('unhandledRejection', unhandled)
    try {
      const { loader } = makeHarness()
      void loader.navigateToPath({ path: '/a' })
      await loader.loadDirectory({ path: '/b' })
      await new Promise((resolve) => setTimeout(resolve, 0))
      expect(unhandled).not.toHaveBeenCalled()
    } finally {
      process.off('unhandledRejection', unhandled)
    }
  })

  it('resetLoadingState rejects the pending load with its message', async () => {
    const { loader } = makeHarness()
    const p = loader.navigateToPath({ path: '/a' })
    const rejected = vi.fn()
    p.catch(rejected)
    await Promise.resolve()

    loader.resetLoadingState('kaboom')
    await vi.waitFor(() => {
      expect(rejected).toHaveBeenCalled()
    })
    expect((rejected.mock.calls[0][0] as Error).message).toBe('kaboom')
  })

  it('whenLoadSettles resolves immediately when not loading', async () => {
    const { loader } = makeHarness({ loading: false })
    await expect(loader.whenLoadSettles()).resolves.toBeUndefined()
  })

  it('whenLoadSettles chains onto a pending navigateToPath without disturbing it', async () => {
    const { loader, state } = makeHarness({ loading: true })
    const nav = loader.navigateToPath({ path: '/a' })
    await vi.waitFor(() => {
      expect(h.listeners.complete.length).toBeGreaterThan(0)
    })
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
