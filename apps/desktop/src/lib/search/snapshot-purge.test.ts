/**
 * The search-snapshot purge, driven by what the operation actually did.
 *
 * A stored snapshot can outlive the files it lists, in any window, long after
 * the operation that removed them ended. The purge keeps it honest, and its
 * input is the per-path `write-source-item-done` stream: a top-level item emits
 * once it's fully processed, carrying `sourceRemoved`, so a skipped item and one
 * a cancel never reached both emit nothing at all.
 *
 * Both of the first two cases here are shipped bugs the old shape had, from
 * walking the operation's INTENDED `sourcePaths` on completion only.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { initSnapshotPurge, destroySnapshotPurge } from './snapshot-purge'
import { getOrCreate, getSnapshot, getMutationTick, _resetForTesting } from './snapshot-store.svelte'
import type { WriteSourceItemDoneEvent } from '$lib/ipc/bindings'
import type { SearchResultEntry } from '$lib/ipc/bindings'

const { onWriteSourceItemDone, unlisten } = vi.hoisted(() => ({
  onWriteSourceItemDone: vi.fn(),
  unlisten: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({ onWriteSourceItemDone }))

const FOLDER = '/Users/me/photos'
const A = `${FOLDER}/a.jpg`
const B = `${FOLDER}/b.jpg`

/** The handler the purge registered, so a test can play the backend. */
let emit: (event: WriteSourceItemDoneEvent) => void

function entry(path: string, name: string): SearchResultEntry {
  return { name, path, parentPath: FOLDER, isDirectory: false, size: 1, modifiedAt: null, iconId: 'file' }
}

function seedSnapshot(): void {
  getOrCreate('sr-1', {
    id: 'sr-1',
    query: 'jpg',
    mode: 'filename',
    filters: {},
    scope: FOLDER,
    caseSensitive: false,
    excludeSystemDirs: true,
    entries: [entry(A, 'a.jpg'), entry(B, 'b.jpg')],
    totalCount: 2,
    createdAt: 0,
    label: 'jpg',
  })
}

/** What a source item's completion looks like on the wire. */
function done(sourcePath: string, sourceRemoved: boolean): WriteSourceItemDoneEvent {
  return { operationId: 'op-1', sourcePath, sourceRemoved }
}

const remainingPaths = () => getSnapshot('sr-1')?.entries.map((e) => e.path)

beforeEach(async () => {
  vi.clearAllMocks()
  _resetForTesting()
  seedSnapshot()
  onWriteSourceItemDone.mockImplementation((handler: (event: WriteSourceItemDoneEvent) => void) => {
    emit = handler
    return Promise.resolve(unlisten)
  })
  await initSnapshotPurge()
})

afterEach(() => {
  destroySnapshotPurge()
})

describe('an operation that stopped early', () => {
  it('drops the rows for what it DID move, and keeps the rest', () => {
    // `a.jpg` moved; the user cancelled before `b.jpg`, so no event for it.
    emit(done(A, true))

    expect(remainingPaths()).toEqual([B])
  })
})

describe('an item the operation skipped', () => {
  it('keeps its row: the file is still on disk', () => {
    // A move resolved `b.jpg`'s conflict with Skip, so only `a.jpg` emits.
    emit(done(A, true))

    expect(remainingPaths()).toEqual([B])
  })
})

describe('an operation that removed nothing', () => {
  it('leaves a copy alone', () => {
    emit(done(A, false))
    emit(done(B, false))

    expect(remainingPaths()).toEqual([A, B])
  })

  it("leaves a cross-FS move's staged item alone until its source is deleted", () => {
    // Phase 2 finishes staging `a.jpg` while the original is still there, and a
    // Skip in the rename phase can mean it stays for good.
    emit(done(A, false))
    expect(remainingPaths()).toEqual([A, B])

    // Phase 4 deletes it.
    emit(done(A, true))
    expect(remainingPaths()).toEqual([B])
  })
})

describe('every snapshot, not only the one on screen', () => {
  it('drops the path from a second snapshot that happened to list it', () => {
    getOrCreate('sr-2', {
      id: 'sr-2',
      query: 'a',
      mode: 'filename',
      filters: {},
      scope: FOLDER,
      caseSensitive: false,
      excludeSystemDirs: true,
      entries: [entry(A, 'a.jpg')],
      totalCount: 1,
      createdAt: 0,
      label: 'a',
    })

    emit(done(A, true))

    expect(getSnapshot('sr-2')?.entries).toEqual([])
  })

  it('bumps the mutation tick so an open results pane re-derives', () => {
    const before = getMutationTick()

    emit(done(A, true))

    expect(getMutationTick()).toBeGreaterThan(before)
  })

  it('costs nothing when no snapshot lists the path', () => {
    const before = getMutationTick()

    emit(done('/Users/me/elsewhere/z.jpg', true))

    expect(getMutationTick()).toBe(before)
    expect(remainingPaths()).toEqual([A, B])
  })
})

describe('lifecycle', () => {
  it('subscribes once, however many times it is initialized', async () => {
    await initSnapshotPurge()

    expect(onWriteSourceItemDone).toHaveBeenCalledTimes(1)
  })

  it('drops the listener on teardown', () => {
    destroySnapshotPurge()

    expect(unlisten).toHaveBeenCalledTimes(1)
  })
})
