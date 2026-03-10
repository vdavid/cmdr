import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

vi.mock('$lib/tauri-commands', () => ({
    getVolumeSpace: vi.fn(),
}))

const { getVolumeSpace } = await import('$lib/tauri-commands')

import { createVolumeSpaceManager } from './volume-space-manager.svelte'
import type { VolumeInfo } from '../types'

const mockGetVolumeSpace = vi.mocked(getVolumeSpace)

function makeVolume(id: string, path = `/${id}`, category: VolumeInfo['category'] = 'attached_volume'): VolumeInfo {
    return { id, name: id, path, category, isEjectable: false }
}

const spaceInfo = { totalBytes: 1_000_000, availableBytes: 500_000 }

beforeEach(() => {
    vi.clearAllMocks()
    vi.useFakeTimers()
})

afterEach(() => {
    vi.useRealTimers()
})

describe('fetchVolumeSpaces', () => {
    it('stores space info on successful fetch', async () => {
        mockGetVolumeSpace.mockResolvedValue({ data: spaceInfo, timedOut: false })
        const mgr = createVolumeSpaceManager()
        const vol = makeVolume('disk1')

        await mgr.fetchVolumeSpaces([vol])

        expect(mgr.volumeSpaceMap.get('disk1')).toEqual(spaceInfo)
        expect(mgr.spaceTimedOutSet.has('disk1')).toBe(false)
    })

    it('skips volumes already in the space map', async () => {
        mockGetVolumeSpace.mockResolvedValue({ data: spaceInfo, timedOut: false })
        const mgr = createVolumeSpaceManager()
        const vol = makeVolume('disk1')

        await mgr.fetchVolumeSpaces([vol])
        mockGetVolumeSpace.mockClear()

        await mgr.fetchVolumeSpaces([vol])
        expect(mockGetVolumeSpace).not.toHaveBeenCalled()
    })

    it('filters out non-physical volumes (favorites, cloud, network, mobile)', async () => {
        mockGetVolumeSpace.mockResolvedValue({ data: spaceInfo, timedOut: false })
        const mgr = createVolumeSpaceManager()
        const vols = [
            makeVolume('fav', '/fav', 'favorite'),
            makeVolume('cloud', '/cloud', 'cloud_drive'),
            makeVolume('net', '/net', 'network'),
            makeVolume('mob', '/mob', 'mobile_device'),
        ]

        await mgr.fetchVolumeSpaces(vols)
        expect(mockGetVolumeSpace).not.toHaveBeenCalled()
    })

    it('includes main_volume and attached_volume', async () => {
        mockGetVolumeSpace.mockResolvedValue({ data: spaceInfo, timedOut: false })
        const mgr = createVolumeSpaceManager()
        const vols = [makeVolume('main', '/', 'main_volume'), makeVolume('ext', '/ext', 'attached_volume')]

        await mgr.fetchVolumeSpaces(vols)
        expect(mockGetVolumeSpace).toHaveBeenCalledTimes(2)
    })

    it('marks volume as timed out when backend reports timedOut', async () => {
        mockGetVolumeSpace.mockResolvedValue({ data: null, timedOut: true })
        const mgr = createVolumeSpaceManager()
        const vol = makeVolume('slow')

        const promise = mgr.fetchVolumeSpaces([vol])
        await vi.advanceTimersByTimeAsync(0)
        await promise

        expect(mgr.spaceTimedOutSet.has('slow')).toBe(true)
        expect(mgr.volumeSpaceMap.has('slow')).toBe(false)
    })

    it('marks volume as timed out when withTimeout returns fallback null', async () => {
        // getVolumeSpace never resolves, so withTimeout returns the null fallback after 3s
        mockGetVolumeSpace.mockReturnValue(new Promise(() => {}))
        const mgr = createVolumeSpaceManager()
        const vol = makeVolume('hung')

        const promise = mgr.fetchVolumeSpaces([vol])
        await vi.advanceTimersByTimeAsync(3000)
        await promise

        expect(mgr.spaceTimedOutSet.has('hung')).toBe(true)
    })

    it('schedules auto-retry after timeout', async () => {
        mockGetVolumeSpace.mockReturnValue(new Promise(() => {}))
        const mgr = createVolumeSpaceManager()
        const vol = makeVolume('hung')

        const promise = mgr.fetchVolumeSpaces([vol])
        await vi.advanceTimersByTimeAsync(3000)
        await promise

        // Now set up a successful response for the auto-retry
        mockGetVolumeSpace.mockResolvedValue({ data: spaceInfo, timedOut: false })

        await vi.advanceTimersByTimeAsync(5000)

        expect(mgr.volumeSpaceMap.get('hung')).toEqual(spaceInfo)
        expect(mgr.spaceTimedOutSet.has('hung')).toBe(false)
    })

    it('marks auto-retry in spaceAutoRetryingSet', async () => {
        let resolveRetry: (v: unknown) => void = () => {}
        mockGetVolumeSpace.mockReturnValueOnce(new Promise(() => {})) // initial fetch hangs

        const mgr = createVolumeSpaceManager()
        const vol = makeVolume('hung')

        const promise = mgr.fetchVolumeSpaces([vol])
        await vi.advanceTimersByTimeAsync(3000)
        await promise

        // Set up auto-retry that we can control
        mockGetVolumeSpace.mockReturnValueOnce(
            new Promise((resolve) => {
                resolveRetry = resolve as (v: unknown) => void
            }),
        )

        await vi.advanceTimersByTimeAsync(5000)

        // Auto-retry should be in flight
        expect(mgr.spaceAutoRetryingSet.has('hung')).toBe(true)

        // Resolve the retry
        resolveRetry({ data: spaceInfo, timedOut: false })
        await vi.advanceTimersByTimeAsync(0)

        expect(mgr.spaceAutoRetryingSet.has('hung')).toBe(false)
    })

    it('fetches multiple volumes in parallel', async () => {
        mockGetVolumeSpace.mockResolvedValue({ data: spaceInfo, timedOut: false })
        const mgr = createVolumeSpaceManager()
        const vols = [makeVolume('a'), makeVolume('b'), makeVolume('c')]

        await mgr.fetchVolumeSpaces(vols)

        expect(mgr.volumeSpaceMap.size).toBe(3)
    })
})

describe('retryVolumeSpace (doRetryVolumeSpace)', () => {
    it('transitions from timed out to success on retry', async () => {
        // Initial fetch times out
        mockGetVolumeSpace.mockReturnValueOnce(new Promise(() => {}))
        const mgr = createVolumeSpaceManager()
        const vol = makeVolume('disk1')

        const promise = mgr.fetchVolumeSpaces([vol])
        await vi.advanceTimersByTimeAsync(3000)
        await promise
        expect(mgr.spaceTimedOutSet.has('disk1')).toBe(true)

        // Retry succeeds
        mockGetVolumeSpace.mockResolvedValueOnce({ data: spaceInfo, timedOut: false })
        mgr.retryVolumeSpace(vol)
        await vi.advanceTimersByTimeAsync(0)

        expect(mgr.volumeSpaceMap.get('disk1')).toEqual(spaceInfo)
        expect(mgr.spaceTimedOutSet.has('disk1')).toBe(false)
        expect(mgr.spaceRetryingSet.has('disk1')).toBe(false)
    })

    it('sets spaceRetryingSet while retry is in flight', async () => {
        let resolveRetry: (v: unknown) => void = () => {}
        mockGetVolumeSpace.mockReturnValueOnce(
            new Promise((resolve) => {
                resolveRetry = resolve as (v: unknown) => void
            }),
        )

        const mgr = createVolumeSpaceManager()
        const vol = makeVolume('disk1')
        mgr.spaceTimedOutSet.add('disk1')

        mgr.retryVolumeSpace(vol)
        expect(mgr.spaceRetryingSet.has('disk1')).toBe(true)

        resolveRetry({ data: spaceInfo, timedOut: false })
        await vi.advanceTimersByTimeAsync(0)

        expect(mgr.spaceRetryingSet.has('disk1')).toBe(false)
    })

    it('adds volume to spaceRetryAttemptedSet', async () => {
        mockGetVolumeSpace.mockResolvedValue({ data: spaceInfo, timedOut: false })
        const mgr = createVolumeSpaceManager()
        const vol = makeVolume('disk1')

        mgr.retryVolumeSpace(vol)
        await vi.advanceTimersByTimeAsync(0)

        expect(mgr.spaceRetryAttemptedSet.has('disk1')).toBe(true)
    })

    it('debounces: ignores clicks while retry is in flight', async () => {
        let resolveRetry: (v: unknown) => void = () => {}
        mockGetVolumeSpace.mockReturnValueOnce(
            new Promise((resolve) => {
                resolveRetry = resolve as (v: unknown) => void
            }),
        )

        const mgr = createVolumeSpaceManager()
        const vol = makeVolume('disk1')

        mgr.retryVolumeSpace(vol)
        mgr.retryVolumeSpace(vol) // should be ignored
        mgr.retryVolumeSpace(vol) // should be ignored

        expect(mockGetVolumeSpace).toHaveBeenCalledTimes(1)

        resolveRetry({ data: spaceInfo, timedOut: false })
        await vi.advanceTimersByTimeAsync(0)
    })

    it('allows a new retry after the previous one completes', async () => {
        mockGetVolumeSpace.mockResolvedValue({ data: spaceInfo, timedOut: false })
        const mgr = createVolumeSpaceManager()
        const vol = makeVolume('disk1')

        mgr.retryVolumeSpace(vol)
        await vi.advanceTimersByTimeAsync(0)

        mockGetVolumeSpace.mockClear()
        // Remove from map to allow a second retry to proceed
        mgr.volumeSpaceMap.delete('disk1')

        mgr.retryVolumeSpace(vol)
        expect(mockGetVolumeSpace).toHaveBeenCalledTimes(1)
        await vi.advanceTimersByTimeAsync(0)
    })
})

describe('handleRetryFailure', () => {
    it('triggers shake animation via spaceRetryFailedSet, clears after 400ms', async () => {
        // Retry that times out via withTimeout fallback
        mockGetVolumeSpace.mockReturnValue(new Promise(() => {}))
        const mgr = createVolumeSpaceManager()
        const vol = makeVolume('disk1')

        mgr.retryVolumeSpace(vol)
        // Wait for withTimeout to fire (3s)
        await vi.advanceTimersByTimeAsync(3000)

        expect(mgr.spaceRetryFailedSet.has('disk1')).toBe(true)
        expect(mgr.spaceRetryingSet.has('disk1')).toBe(false)

        // Shake animation clears after 400ms
        await vi.advanceTimersByTimeAsync(400)
        expect(mgr.spaceRetryFailedSet.has('disk1')).toBe(false)
    })

    it('triggers failure when retry returns timedOut: true', async () => {
        mockGetVolumeSpace.mockResolvedValue({ data: null, timedOut: true })
        const mgr = createVolumeSpaceManager()
        const vol = makeVolume('disk1')

        mgr.retryVolumeSpace(vol)
        await vi.advanceTimersByTimeAsync(0)

        expect(mgr.spaceRetryFailedSet.has('disk1')).toBe(true)

        await vi.advanceTimersByTimeAsync(400)
        expect(mgr.spaceRetryFailedSet.has('disk1')).toBe(false)
    })

    it('triggers failure when getVolumeSpace throws', async () => {
        mockGetVolumeSpace.mockRejectedValue(new Error('IPC error'))
        const mgr = createVolumeSpaceManager()
        const vol = makeVolume('disk1')

        mgr.retryVolumeSpace(vol)
        await vi.advanceTimersByTimeAsync(0)

        expect(mgr.spaceRetryFailedSet.has('disk1')).toBe(true)

        await vi.advanceTimersByTimeAsync(400)
        expect(mgr.spaceRetryFailedSet.has('disk1')).toBe(false)
    })
})

describe('scheduleAutoRetry', () => {
    it('does not auto-retry if volume is no longer timed out', async () => {
        mockGetVolumeSpace.mockReturnValueOnce(new Promise(() => {}))
        const mgr = createVolumeSpaceManager()
        const vol = makeVolume('disk1')

        const promise = mgr.fetchVolumeSpaces([vol])
        await vi.advanceTimersByTimeAsync(3000)
        await promise

        expect(mgr.spaceTimedOutSet.has('disk1')).toBe(true)

        // Manually clear the timed out state before auto-retry fires
        mgr.spaceTimedOutSet.delete('disk1')
        mockGetVolumeSpace.mockClear()

        await vi.advanceTimersByTimeAsync(5000)
        expect(mockGetVolumeSpace).not.toHaveBeenCalled()
    })

    it('does not auto-retry if volume is already retrying', async () => {
        mockGetVolumeSpace.mockReturnValueOnce(new Promise(() => {}))
        const mgr = createVolumeSpaceManager()
        const vol = makeVolume('disk1')

        const promise = mgr.fetchVolumeSpaces([vol])
        await vi.advanceTimersByTimeAsync(3000)
        await promise

        // Manually mark as retrying
        mgr.spaceRetryingSet.add('disk1')
        mockGetVolumeSpace.mockClear()

        await vi.advanceTimersByTimeAsync(5000)
        expect(mockGetVolumeSpace).not.toHaveBeenCalled()
    })

    it('auto-retry failure triggers shake animation', async () => {
        // Initial fetch hangs
        mockGetVolumeSpace.mockReturnValueOnce(new Promise(() => {}))
        const mgr = createVolumeSpaceManager()
        const vol = makeVolume('disk1')

        const promise = mgr.fetchVolumeSpaces([vol])
        await vi.advanceTimersByTimeAsync(3000)
        await promise

        // Auto-retry also hangs (times out)
        mockGetVolumeSpace.mockReturnValueOnce(new Promise(() => {}))
        await vi.advanceTimersByTimeAsync(5000) // auto-retry fires
        await vi.advanceTimersByTimeAsync(3000) // withTimeout fires

        expect(mgr.spaceRetryFailedSet.has('disk1')).toBe(true)

        await vi.advanceTimersByTimeAsync(400)
        expect(mgr.spaceRetryFailedSet.has('disk1')).toBe(false)
    })
})

describe('clearAll', () => {
    it('clears all sets and the space map', async () => {
        mockGetVolumeSpace.mockResolvedValue({ data: spaceInfo, timedOut: false })
        const mgr = createVolumeSpaceManager()
        const vol = makeVolume('disk1')

        await mgr.fetchVolumeSpaces([vol])
        mgr.spaceTimedOutSet.add('other')
        mgr.spaceRetryingSet.add('other')
        mgr.spaceRetryFailedSet.add('other')
        mgr.spaceRetryAttemptedSet.add('other')
        mgr.spaceAutoRetryingSet.add('other')

        mgr.clearAll()

        expect(mgr.volumeSpaceMap.size).toBe(0)
        expect(mgr.spaceTimedOutSet.size).toBe(0)
        expect(mgr.spaceRetryingSet.size).toBe(0)
        expect(mgr.spaceRetryFailedSet.size).toBe(0)
        expect(mgr.spaceRetryAttemptedSet.size).toBe(0)
        expect(mgr.spaceAutoRetryingSet.size).toBe(0)
    })
})

describe('destroy', () => {
    it('cancels pending auto-retry timers', async () => {
        mockGetVolumeSpace.mockReturnValueOnce(new Promise(() => {}))
        const mgr = createVolumeSpaceManager()
        const vol = makeVolume('disk1')

        const promise = mgr.fetchVolumeSpaces([vol])
        await vi.advanceTimersByTimeAsync(3000)
        await promise

        expect(mgr.spaceTimedOutSet.has('disk1')).toBe(true)

        mgr.destroy()

        // Set up mock that would succeed if the auto-retry fired
        mockGetVolumeSpace.mockResolvedValue({ data: spaceInfo, timedOut: false })

        await vi.advanceTimersByTimeAsync(5000)

        // Auto-retry should not have fired
        expect(mgr.volumeSpaceMap.has('disk1')).toBe(false)
    })
})
