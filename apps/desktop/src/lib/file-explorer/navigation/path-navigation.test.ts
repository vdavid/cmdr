import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

vi.mock('$lib/tauri-commands', () => ({
    pathExists: vi.fn(),
    DEFAULT_VOLUME_ID: 'root',
}))

vi.mock('$lib/app-status-store', () => ({
    getLastUsedPathForVolume: vi.fn(),
}))

const { pathExists } = await import('$lib/tauri-commands')
const { getLastUsedPathForVolume } = await import('$lib/app-status-store')

import { withTimeout, determineNavigationPath, resolveValidPath } from './path-navigation'
import type { OtherPaneState } from './path-navigation'

const mockPathExists = vi.mocked(pathExists)
const mockGetLastUsedPath = vi.mocked(getLastUsedPathForVolume)

beforeEach(() => {
    vi.clearAllMocks()
})

describe('withTimeout', () => {
    beforeEach(() => {
        vi.useFakeTimers()
    })

    afterEach(() => {
        vi.useRealTimers()
    })

    it('returns the promise result if it resolves before timeout', async () => {
        const promise = Promise.resolve('resolved-value')
        const result = await withTimeout(promise, 500, 'fallback')
        expect(result).toBe('resolved-value')
    })

    it('returns the fallback if the promise does not resolve in time', async () => {
        const neverResolves = new Promise<string>(() => {})
        const resultPromise = withTimeout(neverResolves, 500, 'fallback')

        await vi.advanceTimersByTimeAsync(500)

        const result = await resultPromise
        expect(result).toBe('fallback')
    })
})

describe('determineNavigationPath', () => {
    const defaultOtherPane: OtherPaneState = {
        otherPaneVolumeId: 'other-volume',
        otherPanePath: '/other/path',
    }

    beforeEach(() => {
        vi.useFakeTimers()
    })

    afterEach(() => {
        vi.useRealTimers()
    })

    it('returns targetPath when it differs from volumePath (favorite navigation)', async () => {
        const result = await determineNavigationPath('root', '/root-path', '/favorite/path', defaultOtherPane)
        expect(result).toBe('/favorite/path')
        expect(mockPathExists).not.toHaveBeenCalled()
    })

    it('returns other pane path when same volume and path exists', async () => {
        const otherPane: OtherPaneState = {
            otherPaneVolumeId: 'root',
            otherPanePath: '/Users/test/documents',
        }
        mockPathExists.mockResolvedValue(true)
        mockGetLastUsedPath.mockResolvedValue(undefined)

        const resultPromise = determineNavigationPath('root', '/root-path', '/root-path', otherPane)
        await vi.advanceTimersByTimeAsync(500)
        const result = await resultPromise

        expect(result).toBe('/Users/test/documents')
    })

    it('does not return other pane path when volumes differ', async () => {
        const otherPane: OtherPaneState = {
            otherPaneVolumeId: 'external',
            otherPanePath: '/Volumes/External/docs',
        }
        mockPathExists.mockResolvedValue(false)
        mockGetLastUsedPath.mockResolvedValue(undefined)

        const resultPromise = determineNavigationPath('root', '/', '/', otherPane)
        await vi.advanceTimersByTimeAsync(500)
        const result = await resultPromise

        expect(result).toBe('~')
        expect(mockPathExists).not.toHaveBeenCalledWith('/Volumes/External/docs')
    })

    it('returns last used path when it exists', async () => {
        mockPathExists.mockResolvedValue(false)
        mockGetLastUsedPath.mockResolvedValue('/Users/test/last-used')

        // pathExists returns false for otherPane (different volume), true for lastUsedPath
        mockPathExists.mockImplementation(
            (p: string): Promise<boolean> =>
                p === '/Users/test/last-used' ? Promise.resolve(true) : Promise.resolve(false),
        )

        const resultPromise = determineNavigationPath('root', '/', '/', defaultOtherPane)
        await vi.advanceTimersByTimeAsync(500)
        const result = await resultPromise

        expect(result).toBe('/Users/test/last-used')
    })

    it('returns ~ when volume is DEFAULT_VOLUME_ID and no better option', async () => {
        mockPathExists.mockResolvedValue(false)
        mockGetLastUsedPath.mockResolvedValue(undefined)

        const resultPromise = determineNavigationPath('root', '/', '/', defaultOtherPane)
        await vi.advanceTimersByTimeAsync(500)
        const result = await resultPromise

        expect(result).toBe('~')
    })

    it('returns volumePath when not default volume and no better option', async () => {
        mockPathExists.mockResolvedValue(false)
        mockGetLastUsedPath.mockResolvedValue(undefined)

        const resultPromise = determineNavigationPath(
            'external',
            '/Volumes/External',
            '/Volumes/External',
            defaultOtherPane,
        )
        await vi.advanceTimersByTimeAsync(500)
        const result = await resultPromise

        expect(result).toBe('/Volumes/External')
    })

    it('handles timeout on pathExists (slow response falls through to default)', async () => {
        const otherPane: OtherPaneState = {
            otherPaneVolumeId: 'root',
            otherPanePath: '/Users/test/slow-mount',
        }
        // pathExists never resolves — simulates a hung network mount
        mockPathExists.mockReturnValue(new Promise<boolean>(() => {}))
        mockGetLastUsedPath.mockResolvedValue(undefined)

        const resultPromise = determineNavigationPath('root', '/', '/', otherPane)
        await vi.advanceTimersByTimeAsync(500)
        const result = await resultPromise

        expect(result).toBe('~')
    })

    it('runs pathExists checks in parallel', async () => {
        const otherPane: OtherPaneState = {
            otherPaneVolumeId: 'root',
            otherPanePath: '/Users/test/other',
        }

        const callTimestamps: number[] = []
        mockPathExists.mockImplementation((): Promise<boolean> => {
            callTimestamps.push(Date.now())
            return Promise.resolve(false)
        })
        mockGetLastUsedPath.mockResolvedValue('/Users/test/last')

        const resultPromise = determineNavigationPath('root', '/', '/', otherPane)
        await vi.advanceTimersByTimeAsync(500)
        await resultPromise

        // pathExists should have been called at least twice (otherPane + lastUsedPath)
        expect(mockPathExists).toHaveBeenCalledTimes(2)
        // Both calls should have started at the same tick (parallel via Promise.all)
        expect(callTimestamps[0]).toBe(callTimestamps[1])
    })
})

describe('resolveValidPath', () => {
    beforeEach(() => {
        vi.useFakeTimers()
    })

    afterEach(() => {
        vi.useRealTimers()
    })

    it('returns the target path if it exists', async () => {
        mockPathExists.mockResolvedValue(true)

        const resultPromise = resolveValidPath('/Users/test/documents')
        await vi.advanceTimersByTimeAsync(1000)
        const result = await resultPromise

        expect(result).toBe('/Users/test/documents')
        expect(mockPathExists).toHaveBeenCalledWith('/Users/test/documents')
    })

    it('walks up parent tree to find existing directory', async () => {
        mockPathExists.mockImplementation(
            (p: string): Promise<boolean> => (p === '/Users/test' ? Promise.resolve(true) : Promise.resolve(false)),
        )

        const resultPromise = resolveValidPath('/Users/test/documents/subfolder')
        await vi.advanceTimersByTimeAsync(3000)
        const result = await resultPromise

        expect(result).toBe('/Users/test')
        expect(mockPathExists).toHaveBeenCalledWith('/Users/test/documents/subfolder')
        expect(mockPathExists).toHaveBeenCalledWith('/Users/test/documents')
        expect(mockPathExists).toHaveBeenCalledWith('/Users/test')
    })

    it('falls back to ~ if no parent exists', async () => {
        mockPathExists.mockImplementation(
            (p: string): Promise<boolean> => (p === '~' ? Promise.resolve(true) : Promise.resolve(false)),
        )

        const resultPromise = resolveValidPath('/nonexistent/deep/path')
        await vi.advanceTimersByTimeAsync(5000)
        const result = await resultPromise

        expect(result).toBe('~')
    })

    it('falls back to / if ~ does not exist', async () => {
        mockPathExists.mockImplementation(
            (p: string): Promise<boolean> => (p === '/' ? Promise.resolve(true) : Promise.resolve(false)),
        )

        const resultPromise = resolveValidPath('/nonexistent/path')
        await vi.advanceTimersByTimeAsync(5000)
        const result = await resultPromise

        expect(result).toBe('/')
    })

    it('returns null if nothing exists (volume unmounted)', async () => {
        mockPathExists.mockResolvedValue(false)

        const resultPromise = resolveValidPath('/Volumes/Unmounted/data')
        await vi.advanceTimersByTimeAsync(10000)
        const result = await resultPromise

        expect(result).toBeNull()
    })

    it('each step respects timeout (slow pathExists treated as false)', async () => {
        // First call (target path) hangs, second call (parent) resolves true
        let callCount = 0
        mockPathExists.mockImplementation((): Promise<boolean> => {
            callCount++
            if (callCount === 1) return new Promise<boolean>(() => {}) // hangs
            if (callCount === 2) return Promise.resolve(true) // parent exists
            return Promise.resolve(false)
        })

        const resultPromise = resolveValidPath('/Users/test/hung-mount')
        // Advance past the first step's 1s timeout
        await vi.advanceTimersByTimeAsync(1000)
        // Second call should resolve immediately
        await vi.advanceTimersByTimeAsync(1000)
        const result = await resultPromise

        expect(result).toBe('/Users/test')
        expect(callCount).toBe(2)
    })
})
