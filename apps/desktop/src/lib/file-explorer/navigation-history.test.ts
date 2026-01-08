import { describe, it, expect } from 'vitest'
import {
    createHistory,
    push,
    pushPath,
    back,
    forward,
    getCurrentPath,
    getCurrentEntry,
    canGoBack,
    canGoForward,
    setCurrentIndex,
    getEntryAt,
} from './navigation-history'

const ROOT_VOLUME = 'root'
const EXT_VOLUME = '/Volumes/External'

describe('NavigationHistory', () => {
    describe('createHistory', () => {
        it('creates history with initial entry at index 0', () => {
            const history = createHistory(ROOT_VOLUME, '/home')
            expect(history.stack).toEqual([{ volumeId: ROOT_VOLUME, path: '/home' }])
            expect(history.currentIndex).toBe(0)
        })
    })

    describe('push', () => {
        it('adds entry to stack and updates index', () => {
            const h1 = createHistory(ROOT_VOLUME, '/a')
            const h2 = push(h1, { volumeId: ROOT_VOLUME, path: '/b' })
            expect(h2.stack).toEqual([
                { volumeId: ROOT_VOLUME, path: '/a' },
                { volumeId: ROOT_VOLUME, path: '/b' },
            ])
            expect(h2.currentIndex).toBe(1)
        })

        it('does not duplicate if pushing same entry', () => {
            const h1 = createHistory(ROOT_VOLUME, '/a')
            const h2 = push(h1, { volumeId: ROOT_VOLUME, path: '/a' })
            expect(h2).toBe(h1)
        })

        it('allows same path but different volume', () => {
            const h1 = createHistory(ROOT_VOLUME, '/data')
            const h2 = push(h1, { volumeId: EXT_VOLUME, path: '/data' })
            expect(h2.stack).toHaveLength(2)
            expect(h2.currentIndex).toBe(1)
        })

        it('truncates forward history when pushing after back', () => {
            // Navigate: /a -> /b -> /c, then back, then push /d
            let history = createHistory(ROOT_VOLUME, '/a')
            history = push(history, { volumeId: ROOT_VOLUME, path: '/b' })
            history = push(history, { volumeId: ROOT_VOLUME, path: '/c' })
            history = back(history) // now at /b
            history = push(history, { volumeId: ROOT_VOLUME, path: '/d' })

            expect(history.stack.map((e) => e.path)).toEqual(['/a', '/b', '/d'])
            expect(history.currentIndex).toBe(2)
            expect(getCurrentPath(history)).toBe('/d')
        })
    })

    describe('pushPath', () => {
        it('keeps current volumeId when pushing just a path', () => {
            let history = createHistory(EXT_VOLUME, '/start')
            history = pushPath(history, '/folder')
            expect(getCurrentEntry(history)).toEqual({ volumeId: EXT_VOLUME, path: '/folder' })
        })
    })

    describe('back', () => {
        it('decrements index', () => {
            let history = createHistory(ROOT_VOLUME, '/a')
            history = push(history, { volumeId: ROOT_VOLUME, path: '/b' })
            history = back(history)
            expect(history.currentIndex).toBe(0)
            expect(getCurrentPath(history)).toBe('/a')
        })

        it('returns unchanged when at oldest entry', () => {
            const history = createHistory(ROOT_VOLUME, '/a')
            const result = back(history)
            expect(result).toBe(history)
            expect(result.currentIndex).toBe(0)
        })
    })

    describe('forward', () => {
        it('increments index', () => {
            let history = createHistory(ROOT_VOLUME, '/a')
            history = push(history, { volumeId: ROOT_VOLUME, path: '/b' })
            history = back(history) // at /a
            history = forward(history) // at /b
            expect(history.currentIndex).toBe(1)
            expect(getCurrentPath(history)).toBe('/b')
        })

        it('returns unchanged when at newest entry', () => {
            const history = createHistory(ROOT_VOLUME, '/a')
            const result = forward(history)
            expect(result).toBe(history)
            expect(result.currentIndex).toBe(0)
        })
    })

    describe('canGoBack', () => {
        it('returns false at oldest entry', () => {
            const history = createHistory(ROOT_VOLUME, '/a')
            expect(canGoBack(history)).toBe(false)
        })

        it('returns true when history exists', () => {
            let history = createHistory(ROOT_VOLUME, '/a')
            history = push(history, { volumeId: ROOT_VOLUME, path: '/b' })
            expect(canGoBack(history)).toBe(true)
        })
    })

    describe('canGoForward', () => {
        it('returns false at newest entry', () => {
            let history = createHistory(ROOT_VOLUME, '/a')
            history = push(history, { volumeId: ROOT_VOLUME, path: '/b' })
            expect(canGoForward(history)).toBe(false)
        })

        it('returns true after going back', () => {
            let history = createHistory(ROOT_VOLUME, '/a')
            history = push(history, { volumeId: ROOT_VOLUME, path: '/b' })
            history = back(history)
            expect(canGoForward(history)).toBe(true)
        })
    })

    describe('getCurrentPath', () => {
        it('returns the path at currentIndex', () => {
            let history = createHistory(ROOT_VOLUME, '/a')
            history = push(history, { volumeId: ROOT_VOLUME, path: '/b' })
            history = push(history, { volumeId: ROOT_VOLUME, path: '/c' })
            expect(getCurrentPath(history)).toBe('/c')
            history = back(history)
            expect(getCurrentPath(history)).toBe('/b')
        })
    })

    describe('getCurrentEntry', () => {
        it('returns the full entry at currentIndex', () => {
            let history = createHistory(ROOT_VOLUME, '/a')
            history = push(history, { volumeId: EXT_VOLUME, path: '/b' })
            expect(getCurrentEntry(history)).toEqual({ volumeId: EXT_VOLUME, path: '/b' })
        })
    })

    describe('getEntryAt', () => {
        it('returns entry at specified index', () => {
            let history = createHistory(ROOT_VOLUME, '/a')
            history = push(history, { volumeId: EXT_VOLUME, path: '/b' })
            expect(getEntryAt(history, 0)).toEqual({ volumeId: ROOT_VOLUME, path: '/a' })
            expect(getEntryAt(history, 1)).toEqual({ volumeId: EXT_VOLUME, path: '/b' })
        })

        it('returns undefined for out-of-bounds index', () => {
            const history = createHistory(ROOT_VOLUME, '/a')
            expect(getEntryAt(history, 5)).toBeUndefined()
            expect(getEntryAt(history, -1)).toBeUndefined()
        })
    })

    describe('setCurrentIndex', () => {
        it('sets the current index', () => {
            let history = createHistory(ROOT_VOLUME, '/a')
            history = push(history, { volumeId: ROOT_VOLUME, path: '/b' })
            history = push(history, { volumeId: ROOT_VOLUME, path: '/c' })
            history = setCurrentIndex(history, 0)
            expect(history.currentIndex).toBe(0)
            expect(getCurrentPath(history)).toBe('/a')
        })

        it('clamps to valid range', () => {
            let history = createHistory(ROOT_VOLUME, '/a')
            history = push(history, { volumeId: ROOT_VOLUME, path: '/b' })
            expect(setCurrentIndex(history, 100).currentIndex).toBe(1)
            expect(setCurrentIndex(history, -5).currentIndex).toBe(0)
        })

        it('returns unchanged if index is the same', () => {
            let history = createHistory(ROOT_VOLUME, '/a')
            history = push(history, { volumeId: ROOT_VOLUME, path: '/b' })
            const result = setCurrentIndex(history, 1)
            expect(result).toBe(history)
        })
    })

    describe('volume switching', () => {
        it('tracks navigation across different volumes', () => {
            let h = createHistory(ROOT_VOLUME, '/home')
            h = push(h, { volumeId: ROOT_VOLUME, path: '/home/docs' })
            h = push(h, { volumeId: EXT_VOLUME, path: '/data' }) // switch volume
            h = push(h, { volumeId: EXT_VOLUME, path: '/data/backup' })

            expect(h.stack.map((e) => e.volumeId)).toEqual([ROOT_VOLUME, ROOT_VOLUME, EXT_VOLUME, EXT_VOLUME])

            // Go back to root volume
            h = back(h)
            h = back(h)
            expect(getCurrentEntry(h).volumeId).toBe(ROOT_VOLUME)
            expect(getCurrentPath(h)).toBe('/home/docs')
        })

        it('preserves volume info after going back and forward', () => {
            let h = createHistory(ROOT_VOLUME, '/a')
            h = push(h, { volumeId: EXT_VOLUME, path: '/b' })
            h = back(h)
            h = forward(h)
            expect(getCurrentEntry(h)).toEqual({ volumeId: EXT_VOLUME, path: '/b' })
        })
    })

    describe('network volume navigation', () => {
        const networkHost = { id: 'server1', name: 'server1', hostname: 'server1.local', port: 445 }

        it('tracks network host selection in history', () => {
            let h = createHistory('network', 'smb://')
            h = push(h, { volumeId: 'network', path: 'smb://', networkHost })

            expect(h.stack).toHaveLength(2)
            expect(getCurrentEntry(h).networkHost).toEqual(networkHost)
        })

        it('distinguishes different network hosts', () => {
            const host1 = { id: 'server1', name: 'server1', hostname: 'server1.local', port: 445 }
            const host2 = { id: 'server2', name: 'server2', hostname: 'server2.local', port: 445 }

            let h = createHistory('network', 'smb://')
            h = push(h, { volumeId: 'network', path: 'smb://', networkHost: host1 })
            h = push(h, { volumeId: 'network', path: 'smb://', networkHost: host2 })

            expect(h.stack).toHaveLength(3) // root, host1, host2
            expect(h.stack[1].networkHost).toEqual(host1)
            expect(h.stack[2].networkHost).toEqual(host2)
        })

        it('does not duplicate identical network host entries', () => {
            const host = { id: 'server1', name: 'server1', hostname: 'server1.local', port: 445 }

            let h = createHistory('network', 'smb://')
            h = push(h, { volumeId: 'network', path: 'smb://', networkHost: host })
            const before = h
            h = push(h, { volumeId: 'network', path: 'smb://', networkHost: host })

            expect(h).toBe(before) // Unchanged
        })
    })

    describe('complex navigation sequences', () => {
        it('handles navigation-after-back correctly', () => {
            // Start at /a, go to /b, /c, /d
            // Go back twice (to /b)
            // Navigate to /e - should clear /c, /d from forward history
            let h = createHistory(ROOT_VOLUME, '/a')
            h = push(h, { volumeId: ROOT_VOLUME, path: '/b' })
            h = push(h, { volumeId: ROOT_VOLUME, path: '/c' })
            h = push(h, { volumeId: ROOT_VOLUME, path: '/d' })
            expect(h.stack.map((e) => e.path)).toEqual(['/a', '/b', '/c', '/d'])

            h = back(h) // /c
            h = back(h) // /b
            expect(getCurrentPath(h)).toBe('/b')

            h = push(h, { volumeId: ROOT_VOLUME, path: '/e' })
            expect(h.stack.map((e) => e.path)).toEqual(['/a', '/b', '/e'])
            expect(getCurrentPath(h)).toBe('/e')
            expect(canGoForward(h)).toBe(false)
        })

        it('handles multiple back-forward cycles', () => {
            let h = createHistory(ROOT_VOLUME, '/a')
            h = push(h, { volumeId: ROOT_VOLUME, path: '/b' })

            // Cycle multiple times
            h = back(h)
            expect(getCurrentPath(h)).toBe('/a')
            h = forward(h)
            expect(getCurrentPath(h)).toBe('/b')
            h = back(h)
            expect(getCurrentPath(h)).toBe('/a')

            // Stack should be unchanged
            expect(h.stack.map((e) => e.path)).toEqual(['/a', '/b'])
        })

        it('handles complex volume-switching sequence with back/forward', () => {
            // Simulate: browse root, switch to external, browse, switch to network, go back
            let h = createHistory(ROOT_VOLUME, '~')
            h = push(h, { volumeId: ROOT_VOLUME, path: '/Users/test' })
            h = push(h, { volumeId: EXT_VOLUME, path: '/Volumes/External' }) // volume switch
            h = push(h, { volumeId: EXT_VOLUME, path: '/Volumes/External/data' })
            h = push(h, { volumeId: 'network', path: 'smb://' }) // to network

            // Go back three times
            h = back(h) // external/data
            h = back(h) // external root
            h = back(h) // /Users/test

            expect(getCurrentEntry(h)).toEqual({ volumeId: ROOT_VOLUME, path: '/Users/test' })

            // Forward twice
            h = forward(h)
            h = forward(h)
            expect(getCurrentEntry(h)).toEqual({ volumeId: EXT_VOLUME, path: '/Volumes/External/data' })
        })
    })
})
