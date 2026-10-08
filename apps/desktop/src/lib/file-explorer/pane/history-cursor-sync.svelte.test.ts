/**
 * Tests for `history-cursor-sync.svelte.ts`: the pane's side of per-history-entry
 * cursor memory. Uses Svelte runes, so the filename carries the `.svelte.` infix.
 */
import { describe, it, expect, vi, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import { createHistoryCursorSync, type HistoryCursorSync } from './history-cursor-sync.svelte'

const SNAP = 'search-results://sr-1'
const rows = [{ path: '/r/one.pdf' }, { path: '/r/two.pdf' }, { path: '/r/three.pdf' }]

describe('createHistoryCursorSync', () => {
  let dispose: (() => void) | undefined

  afterEach(() => {
    dispose?.()
    dispose = undefined
  })

  function create(opts: { location?: { volumeId: string; path: string } | null; snapshotRows?: typeof rows } = {}) {
    let cursorIndex = $state(0)
    let location = $state<{ volumeId: string; path: string } | null>(
      opts.location === undefined ? { volumeId: 'root', path: '/a' } : opts.location,
    )
    let snapshotRows = $state<typeof rows | undefined>(opts.snapshotRows)
    let currentPath = $state(location?.path ?? '/a')
    const onReading = vi.fn()
    const setCursorIndex = vi.fn((index: number) => {
      cursorIndex = index
    })
    let sync!: HistoryCursorSync
    dispose = $effect.root(() => {
      sync = createHistoryCursorSync({
        getCursorIndex: () => cursorIndex,
        getShownLocation: () => location,
        getSnapshotRows: () => snapshotRows,
        getCurrentPath: () => currentPath,
        setCursorIndex,
        onReading,
      })
    })
    flushSync()
    return {
      sync,
      onReading,
      setCursorIndex,
      moveCursor: (index: number) => {
        cursorIndex = index
        flushSync()
      },
      show: (next: { volumeId: string; path: string } | null, nextRows?: typeof rows) => {
        location = next
        snapshotRows = nextRows
        if (next) currentPath = next.path
        flushSync()
      },
    }
  }

  describe('reporting', () => {
    it('reports each cursor move with the listing it happened in', () => {
      const { onReading, moveCursor } = create()
      moveCursor(5)
      expect(onReading).toHaveBeenLastCalledWith({ volumeId: 'root', path: '/a', index: 5 })
    })

    it('stays quiet while no rows are settled on screen', () => {
      const { onReading, moveCursor } = create({ location: null })
      moveCursor(5)
      expect(onReading).not.toHaveBeenCalled()
    })

    it('reports a confirmed row only for the index the cursor is still on', () => {
      const { sync, onReading, moveCursor } = create()
      moveCursor(5)
      sync.reportRow(4, '/a/four')
      expect(onReading).not.toHaveBeenCalledWith(expect.objectContaining({ rowPath: '/a/four' }))
      sync.reportRow(5, '/a/five')
      expect(onReading).toHaveBeenLastCalledWith({ volumeId: 'root', path: '/a', index: 5, rowPath: '/a/five' })
    })
  })

  describe('restoring a listing', () => {
    it('hands the pending cursor to the load of its path, once', () => {
      const { sync } = create()
      sync.restore({ path: '/b', cursor: { index: 4 } })
      expect(sync.takeForLoad('/b')).toEqual({ index: 4 })
      expect(sync.takeForLoad('/b')).toBeUndefined()
    })

    it('drops the pending cursor when another path loads first', () => {
      const { sync } = create()
      sync.restore({ path: '/b', cursor: { index: 4 } })
      expect(sync.takeForLoad('/c')).toBeUndefined()
      expect(sync.takeForLoad('/b')).toBeUndefined()
    })
  })

  describe('restoring a snapshot', () => {
    it('lands on the remembered result once the snapshot is on screen', () => {
      const { sync, setCursorIndex, show } = create()
      sync.restore({ path: SNAP, cursor: { index: 0, rowPath: '/r/two.pdf' } })
      expect(setCursorIndex).not.toHaveBeenCalled()

      show({ volumeId: 'search-results', path: SNAP }, rows)
      expect(setCursorIndex).toHaveBeenCalledTimes(1)
      expect(setCursorIndex).toHaveBeenCalledWith(1)
    })

    it('falls back to the clamped index when the result is gone', () => {
      const { sync, setCursorIndex, show } = create()
      sync.restore({ path: SNAP, cursor: { index: 7, rowPath: '/r/gone.pdf' } })
      show({ volumeId: 'search-results', path: SNAP }, rows)
      expect(setCursorIndex).toHaveBeenCalledWith(2)
    })

    it('starts at the top when the entry remembers nothing', () => {
      const { sync, setCursorIndex, show, moveCursor } = create()
      moveCursor(11)
      sync.restore({ path: SNAP, cursor: undefined })
      show({ volumeId: 'search-results', path: SNAP }, rows)
      expect(setCursorIndex).toHaveBeenCalledWith(0)
    })

    it('applies once, so later snapshot changes leave the cursor to the pane', () => {
      const { sync, setCursorIndex, show } = create()
      sync.restore({ path: SNAP, cursor: { index: 1 } })
      show({ volumeId: 'search-results', path: SNAP }, rows)
      show({ volumeId: 'search-results', path: SNAP }, rows.slice(0, 2))
      expect(setCursorIndex).toHaveBeenCalledTimes(1)
    })
  })
})
