import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync } from 'svelte'
import { createQuickFilterController } from './quick-filter-controller.svelte'
import { initListingDiffSync } from './listing-diff-sync.svelte'
import { createListingUpdateQueue } from './listing-update-queue'
import { createPaneRowState } from './pane-row-state'
import { createSelectionState } from './selection-state.svelte'
import { createRenameState } from '../rename/rename-state.svelte'
import type { DirectoryDiff } from '../types'

const ipc = vi.hoisted(() => ({
  setListingNameFilter: vi.fn(),
  getTotalCount: vi.fn(),
  onDirectoryDiff: vi.fn(),
}))
vi.mock('#lib/tauri-commands/index.js', () => ({
  ...ipc,
  onDirectoryDeleted: () => Promise.resolve(() => {}),
  onListingRespelled: () => Promise.resolve(() => {}),
  onWriteSourceItemDone: () => Promise.resolve(() => {}),
}))

const settle = () => new Promise((resolve) => setTimeout(resolve, 0))

/** `Promise.withResolvers`, which the app's Safari 15.4 type floor leaves out (`src/webkit-floor.d.ts`). */
function withResolvers<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}
const filtered = {
  accepted: true,
  totalCount: 2,
  hiddenCount: 0,
  newCursorIndex: 1,
  newSelectedIndices: [1],
  sequence: 5,
}
const added: DirectoryDiff = {
  listingId: 'listing',
  batches: [
    {
      fromSequence: 5,
      sequence: 6,
      totalCount: 3,
      hiddenCount: 0,
      effectiveSortBy: null,
      changes: [
        {
          type: 'add',
          index: 0,
          previousIndex: null,
          entry: {
            name: 'a.pdf',
            path: '/a.pdf',
            isDirectory: false,
            isSymlink: false,
            permissions: 0o644,
            owner: 'user',
            group: 'staff',
            iconId: 'file',
          },
        },
      ],
    },
  ],
}

const transition = (fromSequence: number, sequence: number): DirectoryDiff => ({
  ...added,
  batches: [{ ...added.batches[0], fromSequence, sequence }],
})

describe('filter and watcher row-space ordering', () => {
  let dispose: () => void
  let emit: (diff: DirectoryDiff) => void
  beforeEach(() => {
    vi.clearAllMocks()
    ipc.onDirectoryDiff.mockImplementation((callback: typeof emit) => {
      emit = callback
      return Promise.resolve(() => {})
    })
    ipc.getTotalCount.mockResolvedValue(3)
  })
  afterEach(() => {
    dispose()
  })

  function wire() {
    let cursor = 4
    let count = 8
    let hidden = 0
    const rowState = createPaneRowState({
      getListingId: () => 'listing',
      getLoading: () => false,
      getOperationActive: () => false,
      getIncludeHidden: () => false,
    })
    rowState.initialize()
    const runListingUpdate = createListingUpdateQueue()
    const selection = createSelectionState()
    selection.setSelectedIndices([4])
    const ctl = createQuickFilterController({
      rowState,
      runListingUpdate,
      getListingId: () => 'listing',
      getLoading: () => false,
      getHasBackendListing: () => true,
      getIncludeHidden: () => false,
      getHasParent: () => true,
      getCursorFilename: () => 'z.pdf',
      getSelectedIndices: () => selection.getSelectedIndices(),
      apply: (result) => {
        count = result.totalCount
        hidden = result.hiddenCount
        cursor = result.cursorIndex
        selection.setSelectedIndices(result.selectedIndices)
      },
    })
    dispose = $effect.root(() => {
      initListingDiffSync({
        rowState,
        selection,
        rename: createRenameState(),
        renameFlow: { pendingCursorName: null },
        getListingId: () => 'listing',
        getIncludeHidden: () => false,
        getHasParent: () => true,
        getCursorIndex: () => cursor,
        setCursorIndex: (index) => {
          cursor = index
          return Promise.resolve()
        },
        applyCursorIndex: (index) => {
          cursor = index
        },
        getCurrentPath: () => '/',
        getVolumePath: () => '/',
        getOperationSelectedNames: () => null,
        setEffectiveSortBy: () => {},
        setTotalCount: (value) => {
          count = value
        },
        setHiddenCount: (value) => {
          hidden = value
        },
        bumpSoftRefreshTick: vi.fn(),
        scheduleColumnWidthRefetch: vi.fn(),
        fetchEntryUnderCursor: vi.fn(),
        fetchListingStats: vi.fn(),
        navigateToFallback: vi.fn(),
        adoptStoredPath: vi.fn(),
        bumpCacheGeneration: vi.fn(),
      })
    })
    flushSync()
    return {
      ctl,
      ready: rowState.isReady,
      state: () => ({ cursor, count, selection: selection.getSelectedIndices(), sequence: rowState.getSequence() }),
      hidden: () => hidden,
    }
  }

  // A dotfile coming or going in a folder that looks empty moves no row: the backend
  // sends a batch with no changes for the count alone, and the empty state must hear it.
  it('takes the hidden count from a batch that changes no row', async () => {
    const pane = wire()
    emit({
      listingId: 'listing',
      batches: [{ fromSequence: 0, sequence: 1, totalCount: 0, hiddenCount: 3, effectiveSortBy: null, changes: [] }],
    })
    await settle()
    expect(pane.hidden()).toBe(3)
    expect(pane.state().sequence).toBe(1)
  })

  it('takes the hidden count a filter answer carries', async () => {
    ipc.setListingNameFilter.mockResolvedValue({ ...filtered, totalCount: 0, hiddenCount: 2, newSelectedIndices: [] })
    const pane = wire()
    pane.ctl.append('git')
    await settle()
    expect(pane.hidden()).toBe(2)
  })

  it('applies a new-row diff only after the filter response establishes those rows', async () => {
    const answer = withResolvers<typeof filtered>()
    ipc.setListingNameFilter.mockReturnValue(answer.promise)
    const pane = wire()
    pane.ctl.append('p')
    expect(pane.ready()).toBe(false)
    await settle()
    emit(added)
    await settle()
    expect(ipc.getTotalCount).not.toHaveBeenCalled()
    answer.resolve(filtered)
    await settle()
    expect(pane.state()).toEqual({ cursor: 3, count: 3, selection: [3], sequence: 6 })
    expect(pane.ready()).toBe(true)
  })

  it('retries a changed refusal with the reconciled selection and revision', async () => {
    vi.useFakeTimers()
    try {
      ipc.setListingNameFilter.mockRejectedValueOnce({ type: 'changed' }).mockResolvedValue(filtered)
      const pane = wire()
      pane.ctl.append('p')
      emit(transition(0, 4))
      await vi.advanceTimersByTimeAsync(100)
      expect(ipc.setListingNameFilter.mock.calls[1][4]).toEqual([4])
      expect(ipc.setListingNameFilter.mock.calls[1][6]).toBe(4)
      expect(pane.state()).toEqual({ cursor: 2, count: 2, selection: [2], sequence: 5 })
      expect(pane.ready()).toBe(true)
    } finally {
      vi.useRealTimers()
    }
  })

  it('applies old-row batches synchronously before taking the guarded filter selection snapshot', async () => {
    ipc.setListingNameFilter.mockResolvedValue(filtered)
    const pane = wire()
    emit(transition(0, 4))
    await settle()
    pane.ctl.append('p')
    await settle()
    expect(ipc.setListingNameFilter.mock.calls[0][4]).toEqual([4])
    expect(ipc.setListingNameFilter.mock.calls[0][6]).toBe(4)
    expect(pane.state()).toEqual({ cursor: 2, count: 2, selection: [2], sequence: 5 })
  })

  it('discards an old-row diff delivered while the filter response is pending', async () => {
    const answer = withResolvers<typeof filtered>()
    ipc.setListingNameFilter.mockReturnValue(answer.promise)
    const pane = wire()
    pane.ctl.append('p')
    await settle()
    emit(transition(0, 4))
    answer.resolve(filtered)
    await settle()
    expect(ipc.getTotalCount).not.toHaveBeenCalled()
    expect(pane.state()).toEqual({ cursor: 2, count: 2, selection: [2], sequence: 5 })
  })

  it('does not restore the old count when a throttled refresh runs after the filter', async () => {
    vi.useFakeTimers()
    try {
      ipc.setListingNameFilter.mockResolvedValue(filtered)
      const pane = wire()
      emit(transition(0, 3))
      await vi.advanceTimersByTimeAsync(0)
      emit(transition(3, 4))
      await vi.advanceTimersByTimeAsync(0)
      pane.ctl.append('p')
      await vi.advanceTimersByTimeAsync(0)
      await vi.advanceTimersByTimeAsync(250)
      expect(pane.state()).toEqual({ cursor: 2, count: 2, selection: [2], sequence: 5 })
    } finally {
      vi.useRealTimers()
    }
  })

  it('keeps processing watcher updates after a rejected filter IPC', async () => {
    const answer = withResolvers<typeof filtered>()
    ipc.setListingNameFilter.mockReturnValue(answer.promise)
    const pane = wire()
    pane.ctl.append('p')
    emit(transition(0, 6))
    answer.reject({ type: 'ListingGone' })
    await settle()
    expect(ipc.getTotalCount).not.toHaveBeenCalled()
    expect(pane.state()).toEqual({ cursor: 5, count: 3, selection: [5], sequence: 6 })
  })
})
