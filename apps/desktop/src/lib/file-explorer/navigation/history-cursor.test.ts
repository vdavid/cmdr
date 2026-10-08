import { describe, it, expect } from 'vitest'
import { recordCursor, restoredCursorIndex } from './history-cursor'
import { back, createHistory, getCurrentEntry, push, pushPath, type HistoryEntry } from './navigation-history'

const folderA = { volumeId: 'root', path: '/Users/me/a' }

describe('recordCursor', () => {
  it('records the index for the entry the reading came from', () => {
    const entry: HistoryEntry = { ...folderA }
    recordCursor(entry, { ...folderA, index: 7 })
    expect(entry.cursor).toEqual({ index: 7 })
  })

  it('ignores a reading from another listing, so a pane still showing B never writes into A', () => {
    const entry: HistoryEntry = { ...folderA, cursor: { index: 3, rowPath: '/Users/me/a/three' } }
    recordCursor(entry, { volumeId: 'root', path: '/Users/me/b', index: 11 })
    recordCursor(entry, { volumeId: 'ext', path: '/Users/me/a', index: 11 })
    expect(entry.cursor).toEqual({ index: 3, rowPath: '/Users/me/a/three' })
  })

  it('drops the row identity when the index moves, until the new row is confirmed', () => {
    const entry: HistoryEntry = { ...folderA, cursor: { index: 3, rowPath: '/Users/me/a/three' } }
    recordCursor(entry, { ...folderA, index: 4 })
    expect(entry.cursor).toEqual({ index: 4 })
  })

  it('keeps the row identity when the index reading repeats', () => {
    const entry: HistoryEntry = { ...folderA, cursor: { index: 3, rowPath: '/Users/me/a/three' } }
    recordCursor(entry, { ...folderA, index: 3 })
    expect(entry.cursor).toEqual({ index: 3, rowPath: '/Users/me/a/three' })
  })

  it('attaches a confirmed row only to the index it was read at', () => {
    const entry: HistoryEntry = { ...folderA, cursor: { index: 4 } }
    // A stale read for the row the cursor already left must not stick.
    recordCursor(entry, { ...folderA, index: 3, rowPath: '/Users/me/a/three' })
    expect(entry.cursor).toEqual({ index: 4 })
    recordCursor(entry, { ...folderA, index: 4, rowPath: '/Users/me/a/four' })
    expect(entry.cursor).toEqual({ index: 4, rowPath: '/Users/me/a/four' })
  })

  it('tells repeated visits of the same folder apart: each history position keeps its own cursor', () => {
    let history = createHistory('root', '/Users/me/a')
    recordCursor(getCurrentEntry(history), { ...folderA, index: 2 })
    history = pushPath(history, '/Users/me/b')
    history = pushPath(history, '/Users/me/a')
    recordCursor(getCurrentEntry(history), { ...folderA, index: 9 })

    expect(history.stack[0].cursor).toEqual({ index: 2 })
    expect(history.stack[2].cursor).toEqual({ index: 9 })
  })

  it('survives back and a deduplicated push, and a new entry starts without one', () => {
    let history = createHistory('root', '/Users/me/a')
    recordCursor(getCurrentEntry(history), { ...folderA, index: 5 })
    history = push(history, { ...folderA }).history
    expect(getCurrentEntry(history).cursor).toEqual({ index: 5 })

    history = pushPath(history, '/Users/me/b')
    expect(getCurrentEntry(history).cursor).toBeUndefined()
    expect(getCurrentEntry(back(history)).cursor).toEqual({ index: 5 })
  })
})

describe('restoredCursorIndex', () => {
  it('starts at the top when nothing was remembered', () => {
    expect(restoredCursorIndex(undefined, undefined, 20)).toBe(0)
  })

  it('prefers where the remembered row is now', () => {
    expect(restoredCursorIndex({ index: 4, rowPath: '/a/x' }, 9, 20)).toBe(9)
  })

  it('falls back to the remembered index when the row is gone', () => {
    expect(restoredCursorIndex({ index: 4, rowPath: '/a/x' }, undefined, 20)).toBe(4)
  })

  it('clamps the remembered index into a shorter listing', () => {
    expect(restoredCursorIndex({ index: 30 }, undefined, 12)).toBe(11)
  })

  it('lands on 0 in an empty listing', () => {
    expect(restoredCursorIndex({ index: 30 }, undefined, 0)).toBe(0)
  })

  it('ignores a found index outside the rows', () => {
    expect(restoredCursorIndex({ index: 2 }, 50, 10)).toBe(2)
  })
})
