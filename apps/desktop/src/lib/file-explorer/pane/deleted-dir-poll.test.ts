/**
 * Tests for `deleted-dir-poll.ts`, the fallback for a directory deleted behind
 * the pane's back (macOS FSEvents doesn't report it). They pin:
 * - two consecutive confirmed "not exists" before navigating away, never one,
 * - a timeout (a slow syscall, or an SMB volume in `Disconnected`) resets the
 *   counter instead of counting as gone,
 * - the skips: no listing, mid-load, and a pane whose capabilities say its
 *   folder isn't polled (which kinds are: `volume-capabilities.test.ts`),
 * - on an external volume, a gone volume root hands off to the unmount handler
 *   rather than walking up inside a volume that isn't there,
 * - `stop()` ends the poll.
 */
import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from 'vitest'

const { ipc, resolution } = vi.hoisted<{
  ipc: { pathExistsChecked: Mock }
  resolution: { resolveValidPath: Mock }
}>(() => ({
  ipc: { pathExistsChecked: vi.fn() },
  resolution: { resolveValidPath: vi.fn() },
}))

vi.mock('$lib/tauri-commands', () => ({ pathExistsChecked: ipc.pathExistsChecked }))
vi.mock('../navigation/path-resolution', () => ({ resolveValidPath: resolution.resolveValidPath }))

import { createDeletedDirPoll, type DeletedDirPollDeps } from './deleted-dir-poll'

/** One `exists` answer per path, defaulting to "there". */
function existsMap(map: Record<string, { data: boolean; timedOut?: boolean }>) {
  ipc.pathExistsChecked.mockImplementation((path: string) =>
    Promise.resolve(map[path] ?? { data: true, timedOut: false }),
  )
}

describe('createDeletedDirPoll', () => {
  let deps: DeletedDirPollDeps
  let navigateToFallback: Mock
  let state: {
    listingId: string
    loading: boolean
    folderIsPolled: boolean
    currentPath: string
    volumePath: string
  }

  beforeEach(() => {
    vi.useFakeTimers()
    vi.clearAllMocks()
    resolution.resolveValidPath.mockImplementation((p: string) => Promise.resolve(p.replace(/\/[^/]+$/, '') || '/'))
    existsMap({})
    navigateToFallback = vi.fn()
    state = {
      listingId: 'listing-1',
      loading: false,
      folderIsPolled: true,
      currentPath: '/dir/sub',
      volumePath: '/',
    }
    deps = {
      getListingId: () => state.listingId,
      getLoading: () => state.loading,
      getFolderIsPolled: () => state.folderIsPolled,
      getCurrentPath: () => state.currentPath,
      getVolumePath: () => state.volumePath,
      navigateToFallback,
    }
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  async function tick(times = 1) {
    for (let i = 0; i < times; i++) await vi.advanceTimersByTimeAsync(2000)
  }

  it('navigates to the nearest valid parent after two confirmed misses', async () => {
    existsMap({ '/dir/sub': { data: false } })
    const poll = createDeletedDirPoll(deps)
    poll.start()

    await tick()
    expect(navigateToFallback).not.toHaveBeenCalled()

    await tick()
    expect(navigateToFallback).toHaveBeenCalledWith('/dir')
    poll.stop()
  })

  it('resets the counter when a poll times out', async () => {
    existsMap({ '/dir/sub': { data: false } })
    const poll = createDeletedDirPoll(deps)
    poll.start()
    await tick()

    existsMap({ '/dir/sub': { data: false, timedOut: true } })
    await tick()
    expect(navigateToFallback).not.toHaveBeenCalled()

    // The counter restarted, so one more miss still isn't enough.
    existsMap({ '/dir/sub': { data: false } })
    await tick()
    expect(navigateToFallback).not.toHaveBeenCalled()
    poll.stop()
  })

  it('resets the counter as soon as the directory is back', async () => {
    existsMap({ '/dir/sub': { data: false } })
    const poll = createDeletedDirPoll(deps)
    poll.start()
    await tick()

    existsMap({ '/dir/sub': { data: true } })
    await tick()

    existsMap({ '/dir/sub': { data: false } })
    await tick()
    expect(navigateToFallback).not.toHaveBeenCalled()
    poll.stop()
  })

  describe('the skips', () => {
    async function expectNoPoll() {
      const poll = createDeletedDirPoll(deps)
      poll.start()
      await tick(3)
      expect(ipc.pathExistsChecked).not.toHaveBeenCalled()
      poll.stop()
    }

    it('skips while the pane has no listing', async () => {
      state.listingId = ''
      await expectNoPoll()
    })

    it('skips mid-load', async () => {
      state.loading = true
      await expectNoPoll()
    })

    it("skips a pane whose capabilities say its folder isn't polled", async () => {
      state.folderIsPolled = false
      await expectNoPoll()
    })

    it('never walks a phone pane off its phone once the phone leaves the volume list', async () => {
      // ❗ The shape the old accident produced: an `adb://` pane whose row is gone
      // reads `volumePath` as `/`, so two boot-disk "misses" on a path the Mac can't
      // see used to walk it to the phone's root and re-list a gone phone every few
      // seconds. A phone's folder isn't polled at all.
      state.folderIsPolled = false
      state.currentPath = 'adb://R58M1/sdcard/DCIM'
      state.volumePath = '/'
      existsMap({ 'adb://R58M1/sdcard/DCIM': { data: false } })
      await expectNoPoll()
      expect(navigateToFallback).not.toHaveBeenCalled()
    })

    it('picks the answer up live, so a pane that moves onto a phone stops polling', async () => {
      existsMap({ '/dir/sub': { data: false } })
      const poll = createDeletedDirPoll(deps)
      poll.start()
      await tick()
      expect(ipc.pathExistsChecked).toHaveBeenCalledTimes(1)

      state.folderIsPolled = false
      await tick(3)
      expect(ipc.pathExistsChecked).toHaveBeenCalledTimes(1)
      expect(navigateToFallback).not.toHaveBeenCalled()
      poll.stop()
    })
  })

  describe('on an external volume', () => {
    beforeEach(() => {
      state.currentPath = '/Volumes/Ext/photos'
      state.volumePath = '/Volumes/Ext'
    })

    it('walks up when the volume itself is still mounted', async () => {
      existsMap({ '/Volumes/Ext/photos': { data: false }, '/Volumes/Ext': { data: true } })
      const poll = createDeletedDirPoll(deps)
      poll.start()
      await tick(2)
      expect(resolution.resolveValidPath).toHaveBeenCalledWith('/Volumes/Ext/photos', { volumeRoot: '/Volumes/Ext' })
      expect(navigateToFallback).toHaveBeenCalledWith('/Volumes/Ext')
      poll.stop()
    })

    it('leaves a gone volume to the unmount handler', async () => {
      existsMap({ '/Volumes/Ext/photos': { data: false }, '/Volumes/Ext': { data: false } })
      const poll = createDeletedDirPoll(deps)
      poll.start()
      await tick(2)
      expect(navigateToFallback).not.toHaveBeenCalled()
      poll.stop()
    })

    it('stays put when it cannot tell whether the volume is there', async () => {
      existsMap({
        '/Volumes/Ext/photos': { data: false },
        '/Volumes/Ext': { data: true, timedOut: true },
      })
      const poll = createDeletedDirPoll(deps)
      poll.start()
      await tick(2)
      expect(navigateToFallback).not.toHaveBeenCalled()
      poll.stop()
    })
  })

  it('stop() ends the poll', async () => {
    existsMap({ '/dir/sub': { data: false } })
    const poll = createDeletedDirPoll(deps)
    poll.start()
    poll.stop()
    await tick(3)
    expect(ipc.pathExistsChecked).not.toHaveBeenCalled()
  })
})
