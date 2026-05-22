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
  MAX_HISTORY_PER_TAB,
  type NavigationHistory,
  type HistoryEntry,
} from './navigation-history'

const ROOT_VOLUME = 'root'
const EXT_VOLUME = '/Volumes/External'

/** Test helper: push and return the new history (discarding droppedEntries). */
function pushH(history: NavigationHistory, entry: HistoryEntry): NavigationHistory {
  return push(history, entry).history
}

describe('NavigationHistory', () => {
  describe('createHistory', () => {
    it('creates history with initial entry at index 0', () => {
      const history = createHistory(ROOT_VOLUME, '/home')
      expect(history.stack).toEqual([{ volumeId: ROOT_VOLUME, path: '/home' }])
      expect(history.currentIndex).toBe(0)
    })
  })

  describe('push', () => {
    it('adds entry to stack and updates index, with empty droppedEntries', () => {
      const h1 = createHistory(ROOT_VOLUME, '/a')
      const result = push(h1, { volumeId: ROOT_VOLUME, path: '/b' })
      expect(result.history.stack).toEqual([
        { volumeId: ROOT_VOLUME, path: '/a' },
        { volumeId: ROOT_VOLUME, path: '/b' },
      ])
      expect(result.history.currentIndex).toBe(1)
      expect(result.droppedEntries).toEqual([])
    })

    it('does not duplicate if pushing same entry, returns same reference', () => {
      const h1 = createHistory(ROOT_VOLUME, '/a')
      const result = push(h1, { volumeId: ROOT_VOLUME, path: '/a' })
      expect(result.history).toBe(h1)
      expect(result.droppedEntries).toEqual([])
    })

    it('allows same path but different volume', () => {
      const h1 = createHistory(ROOT_VOLUME, '/data')
      const result = push(h1, { volumeId: EXT_VOLUME, path: '/data' })
      expect(result.history.stack).toHaveLength(2)
      expect(result.history.currentIndex).toBe(1)
      expect(result.droppedEntries).toEqual([])
    })

    it('truncates forward history when pushing after back', () => {
      // Navigate: /a -> /b -> /c, then back, then push /d
      let history = createHistory(ROOT_VOLUME, '/a')
      history = pushH(history, { volumeId: ROOT_VOLUME, path: '/b' })
      history = pushH(history, { volumeId: ROOT_VOLUME, path: '/c' })
      history = back(history) // now at /b
      const result = push(history, { volumeId: ROOT_VOLUME, path: '/d' })

      expect(result.history.stack.map((e) => e.path)).toEqual(['/a', '/b', '/d'])
      expect(result.history.currentIndex).toBe(2)
      expect(getCurrentPath(result.history)).toBe('/d')
      // The truncated `/c` is reported in droppedEntries.
      expect(result.droppedEntries).toEqual([{ volumeId: ROOT_VOLUME, path: '/c' }])
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
      history = pushH(history, { volumeId: ROOT_VOLUME, path: '/b' })
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
      history = pushH(history, { volumeId: ROOT_VOLUME, path: '/b' })
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
      history = pushH(history, { volumeId: ROOT_VOLUME, path: '/b' })
      expect(canGoBack(history)).toBe(true)
    })
  })

  describe('canGoForward', () => {
    it('returns false at newest entry', () => {
      let history = createHistory(ROOT_VOLUME, '/a')
      history = pushH(history, { volumeId: ROOT_VOLUME, path: '/b' })
      expect(canGoForward(history)).toBe(false)
    })

    it('returns true after going back', () => {
      let history = createHistory(ROOT_VOLUME, '/a')
      history = pushH(history, { volumeId: ROOT_VOLUME, path: '/b' })
      history = back(history)
      expect(canGoForward(history)).toBe(true)
    })
  })

  describe('getCurrentPath', () => {
    it('returns the path at currentIndex', () => {
      let history = createHistory(ROOT_VOLUME, '/a')
      history = pushH(history, { volumeId: ROOT_VOLUME, path: '/b' })
      history = pushH(history, { volumeId: ROOT_VOLUME, path: '/c' })
      expect(getCurrentPath(history)).toBe('/c')
      history = back(history)
      expect(getCurrentPath(history)).toBe('/b')
    })
  })

  describe('getCurrentEntry', () => {
    it('returns the full entry at currentIndex', () => {
      let history = createHistory(ROOT_VOLUME, '/a')
      history = pushH(history, { volumeId: EXT_VOLUME, path: '/b' })
      expect(getCurrentEntry(history)).toEqual({ volumeId: EXT_VOLUME, path: '/b' })
    })
  })

  describe('getEntryAt', () => {
    it('returns entry at specified index', () => {
      let history = createHistory(ROOT_VOLUME, '/a')
      history = pushH(history, { volumeId: EXT_VOLUME, path: '/b' })
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
      history = pushH(history, { volumeId: ROOT_VOLUME, path: '/b' })
      history = pushH(history, { volumeId: ROOT_VOLUME, path: '/c' })
      history = setCurrentIndex(history, 0)
      expect(history.currentIndex).toBe(0)
      expect(getCurrentPath(history)).toBe('/a')
    })

    it('clamps to valid range', () => {
      let history = createHistory(ROOT_VOLUME, '/a')
      history = pushH(history, { volumeId: ROOT_VOLUME, path: '/b' })
      expect(setCurrentIndex(history, 100).currentIndex).toBe(1)
      expect(setCurrentIndex(history, -5).currentIndex).toBe(0)
    })

    it('returns unchanged if index is the same', () => {
      let history = createHistory(ROOT_VOLUME, '/a')
      history = pushH(history, { volumeId: ROOT_VOLUME, path: '/b' })
      const result = setCurrentIndex(history, 1)
      expect(result).toBe(history)
    })
  })

  describe('volume switching', () => {
    it('tracks navigation across different volumes', () => {
      let h = createHistory(ROOT_VOLUME, '/home')
      h = pushH(h, { volumeId: ROOT_VOLUME, path: '/home/docs' })
      h = pushH(h, { volumeId: EXT_VOLUME, path: '/data' }) // switch volume
      h = pushH(h, { volumeId: EXT_VOLUME, path: '/data/backup' })

      expect(h.stack.map((e) => e.volumeId)).toEqual([ROOT_VOLUME, ROOT_VOLUME, EXT_VOLUME, EXT_VOLUME])

      // Go back to root volume
      h = back(h)
      h = back(h)
      expect(getCurrentEntry(h).volumeId).toBe(ROOT_VOLUME)
      expect(getCurrentPath(h)).toBe('/home/docs')
    })

    it('preserves volume info after going back and forward', () => {
      let h = createHistory(ROOT_VOLUME, '/a')
      h = pushH(h, { volumeId: EXT_VOLUME, path: '/b' })
      h = back(h)
      h = forward(h)
      expect(getCurrentEntry(h)).toEqual({ volumeId: EXT_VOLUME, path: '/b' })
    })
  })

  describe('network volume navigation', () => {
    const networkHost = { id: 'server1', name: 'server1', hostname: 'server1.local', port: 445 }

    it('tracks active network host in history', () => {
      let h = createHistory('network', 'smb://')
      h = pushH(h, { volumeId: 'network', path: 'smb://', networkHost })

      expect(h.stack).toHaveLength(2)
      expect(getCurrentEntry(h).networkHost).toEqual(networkHost)
    })

    it('distinguishes different network hosts', () => {
      const host1 = { id: 'server1', name: 'server1', hostname: 'server1.local', port: 445 }
      const host2 = { id: 'server2', name: 'server2', hostname: 'server2.local', port: 445 }

      let h = createHistory('network', 'smb://')
      h = pushH(h, { volumeId: 'network', path: 'smb://', networkHost: host1 })
      h = pushH(h, { volumeId: 'network', path: 'smb://', networkHost: host2 })

      expect(h.stack).toHaveLength(3) // root, host1, host2
      expect(h.stack[1].networkHost).toEqual(host1)
      expect(h.stack[2].networkHost).toEqual(host2)
    })

    it('does not duplicate identical network host entries', () => {
      const host = { id: 'server1', name: 'server1', hostname: 'server1.local', port: 445 }

      let h = createHistory('network', 'smb://')
      h = pushH(h, { volumeId: 'network', path: 'smb://', networkHost: host })
      const before = h
      h = pushH(h, { volumeId: 'network', path: 'smb://', networkHost: host })

      expect(h).toBe(before) // Unchanged
    })
  })

  describe('complex navigation sequences', () => {
    it('handles navigation-after-back correctly', () => {
      // Start at /a, go to /b, /c, /d
      // Go back twice (to /b)
      // Navigate to /e - should clear /c, /d from forward history
      let h = createHistory(ROOT_VOLUME, '/a')
      h = pushH(h, { volumeId: ROOT_VOLUME, path: '/b' })
      h = pushH(h, { volumeId: ROOT_VOLUME, path: '/c' })
      h = pushH(h, { volumeId: ROOT_VOLUME, path: '/d' })
      expect(h.stack.map((e) => e.path)).toEqual(['/a', '/b', '/c', '/d'])

      h = back(h) // /c
      h = back(h) // /b
      expect(getCurrentPath(h)).toBe('/b')

      h = pushH(h, { volumeId: ROOT_VOLUME, path: '/e' })
      expect(h.stack.map((e) => e.path)).toEqual(['/a', '/b', '/e'])
      expect(getCurrentPath(h)).toBe('/e')
      expect(canGoForward(h)).toBe(false)
    })

    it('handles multiple back-forward cycles', () => {
      let h = createHistory(ROOT_VOLUME, '/a')
      h = pushH(h, { volumeId: ROOT_VOLUME, path: '/b' })

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
      h = pushH(h, { volumeId: ROOT_VOLUME, path: '/Users/test' })
      h = pushH(h, { volumeId: EXT_VOLUME, path: '/Volumes/External' }) // volume switch
      h = pushH(h, { volumeId: EXT_VOLUME, path: '/Volumes/External/data' })
      h = pushH(h, { volumeId: 'network', path: 'smb://' }) // to network

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

  // ---------------------------------------------------------------------------
  // MAX_HISTORY_PER_TAB cap (M8a)
  // ---------------------------------------------------------------------------

  describe('MAX_HISTORY_PER_TAB cap', () => {
    it('exposes the cap as a public constant', () => {
      expect(MAX_HISTORY_PER_TAB).toBe(100)
    })

    it('caps the stack at MAX_HISTORY_PER_TAB by dropping the oldest entries', () => {
      // Push MAX + 1 unique entries; expect length capped at MAX and oldest gone.
      let history = createHistory(ROOT_VOLUME, '/p0')
      // Already has 1 entry. Push MAX more (each unique) to overflow by 1.
      for (let i = 1; i <= MAX_HISTORY_PER_TAB; i++) {
        history = pushH(history, { volumeId: ROOT_VOLUME, path: `/p${String(i)}` })
      }

      expect(history.stack).toHaveLength(MAX_HISTORY_PER_TAB)
      // Oldest entry `/p0` should be gone; `/p1` is now at index 0.
      expect(history.stack[0].path).toBe('/p1')
      // Newest entry stays at the end.
      expect(history.stack[history.stack.length - 1].path).toBe(`/p${String(MAX_HISTORY_PER_TAB)}`)
      // currentIndex points at the newest entry.
      expect(history.currentIndex).toBe(MAX_HISTORY_PER_TAB - 1)
    })

    it('returns the oldest evicted entries in droppedEntries', () => {
      // Fill to cap exactly, then push one more — should drop exactly `/p0`.
      let history = createHistory(ROOT_VOLUME, '/p0')
      for (let i = 1; i < MAX_HISTORY_PER_TAB; i++) {
        history = pushH(history, { volumeId: ROOT_VOLUME, path: `/p${String(i)}` })
      }
      expect(history.stack).toHaveLength(MAX_HISTORY_PER_TAB)

      const result = push(history, { volumeId: ROOT_VOLUME, path: '/overflow' })
      expect(result.history.stack).toHaveLength(MAX_HISTORY_PER_TAB)
      expect(result.droppedEntries).toEqual([{ volumeId: ROOT_VOLUME, path: '/p0' }])
    })

    it('reports truncated-forward entries in droppedEntries', () => {
      // /a -> /b -> /c, back twice to /a, then push /d -> droppedEntries = [/b, /c]
      let history = createHistory(ROOT_VOLUME, '/a')
      history = pushH(history, { volumeId: ROOT_VOLUME, path: '/b' })
      history = pushH(history, { volumeId: ROOT_VOLUME, path: '/c' })
      history = back(history)
      history = back(history)
      const result = push(history, { volumeId: ROOT_VOLUME, path: '/d' })

      expect(result.history.stack.map((e) => e.path)).toEqual(['/a', '/d'])
      expect(result.droppedEntries).toEqual([
        { volumeId: ROOT_VOLUME, path: '/b' },
        { volumeId: ROOT_VOLUME, path: '/c' },
      ])
    })

    it('handles mixed past-cap-and-truncated-forward across a sequence of pushes', () => {
      // The single-push "both kinds" case is mathematically impossible at the
      // boundary: truncating the forward never *grows* the stack above its current
      // length, and the new entry takes one of the now-vacated slots. To hit the
      // cap with a single push, the stack must already be at the cap with the
      // cursor at the rightmost slot — in which case the forward stack is empty.
      // What IS observable is the cumulative effect across a sequence: cap fills,
      // user goes back deep, then forward-truncates with new pushes that
      // eventually overflow the cap. Both kinds appear over the sequence.
      let history = createHistory(ROOT_VOLUME, '/p0')
      for (let i = 1; i < MAX_HISTORY_PER_TAB; i++) {
        history = pushH(history, { volumeId: ROOT_VOLUME, path: `/p${String(i)}` })
      }
      // Go back 5 — currentIndex is now MAX-6. Forward stack is /p95.../p99.
      for (let i = 0; i < 5; i++) history = back(history)

      // Push /n0 — truncates 5 forward entries, no overflow yet (length MAX-5+1).
      let totalDropped: HistoryEntry[] = []
      let r = push(history, { volumeId: ROOT_VOLUME, path: '/n0' })
      totalDropped = totalDropped.concat(r.droppedEntries)
      history = r.history
      expect(history.stack).toHaveLength(MAX_HISTORY_PER_TAB - 4)

      // Push another 5 entries — eventually back to the cap, then one more
      // overflows the front.
      for (let i = 1; i <= 5; i++) {
        r = push(history, { volumeId: ROOT_VOLUME, path: `/n${String(i)}` })
        totalDropped = totalDropped.concat(r.droppedEntries)
        history = r.history
      }
      // Length capped.
      expect(history.stack).toHaveLength(MAX_HISTORY_PER_TAB)
      // Cumulative droppedEntries: the 5 truncated-forward (/p95–/p99) and the 1
      // evicted-oldest (/p0).
      const droppedPaths = totalDropped.map((e) => e.path).sort()
      expect(droppedPaths).toContain('/p0')
      for (const p of ['/p95', '/p96', '/p97', '/p98', '/p99']) {
        expect(droppedPaths).toContain(p)
      }
    })

    it('caps history across mixed volumes (not just one volume)', () => {
      // Push entries that alternate between volumes; cap still applies globally.
      let history = createHistory(ROOT_VOLUME, '/r0')
      for (let i = 1; i < MAX_HISTORY_PER_TAB; i++) {
        const volume = i % 2 === 0 ? ROOT_VOLUME : EXT_VOLUME
        history = pushH(history, { volumeId: volume, path: `/p${String(i)}` })
      }
      // Length is exactly MAX. Push one more on a third "volume" (network).
      const result = push(history, { volumeId: 'network', path: 'smb://' })
      expect(result.history.stack).toHaveLength(MAX_HISTORY_PER_TAB)
      // Oldest (`/r0` on root) is gone.
      expect(result.droppedEntries).toEqual([{ volumeId: ROOT_VOLUME, path: '/r0' }])
      // The most recent entry on the new volume is at the end.
      expect(result.history.stack[result.history.stack.length - 1]).toEqual({
        volumeId: 'network',
        path: 'smb://',
      })
    })

    it('pushPath delegates and discards droppedEntries (backwards-compatible shape)', () => {
      // Even at overflow, pushPath returns only the history; the evicted entry is
      // simply lost from the caller's view. Refcounting callers must use push().
      let history = createHistory(ROOT_VOLUME, '/p0')
      for (let i = 1; i < MAX_HISTORY_PER_TAB; i++) {
        history = pushH(history, { volumeId: ROOT_VOLUME, path: `/p${String(i)}` })
      }
      const result: NavigationHistory = pushPath(history, '/overflow')
      // No droppedEntries field on the return; the shape is bare NavigationHistory.
      expect(result.stack).toHaveLength(MAX_HISTORY_PER_TAB)
      expect(result.stack[0].path).toBe('/p1')
      expect(result.stack[result.stack.length - 1].path).toBe('/overflow')
    })
  })
})
