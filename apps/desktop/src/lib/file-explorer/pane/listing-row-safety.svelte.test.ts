import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushSync } from 'svelte'
import { initListingDiffSync, type ListingDiffSyncDeps } from './listing-diff-sync.svelte'
import { createPaneRowState } from './pane-row-state'
import type { DirectoryDiff } from '../types'

const ipc = vi.hoisted(() => ({
  diff: null as ((diff: DirectoryDiff) => void) | null,
  getTotalCount: vi.fn(() => new Promise<number>(() => {})),
  findFileIndex: vi.fn(),
  findFileIndices: vi.fn(),
  getSelectionSnapshot: vi.fn(() => Promise.resolve({ paths: [], fileCount: 0, folderCount: 0 })),
}))
vi.mock('$lib/tauri-commands', () => ({
  onDirectoryDiff: (callback: (diff: DirectoryDiff) => void) => {
    ipc.diff = callback
    return Promise.resolve(() => {})
  },
  onDirectoryDeleted: () => Promise.resolve(() => {}),
  onListingRespelled: () => Promise.resolve(() => {}),
  onWriteSourceItemDone: () => Promise.resolve(() => {}),
  getTotalCount: ipc.getTotalCount,
  findFileIndex: ipc.findFileIndex,
  findFileIndices: ipc.findFileIndices,
  getSelectionSnapshot: ipc.getSelectionSnapshot,
}))
vi.mock('$lib/logging/logger', () => ({ getAppLogger: () => ({ info: vi.fn() }) }))

let dispose = () => {}
afterEach(() => {
  dispose()
  vi.clearAllMocks()
})

function harness(input: { initialized?: boolean; pendingRename?: string; operationNames?: string[] } = {}) {
  let cursor = 2
  let selected = [2]
  let sequence = 0
  const rows = createPaneRowState({ getListingId: () => 'L', getLoading: () => false, getOperationActive: () => false })
  if (input.initialized !== false) rows.initialize()
  const renameFlow = { pendingCursorName: input.pendingRename ?? null }
  const deps = {
    rowState: rows,
    selection: {
      getSelectedIndices: () => selected,
      setSelectedIndices: (indices: number[]) => {
        selected = indices
      },
    },
    rename: { active: false },
    renameFlow,
    getListingId: () => 'L',
    getIncludeHidden: () => true,
    getHasParent: () => false,
    getCursorIndex: () => cursor,
    applyCursorIndex: (index: number) => {
      cursor = index
    },
    setCursorIndex: (index: number) => {
      cursor = index
      return Promise.resolve()
    },
    getOperationSelectedNames: () => input.operationNames ?? null,
    getLastSequence: () => sequence,
    setLastSequence: (value: number) => {
      sequence = value
    },
    getDiffGeneration: () => 0,
    bumpDiffGeneration: () => 0,
    setTotalCount: vi.fn(),
    bumpSoftRefreshTick: vi.fn(),
    scheduleColumnWidthRefetch: vi.fn(),
    fetchEntryUnderCursor: vi.fn(),
    fetchListingStats: vi.fn(),
  } as unknown as ListingDiffSyncDeps
  dispose = $effect.root(() => {
    initListingDiffSync(deps)
  })
  flushSync()
  return { rows, renameFlow, selected: () => selected, cursor: () => cursor }
}

function removal(fromSequence: number, sequence: number, totalCount: number): DirectoryDiff {
  const changes = [{ type: 'remove' as const, index: 0, entry: { name: 'a', path: '/a' } }]
  // Legacy top-level fields let the regression exercise the old asynchronous handler too.
  return {
    listingId: 'L',
    sequence,
    changes,
    batches: [{ fromSequence, sequence, totalCount, changes }],
  } as unknown as DirectoryDiff
}

function emit(diff: DirectoryDiff): void {
  if (!ipc.diff) throw new Error('Diff listener was not registered')
  ipc.diff(diff)
}

describe('applied listing rows', () => {
  it('remaps rows synchronously without waiting for a live count', () => {
    const pane = harness()
    emit(removal(0, 1, 2))
    expect(pane.selected()).toEqual([1])
    expect(pane.cursor()).toBe(1)
    expect(ipc.getTotalCount).not.toHaveBeenCalled()
  })

  it('applies successive removals in separate row spaces, preserving the surviving identity', () => {
    const pane = harness()
    const first = removal(0, 1, 2)
    const second = removal(1, 2, 1)
    emit({ listingId: 'L', batches: [...first.batches, ...second.batches] })
    expect(pane.selected()).toEqual([0])
    expect(pane.rows.getSequence()).toBe(2)
  })

  it('buffers a successor until its predecessor arrives, never calling a gap applied', () => {
    const pane = harness()
    emit(removal(1, 2, 1))
    expect(pane.rows.getSequence()).toBe(0)
    expect(pane.rows.isReady()).toBe(false)
    emit(removal(0, 1, 2))
    expect(pane.selected()).toEqual([0])
    expect(pane.rows.getSequence()).toBe(2)
    emit(removal(0, 1, 2))
    expect(pane.selected()).toEqual([0])
  })

  it('does not select the neighbor when a later batch removes the selected row', () => {
    const pane = harness()
    const second = removal(1, 2, 1)
    second.batches[0].changes[0].index = 1
    emit({ listingId: 'L', batches: [...removal(0, 1, 2).batches, ...second.batches] })
    expect(pane.selected()).toEqual([])
    expect(pane.rows.getSequence()).toBe(2)
  })

  it('refreshes once for a burst, after every logical row transition has been applied', () => {
    const pane = harness()
    const observed: number[] = []
    pane.rows.setBatchApplier(() => () => {
      observed.push(pane.rows.getSequence())
    })
    emit({ listingId: 'L', batches: [...removal(0, 1, 2).batches, ...removal(1, 2, 1).batches] })
    expect(observed).toEqual([2])
  })

  it('keeps baseline zero while loading and drains initial diffs only after initialization', () => {
    const pane = harness({ initialized: false })
    emit(removal(0, 1, 2))
    expect(pane.rows.isReady()).toBe(false)
    expect(pane.rows.getSequence()).toBe(0)
    expect(pane.selected()).toEqual([2])
    pane.rows.initialize()
    expect(pane.selected()).toEqual([1])
    expect(pane.rows.getSequence()).toBe(1)
  })

  it('remaps selection while pending rename lookup waits and discards its cursor after sort', async () => {
    let finish!: (index: number) => void
    ipc.findFileIndex.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve
        }),
    )
    const pane = harness({ pendingRename: 'renamed' })
    emit(removal(0, 1, 2))
    expect(pane.selected()).toEqual([1])
    expect(pane.rows.isReady()).toBe(false)
    await Promise.resolve()
    await pane.rows.changeView({
      request: () => Promise.resolve({ sequence: 2, totalCount: 2, newCursorIndex: 0, newSelectedIndices: [0] }),
      install: () => {},
    })
    finish(0)
    await vi.waitFor(() => {
      expect(pane.rows.isReady()).toBe(true)
    })
    expect(pane.cursor()).toBe(1)
  })

  it('drops a delayed operation name result after selection ownership changes', async () => {
    let finish!: (indices: Record<string, number>) => void
    ipc.findFileIndices.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve
        }),
    )
    const pane = harness({ operationNames: ['survivor'] })
    emit(removal(0, 1, 2))
    await Promise.resolve()
    pane.rows.invalidateWork()
    finish({ survivor: 0 })
    await vi.waitFor(() => {
      expect(pane.rows.isReady()).toBe(true)
    })
    expect(pane.selected()).toEqual([1])
  })
})
