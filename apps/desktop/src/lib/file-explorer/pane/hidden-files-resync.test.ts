import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createHiddenFilesResync } from './hidden-files-resync'
import { createPaneRowState } from './pane-row-state'
import type { ResortResult } from '../types'

const ipc = vi.hoisted(() => ({ setListingIncludeHidden: vi.fn(), getTotalCount: vi.fn() }))
vi.mock('$lib/tauri-commands', () => ipc)

function harness() {
  let listingId = 'listing-1'
  const rows = createPaneRowState({
    getListingId: () => listingId,
    getLoading: () => false,
    getOperationActive: () => false,
  })
  rows.initialize()
  rows.setSequence(7)
  const sync = createHiddenFilesResync(() => listingId)
  const install = vi.fn()
  const getSortState = vi.fn(() => ({
    cursorFilename: 'a.txt',
    backendSelectedIndices: [2],
    allSelected: false,
    hasParent: false,
  }))
  return {
    rows,
    sync,
    install,
    getSortState,
    navigate: () => {
      listingId = 'listing-2'
      rows.reset()
    },
    run: (includeHidden = true) =>
      sync.resync({ listingId, includeHidden, rowState: rows, getSortState, applyResult: install }),
  }
}

const result: ResortResult = { sequence: 8, totalCount: 3, newCursorIndex: 1, newSelectedIndices: [1] }

describe('hidden visibility row transition', () => {
  beforeEach(() => {
    vi.resetAllMocks()
    ipc.setListingIncludeHidden.mockResolvedValue(result)
  })

  it('atomically remaps hidden visibility from the exact old selected rows', async () => {
    const pane = harness()
    const done = pane.run()
    expect(pane.rows.isReady()).toBe(false)
    await done
    expect(ipc.setListingIncludeHidden).toHaveBeenCalledWith('listing-1', true, 7, 'a.txt', [2], false)
    expect(ipc.getTotalCount).not.toHaveBeenCalled()
    expect(pane.install).toHaveBeenCalledExactlyOnceWith(result)
    expect(pane.rows.getSequence()).toBe(8)
  })

  it('installs the response before draining a newer buffered transition', async () => {
    const pane = harness()
    const order: string[] = []
    pane.install.mockImplementation(() => order.push('response'))
    pane.rows.setBatchApplier(() => {
      order.push('diff')
    })
    ipc.setListingIncludeHidden.mockImplementation(() => {
      pane.rows.receive([{ fromSequence: 8, sequence: 9, totalCount: 2, changes: [] }])
      expect(order).toEqual([])
      return Promise.resolve(result)
    })
    await pane.run()
    expect(order).toEqual(['response', 'diff'])
    expect(pane.rows.getSequence()).toBe(9)
  })

  it('recaptures selection after draining a changed refusal, rather than reusing old rows', async () => {
    vi.useFakeTimers()
    const pane = harness()
    pane.rows.setBatchApplier(() => {
      pane.getSortState.mockReturnValue({
        cursorFilename: 'a.txt',
        backendSelectedIndices: [1],
        allSelected: false,
        hasParent: false,
      })
    })
    ipc.setListingIncludeHidden
      .mockImplementationOnce(() => {
        pane.rows.receive([{ fromSequence: 7, sequence: 8, totalCount: 2, changes: [] }])
        return Promise.reject(Object.assign(new Error(), { type: 'changed', listingId: 'listing-1' }))
      })
      .mockResolvedValueOnce({ ...result, sequence: 9 })
    const done = pane.run(false)
    await vi.advanceTimersByTimeAsync(100)
    await done
    expect(ipc.setListingIncludeHidden).toHaveBeenLastCalledWith('listing-1', false, 8, 'a.txt', [1], false)
    vi.useRealTimers()
  })

  it('serializes off/on ABA and advances the local generation for both requests immediately', async () => {
    const pane = harness()
    let finish!: (result: ResortResult) => void
    ipc.setListingIncludeHidden
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            finish = resolve
          }),
      )
      .mockResolvedValueOnce({ ...result, sequence: 9 })
    const generation = pane.rows.getGeneration()
    const first = pane.run(false)
    const second = pane.run(true)
    expect(pane.rows.getGeneration()).toBe(generation + 2)
    await Promise.resolve()
    expect(ipc.setListingIncludeHidden).toHaveBeenCalledTimes(1)
    finish(result)
    await Promise.all([first, second])
    expect(ipc.setListingIncludeHidden).toHaveBeenLastCalledWith('listing-1', true, 8, 'a.txt', [2], false)
  })

  it.each(['navigate', 'dispose'] as const)('does not install after %s', async (action) => {
    const pane = harness()
    ipc.setListingIncludeHidden.mockImplementationOnce(() => {
      if (action === 'navigate') pane.navigate()
      else pane.sync.dispose()
      return Promise.resolve(result)
    })
    await pane.run()
    expect(pane.install).not.toHaveBeenCalled()
  })

  it('suppresses a gone rejection after disposal but not a live fault', async () => {
    const pane = harness()
    ipc.setListingIncludeHidden.mockImplementationOnce(() => {
      pane.sync.dispose()
      return Promise.reject(Object.assign(new Error(), { type: 'gone' }))
    })
    await expect(pane.run()).resolves.toBeUndefined()
    const live = harness()
    ipc.setListingIncludeHidden.mockRejectedValueOnce({ type: 'internal' })
    await expect(live.run()).rejects.toEqual({ type: 'internal' })
  })
})
