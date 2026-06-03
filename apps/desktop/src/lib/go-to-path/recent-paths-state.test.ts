import { describe, it, expect, vi, beforeEach } from 'vitest'

// The state mirror re-reads the authoritative list from the backend after every
// write, so the dedupe / move-to-top / cap behavior is the BACKEND's. Here we
// stand up an in-memory fake backend that mirrors `go_to_path/history.rs`
// (dedupe by path, move-to-top, cap 10) and assert the `$state` mirror reflects
// it after add / remove.

const { getRecentPathsMock, addRecentPathMock, removeRecentPathMock } = vi.hoisted(() => ({
  getRecentPathsMock: vi.fn(),
  addRecentPathMock: vi.fn(),
  removeRecentPathMock: vi.fn(),
}))

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    getRecentPaths: getRecentPathsMock,
    addRecentPath: addRecentPathMock,
    removeRecentPath: removeRecentPathMock,
  },
}))

import {
  getRecentPathsList,
  getRecentPathsLoaded,
  loadRecentPaths,
  addRecentPath,
  removeRecentPath,
  resetRecentPathsForTests,
} from './recent-paths-state.svelte'
import type { RecentPathEntry } from '$lib/ipc/bindings'

const CAP = 10

/** In-memory stand-in for the backend store. */
let store: RecentPathEntry[] = []

function backendAdd(entry: RecentPathEntry): void {
  // Dedupe by resolved path (drop any existing entry for the same path).
  store = store.filter((e) => e.path !== entry.path)
  // Move-to-top: newest first.
  store.unshift(entry)
  // Cap.
  if (store.length > CAP) store = store.slice(0, CAP)
}

function backendRemove(id: string): void {
  store = store.filter((e) => e.id !== id)
}

function makeEntry(path: string, id = path, timestamp = 1): RecentPathEntry {
  return { id, path, timestamp }
}

describe('recent-paths-state mirror', () => {
  beforeEach(() => {
    store = []
    resetRecentPathsForTests()
    getRecentPathsMock.mockReset().mockImplementation(() => Promise.resolve([...store]))
    addRecentPathMock.mockReset().mockImplementation((entry: RecentPathEntry) => {
      backendAdd(entry)
      return Promise.resolve({ status: 'ok', data: null })
    })
    removeRecentPathMock.mockReset().mockImplementation((id: string) => {
      backendRemove(id)
      return Promise.resolve({ status: 'ok', data: null })
    })
  })

  it('load() pulls the backend list and marks loaded', async () => {
    store = [makeEntry('/a'), makeEntry('/b')]
    expect(getRecentPathsLoaded()).toBe(false)
    await loadRecentPaths()
    expect(getRecentPathsList().map((e) => e.path)).toEqual(['/a', '/b'])
    expect(getRecentPathsLoaded()).toBe(true)
  })

  it('load() is idempotent unless forced', async () => {
    store = [makeEntry('/a')]
    await loadRecentPaths()
    store = [makeEntry('/b')]
    await loadRecentPaths() // No-op: already loaded.
    expect(getRecentPathsList().map((e) => e.path)).toEqual(['/a'])
    await loadRecentPaths(true) // Forced re-read.
    expect(getRecentPathsList().map((e) => e.path)).toEqual(['/b'])
  })

  it('add reflects newest-first ordering', async () => {
    await addRecentPath(makeEntry('/a'))
    await addRecentPath(makeEntry('/b'))
    expect(getRecentPathsList().map((e) => e.path)).toEqual(['/b', '/a'])
  })

  it('add dedupes by path and moves the existing entry to the top', async () => {
    await addRecentPath(makeEntry('/a'))
    await addRecentPath(makeEntry('/b'))
    await addRecentPath(makeEntry('/a', 'a-again', 2))
    expect(getRecentPathsList().map((e) => e.path)).toEqual(['/a', '/b'])
    expect(getRecentPathsList()).toHaveLength(2)
  })

  it('add caps the mirror at 10, evicting the oldest', async () => {
    for (let i = 0; i < 12; i++) {
      await addRecentPath(makeEntry(`/p${String(i)}`))
    }
    const list = getRecentPathsList()
    expect(list).toHaveLength(10)
    // Newest first; the two oldest (/p0, /p1) are evicted.
    expect(list[0].path).toBe('/p11')
    expect(list.at(-1)?.path).toBe('/p2')
    expect(list.map((e) => e.path)).not.toContain('/p0')
  })

  it('remove drops the entry from the mirror', async () => {
    await addRecentPath(makeEntry('/a', 'id-a'))
    await addRecentPath(makeEntry('/b', 'id-b'))
    await removeRecentPath('id-a')
    expect(getRecentPathsList().map((e) => e.path)).toEqual(['/b'])
  })

  it('a failed add leaves the mirror untouched', async () => {
    await addRecentPath(makeEntry('/a'))
    addRecentPathMock.mockResolvedValueOnce({ status: 'error', error: 'disk full' })
    await addRecentPath(makeEntry('/b'))
    expect(getRecentPathsList().map((e) => e.path)).toEqual(['/a'])
  })
})
