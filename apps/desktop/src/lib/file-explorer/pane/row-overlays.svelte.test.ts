/**
 * Tests for `row-overlays.svelte.ts`, the file pane's three per-row badge feeds.
 * They pin:
 * - sync status merges into the map, and a timed-out fetch schedules exactly one retry,
 * - a new fetch supersedes a pending retry,
 * - the file-badge gate (indexing + badge setting + local pane) blocks the fetch,
 * - the folder-coverage gate ignores the file-badge setting but honours the rest,
 * - turning a gate off clears the matching map (and only that one),
 * - the idle sync poll re-reads the known paths, and skips while there's no listing,
 * - enrich progress refreshes (debounced) and enrich terminal refreshes right away,
 *   both only for THIS pane's volume,
 * - `cleanup()` stops the poll, the retry, the debounce, and the listeners.
 *
 * Uses Svelte runes, so the filename carries the `.svelte.` infix: the factory
 * creates `$effect`s in a reactive root, and the tests back the gate inputs with
 * `$state` so a mutation + `flushSync` drives them like the live component would.
 */
import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from 'vitest'
import { flushSync } from 'svelte'

const { ipc } = vi.hoisted<{
  ipc: {
    getSyncStatus: Mock
    mediaIndexFileStatus: Mock
    mediaIndexFolderCoverage: Mock
    onMediaEnrichProgress: Mock
    onMediaEnrichTerminal: Mock
  }
}>(() => ({
  ipc: {
    getSyncStatus: vi.fn(),
    mediaIndexFileStatus: vi.fn(),
    mediaIndexFolderCoverage: vi.fn(),
    onMediaEnrichProgress: vi.fn(),
    onMediaEnrichTerminal: vi.fn(),
  },
}))

vi.mock('$lib/tauri-commands', () => ({
  getSyncStatus: ipc.getSyncStatus,
  mediaIndexFileStatus: ipc.mediaIndexFileStatus,
  mediaIndexFolderCoverage: ipc.mediaIndexFolderCoverage,
  onMediaEnrichProgress: ipc.onMediaEnrichProgress,
  onMediaEnrichTerminal: ipc.onMediaEnrichTerminal,
}))

// The two reactive settings getters are backed by module-level `$state` so a
// test can flip a toggle and see the factory's `$derived` gates re-run.
vi.mock('$lib/settings/reactive-settings.svelte', async () => await import('./test-media-index-settings.svelte'))

import { createRowOverlays, type RowOverlays } from './row-overlays.svelte'
import {
  resetMediaIndexSettings,
  setMediaIndexEnabled,
  setMediaIndexShowFileStatusIcons,
} from './test-media-index-settings.svelte'

type EnrichHandler = (payload: { volumeId: string }) => void

describe('createRowOverlays', () => {
  let dispose: (() => void) | undefined
  let progressHandler: EnrichHandler | undefined
  let terminalHandler: EnrichHandler | undefined
  let progressUnlisten: Mock
  let terminalUnlisten: Mock

  beforeEach(() => {
    vi.useFakeTimers()
    resetMediaIndexSettings()
    progressUnlisten = vi.fn()
    terminalUnlisten = vi.fn()
    ipc.getSyncStatus.mockReset().mockResolvedValue({ data: {}, timedOut: false })
    ipc.mediaIndexFileStatus.mockReset().mockResolvedValue([])
    ipc.mediaIndexFolderCoverage.mockReset().mockResolvedValue([])
    ipc.onMediaEnrichProgress.mockReset().mockImplementation((cb: EnrichHandler) => {
      progressHandler = cb
      return Promise.resolve(progressUnlisten)
    })
    ipc.onMediaEnrichTerminal.mockReset().mockImplementation((cb: EnrichHandler) => {
      terminalHandler = cb
      return Promise.resolve(terminalUnlisten)
    })
  })

  afterEach(() => {
    dispose?.()
    dispose = undefined
    vi.useRealTimers()
  })

  function create(opts: { volumeId?: string; listingId?: string; isLocal?: boolean } = {}) {
    let volumeId = $state(opts.volumeId ?? 'root')
    let listingId = $state(opts.listingId ?? 'listing-1')
    let isLocal = $state(opts.isLocal ?? true)
    let overlays!: RowOverlays
    dispose = $effect.root(() => {
      overlays = createRowOverlays({
        getVolumeId: () => volumeId,
        getListingId: () => listingId,
        getIsLocalPane: () => isLocal,
      })
    })
    flushSync()
    return {
      overlays,
      setVolumeId: (v: string) => {
        volumeId = v
        flushSync()
      },
      setListingId: (v: string) => {
        listingId = v
        flushSync()
      },
      setIsLocal: (v: boolean) => {
        isLocal = v
        flushSync()
      },
      settle: () => {
        flushSync()
      },
    }
  }

  describe('sync status', () => {
    it('merges fetched statuses into the map', async () => {
      ipc.getSyncStatus.mockResolvedValue({ data: { '/a': 'synced' }, timedOut: false })
      const { overlays } = create()
      await overlays.fetchSyncStatusForPaths(['/a'])
      flushSync()
      expect(overlays.syncStatusMap).toEqual({ '/a': 'synced' })
    })

    it('skips the IPC entirely for an empty path list', async () => {
      const { overlays } = create()
      await overlays.fetchSyncStatusForPaths([])
      expect(ipc.getSyncStatus).not.toHaveBeenCalled()
    })

    it('schedules exactly one retry after a timed-out fetch', async () => {
      ipc.getSyncStatus
        .mockResolvedValueOnce({ data: { '/a': 'syncing' }, timedOut: true })
        .mockResolvedValueOnce({ data: { '/a': 'synced' }, timedOut: true })
      const { overlays } = create()
      await overlays.fetchSyncStatusForPaths(['/a'])
      flushSync()
      expect(overlays.syncStatusMap).toEqual({ '/a': 'syncing' })

      await vi.advanceTimersByTimeAsync(5000)
      flushSync()
      expect(ipc.getSyncStatus).toHaveBeenCalledTimes(2)
      expect(overlays.syncStatusMap).toEqual({ '/a': 'synced' })

      // The retry's own `timedOut` must NOT chain into a third attempt.
      await vi.advanceTimersByTimeAsync(20000)
      expect(ipc.getSyncStatus).toHaveBeenCalledTimes(2)
    })

    it('lets a new fetch supersede a pending retry', async () => {
      ipc.getSyncStatus
        .mockResolvedValueOnce({ data: {}, timedOut: true })
        .mockResolvedValueOnce({ data: { '/b': 'synced' }, timedOut: false })
      const { overlays } = create()
      await overlays.fetchSyncStatusForPaths(['/a'])
      await overlays.fetchSyncStatusForPaths(['/b'])
      await vi.advanceTimersByTimeAsync(10000)
      expect(ipc.getSyncStatus).toHaveBeenCalledTimes(2)
    })

    it('keeps a failed fetch silent and leaves the map untouched', async () => {
      ipc.getSyncStatus.mockRejectedValue(new Error('nope'))
      const { overlays } = create()
      await overlays.fetchSyncStatusForPaths(['/a'])
      expect(overlays.syncStatusMap).toEqual({})
    })

    it('clearSyncRetryTimer cancels the pending retry', async () => {
      ipc.getSyncStatus.mockResolvedValue({ data: {}, timedOut: true })
      const { overlays } = create()
      await overlays.fetchSyncStatusForPaths(['/a'])
      overlays.clearSyncRetryTimer()
      await vi.advanceTimersByTimeAsync(10000)
      expect(ipc.getSyncStatus).toHaveBeenCalledTimes(1)
    })
  })

  describe('image-index gates', () => {
    it('fetches file status when indexing, the badge setting, and a local pane all agree', async () => {
      ipc.mediaIndexFileStatus.mockResolvedValue([{ path: '/a.jpg', state: 'indexed' }])
      const { overlays } = create()
      await overlays.fetchIndexStatusForPaths(['/a.jpg'])
      flushSync()
      expect(ipc.mediaIndexFileStatus).toHaveBeenCalledWith('root', ['/a.jpg'])
      expect(overlays.indexStatusMap).toEqual({ '/a.jpg': 'indexed' })
    })

    it('skips the file-status fetch on a non-local pane', async () => {
      const { overlays } = create({ isLocal: false })
      await overlays.fetchIndexStatusForPaths(['/a.jpg'])
      expect(ipc.mediaIndexFileStatus).not.toHaveBeenCalled()
    })

    it('skips the file-status fetch when the badge setting is off', async () => {
      const { overlays } = create()
      setMediaIndexShowFileStatusIcons(false)
      flushSync()
      await overlays.fetchIndexStatusForPaths(['/a.jpg'])
      expect(ipc.mediaIndexFileStatus).not.toHaveBeenCalled()
    })

    it('still fetches folder coverage when only the file-badge setting is off', async () => {
      ipc.mediaIndexFolderCoverage.mockResolvedValue([{ path: '/dir', indexed: 3, total: 4 }])
      const { overlays } = create()
      setMediaIndexShowFileStatusIcons(false)
      flushSync()
      await overlays.fetchFolderCoverageForPaths(['/dir'])
      flushSync()
      expect(overlays.folderCoverageMap).toEqual({ '/dir': { path: '/dir', indexed: 3, total: 4 } })
    })

    it('skips folder coverage when indexing is off entirely', async () => {
      const { overlays } = create()
      setMediaIndexEnabled(false)
      flushSync()
      await overlays.fetchFolderCoverageForPaths(['/dir'])
      expect(ipc.mediaIndexFolderCoverage).not.toHaveBeenCalled()
    })
  })

  describe('coalescing the image-index fetches', () => {
    /** Holds the IPC open so a burst lands while one call is genuinely in flight. */
    function pending<T>(): { resolve: (value: T) => void; promise: Promise<T> } {
      let resolve!: (value: T) => void
      const promise = new Promise<T>((r) => {
        resolve = r
      })
      return { resolve, promise }
    }

    it('keeps one file-status query in flight and re-asks with the newest paths', async () => {
      // A storm of visible-range renders and enrich ticks used to mean one backend
      // query each, which is how the blocking pool ran out and the app froze.
      const first = pending<{ path: string; state: string }[]>()
      ipc.mediaIndexFileStatus.mockReturnValueOnce(first.promise).mockResolvedValue([])
      const { overlays } = create()

      void overlays.fetchIndexStatusForPaths(['/a.jpg'])
      void overlays.fetchIndexStatusForPaths(['/b.jpg'])
      void overlays.fetchIndexStatusForPaths(['/c.jpg'])
      await vi.advanceTimersByTimeAsync(0)
      expect(ipc.mediaIndexFileStatus).toHaveBeenCalledTimes(1)

      first.resolve([{ path: '/a.jpg', state: 'indexed' }])
      await vi.advanceTimersByTimeAsync(0)
      expect(ipc.mediaIndexFileStatus).toHaveBeenCalledTimes(2)
      expect(ipc.mediaIndexFileStatus).toHaveBeenLastCalledWith('root', ['/c.jpg'])
    })

    it('keeps one folder-coverage query in flight too', async () => {
      const first = pending<{ path: string; indexed: number; total: number }[]>()
      ipc.mediaIndexFolderCoverage.mockReturnValueOnce(first.promise).mockResolvedValue([])
      const { overlays } = create()

      void overlays.fetchFolderCoverageForPaths(['/one'])
      void overlays.fetchFolderCoverageForPaths(['/two'])
      await vi.advanceTimersByTimeAsync(0)
      expect(ipc.mediaIndexFolderCoverage).toHaveBeenCalledTimes(1)

      first.resolve([])
      await vi.advanceTimersByTimeAsync(0)
      expect(ipc.mediaIndexFolderCoverage).toHaveBeenLastCalledWith('root', ['/two'])
    })

    it('a queued fetch that outlives the setting never reaches the backend', async () => {
      // Coalescing runs a queued request later than it was made, so the gate is
      // re-checked at run time. The map would end up empty either way (the gate-off
      // `$effect` re-clears it), but the query and the badge flicker are pure waste.
      const first = pending<{ path: string; state: string }[]>()
      ipc.mediaIndexFileStatus.mockReturnValueOnce(first.promise).mockResolvedValue([])
      const { overlays } = create()

      void overlays.fetchIndexStatusForPaths(['/a.jpg'])
      void overlays.fetchIndexStatusForPaths(['/b.jpg'])
      setMediaIndexShowFileStatusIcons(false)
      flushSync()

      first.resolve([{ path: '/a.jpg', state: 'indexed' }])
      await vi.advanceTimersByTimeAsync(0)
      flushSync()
      expect(ipc.mediaIndexFileStatus).toHaveBeenCalledTimes(1)
      expect(overlays.indexStatusMap).toEqual({})
    })

    it('cleanup drops a queued fetch so a destroyed pane asks for nothing more', async () => {
      const first = pending<{ path: string; state: string }[]>()
      ipc.mediaIndexFileStatus.mockReturnValueOnce(first.promise).mockResolvedValue([])
      const { overlays } = create()
      overlays.start()

      void overlays.fetchIndexStatusForPaths(['/a.jpg'])
      void overlays.fetchIndexStatusForPaths(['/b.jpg'])
      overlays.cleanup()

      first.resolve([])
      await vi.advanceTimersByTimeAsync(0)
      expect(ipc.mediaIndexFileStatus).toHaveBeenCalledTimes(1)
    })
  })

  describe('clearing on gate flips', () => {
    it('drops the file-badge map when the badge setting goes off, keeping coverage', async () => {
      ipc.mediaIndexFileStatus.mockResolvedValue([{ path: '/a.jpg', state: 'indexed' }])
      ipc.mediaIndexFolderCoverage.mockResolvedValue([{ path: '/dir', indexed: 1, total: 1 }])
      const { overlays } = create()
      await overlays.fetchIndexStatusForPaths(['/a.jpg'])
      await overlays.fetchFolderCoverageForPaths(['/dir'])
      flushSync()

      setMediaIndexShowFileStatusIcons(false)
      flushSync()
      expect(overlays.indexStatusMap).toEqual({})
      expect(overlays.folderCoverageMap).not.toEqual({})
    })

    it('drops both maps when the pane goes non-local', async () => {
      ipc.mediaIndexFileStatus.mockResolvedValue([{ path: '/a.jpg', state: 'indexed' }])
      ipc.mediaIndexFolderCoverage.mockResolvedValue([{ path: '/dir', indexed: 1, total: 1 }])
      const created = create()
      await created.overlays.fetchIndexStatusForPaths(['/a.jpg'])
      await created.overlays.fetchFolderCoverageForPaths(['/dir'])
      flushSync()

      created.setIsLocal(false)
      expect(created.overlays.indexStatusMap).toEqual({})
      expect(created.overlays.folderCoverageMap).toEqual({})
    })

    it('exposes explicit clears for the listing-swap path', async () => {
      ipc.getSyncStatus.mockResolvedValue({ data: { '/a': 'synced' }, timedOut: false })
      const { overlays } = create()
      await overlays.fetchSyncStatusForPaths(['/a'])
      flushSync()
      overlays.clearSyncStatusMap()
      overlays.clearIndexStatusMap()
      overlays.clearFolderCoverageMap()
      flushSync()
      expect(overlays.syncStatusMap).toEqual({})
    })
  })

  describe('idle poll and enrichment refresh', () => {
    it('re-reads the known sync paths on the poll tick', async () => {
      ipc.getSyncStatus.mockResolvedValue({ data: { '/a': 'synced' }, timedOut: false })
      const { overlays } = create()
      overlays.start()
      await overlays.fetchSyncStatusForPaths(['/a'])
      ipc.getSyncStatus.mockClear()

      await vi.advanceTimersByTimeAsync(3000)
      expect(ipc.getSyncStatus).toHaveBeenCalledWith(['/a'])
    })

    it('skips the poll when the folder holds no cloud files', async () => {
      // `unknown` is what a plain local file reports, and it cannot change without
      // the file being moved (which re-lists and re-fetches). Polling a folder of
      // them re-asks the provider forever for an answer that cannot move: two
      // batches every three seconds, per pane, measured on an idle prod session.
      ipc.getSyncStatus.mockResolvedValue({ data: { '/a': 'unknown', '/b': 'unknown' }, timedOut: false })
      const { overlays } = create()
      overlays.start()
      await overlays.fetchSyncStatusForPaths(['/a', '/b'])
      ipc.getSyncStatus.mockClear()

      await vi.advanceTimersByTimeAsync(3000)
      expect(ipc.getSyncStatus).not.toHaveBeenCalled()
    })

    it('keeps polling when even one path is a cloud file', async () => {
      // One cloud file keeps the whole folder polled: its neighbours are cheap to
      // include and a per-path split would re-ask the provider just as often.
      ipc.getSyncStatus.mockResolvedValue({ data: { '/a': 'unknown', '/b': 'synced' }, timedOut: false })
      const { overlays } = create()
      overlays.start()
      await overlays.fetchSyncStatusForPaths(['/a', '/b'])
      ipc.getSyncStatus.mockClear()

      await vi.advanceTimersByTimeAsync(3000)
      expect(ipc.getSyncStatus).toHaveBeenCalledWith(['/a', '/b'])
    })

    it('skips the poll while the pane has no listing', async () => {
      ipc.getSyncStatus.mockResolvedValue({ data: { '/a': 'synced' }, timedOut: false })
      const created = create()
      created.overlays.start()
      await created.overlays.fetchSyncStatusForPaths(['/a'])
      created.setListingId('')
      ipc.getSyncStatus.mockClear()

      await vi.advanceTimersByTimeAsync(3000)
      expect(ipc.getSyncStatus).not.toHaveBeenCalled()
    })

    it('debounces the enrich-progress refresh and ignores other volumes', async () => {
      ipc.mediaIndexFileStatus.mockResolvedValue([{ path: '/a.jpg', state: 'indexed' }])
      const { overlays } = create()
      overlays.start()
      await overlays.fetchIndexStatusForPaths(['/a.jpg'])
      await vi.advanceTimersByTimeAsync(0)
      ipc.mediaIndexFileStatus.mockClear()

      progressHandler?.({ volumeId: 'other-volume' })
      await vi.advanceTimersByTimeAsync(500)
      expect(ipc.mediaIndexFileStatus).not.toHaveBeenCalled()

      progressHandler?.({ volumeId: 'root' })
      progressHandler?.({ volumeId: 'root' })
      await vi.advanceTimersByTimeAsync(500)
      expect(ipc.mediaIndexFileStatus).toHaveBeenCalledTimes(1)
    })

    it('refreshes right away on the terminal event, without the debounce', async () => {
      ipc.mediaIndexFileStatus.mockResolvedValue([{ path: '/a.jpg', state: 'indexed' }])
      const { overlays } = create()
      overlays.start()
      await overlays.fetchIndexStatusForPaths(['/a.jpg'])
      await vi.advanceTimersByTimeAsync(0)
      ipc.mediaIndexFileStatus.mockClear()

      terminalHandler?.({ volumeId: 'root' })
      expect(ipc.mediaIndexFileStatus).toHaveBeenCalledTimes(1)
    })
  })

  describe('cleanup', () => {
    it('stops the poll, the retry, the debounce, and the listeners', async () => {
      ipc.getSyncStatus.mockResolvedValue({ data: { '/a': 'synced' }, timedOut: true })
      ipc.mediaIndexFileStatus.mockResolvedValue([{ path: '/a.jpg', state: 'indexed' }])
      const { overlays } = create()
      overlays.start()
      await overlays.fetchSyncStatusForPaths(['/a'])
      await overlays.fetchIndexStatusForPaths(['/a.jpg'])
      await vi.advanceTimersByTimeAsync(0)
      progressHandler?.({ volumeId: 'root' })

      ipc.getSyncStatus.mockClear()
      ipc.mediaIndexFileStatus.mockClear()
      overlays.cleanup()

      await vi.advanceTimersByTimeAsync(20000)
      expect(ipc.getSyncStatus).not.toHaveBeenCalled()
      expect(ipc.mediaIndexFileStatus).not.toHaveBeenCalled()
      expect(progressUnlisten).toHaveBeenCalledTimes(1)
      expect(terminalUnlisten).toHaveBeenCalledTimes(1)
    })
  })
})
