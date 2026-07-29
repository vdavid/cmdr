/**
 * Tests for `deleted-dir-poll.ts`, the fallback for a directory deleted behind
 * the pane's back (macOS FSEvents doesn't report it). They pin:
 * - two consecutive confirmed "not exists" before navigating away, never one,
 * - a timeout (a slow syscall, or an SMB volume in `Disconnected`) resets the
 *   counter instead of counting as gone,
 * - the skips: no listing, mid-load, no backend listing, MTP, virtual git paths,
 * - on an external volume, a gone volume root hands off to the unmount handler
 *   rather than walking up inside a volume that isn't there,
 * - `stop()` ends the poll.
 */
import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from 'vitest'

const { ipc, git, resolution } = vi.hoisted<{
  ipc: { pathExistsChecked: Mock }
  git: { isVirtualGitPath: Mock }
  resolution: { resolveValidPath: Mock }
}>(() => ({
  ipc: { pathExistsChecked: vi.fn() },
  git: { isVirtualGitPath: vi.fn() },
  resolution: { resolveValidPath: vi.fn() },
}))

vi.mock('$lib/tauri-commands', () => ({ pathExistsChecked: ipc.pathExistsChecked }))
vi.mock('../git/path-detection', () => ({ isVirtualGitPath: git.isVirtualGitPath }))
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
    hasBackendListing: boolean
    isMtpView: boolean
    currentPath: string
    volumePath: string
  }

  beforeEach(() => {
    vi.useFakeTimers()
    vi.clearAllMocks()
    git.isVirtualGitPath.mockReturnValue(false)
    resolution.resolveValidPath.mockImplementation((p: string) => Promise.resolve(p.replace(/\/[^/]+$/, '') || '/'))
    existsMap({})
    navigateToFallback = vi.fn()
    state = {
      listingId: 'listing-1',
      loading: false,
      hasBackendListing: true,
      isMtpView: false,
      currentPath: '/dir/sub',
      volumePath: '/',
    }
    deps = {
      getListingId: () => state.listingId,
      getLoading: () => state.loading,
      getHasBackendListing: () => state.hasBackendListing,
      getIsMtpView: () => state.isMtpView,
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

    it('skips a pane whose kind has no backend listing', async () => {
      state.hasBackendListing = false
      await expectNoPoll()
    })

    it('skips MTP, which has a listing but no on-disk path to stat', async () => {
      state.isMtpView = true
      await expectNoPoll()
    })

    it('skips virtual git paths, which would evict the user back to `.git/`', async () => {
      git.isVirtualGitPath.mockReturnValue(true)
      await expectNoPoll()
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
