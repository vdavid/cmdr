/**
 * Tests for `compare-directories.ts` (⇧F2). They pin:
 * - both panes' listings and the hidden-files setting go to the backend,
 * - each pane's selection becomes the backend's rows plus its own `..` offset,
 * - the toasts: what was selected, no differences, a pane with no folder,
 * - an answer for panes that moved on meanwhile is dropped,
 * - an answer read from another state than the panes show is asked again, and
 *   given up on (with a toast) when the folders keep changing.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

const { ipc, addToast } = vi.hoisted(() => ({
  ipc: { compareDirectories: vi.fn() },
  addToast: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({ compareDirectories: ipc.compareDirectories }))
vi.mock('$lib/ui/toast', () => ({ addToast }))
vi.mock('$lib/intl/messages.svelte', () => ({
  tString: (key: string, args?: Record<string, unknown>) => (args ? `${key} ${JSON.stringify(args)}` : key),
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { COMPARE_ATTEMPTS, COMPARE_RETRY_DELAY_MS, compareDirectories } from './compare-directories'
import type { FilePaneAPI } from './types'

function paneRef(listingId: string, hasParent: boolean) {
  const ref = {
    listingId,
    getListingId: vi.fn(() => ref.listingId),
    hasParentEntry: vi.fn(() => hasParent),
    sequence: 0,
    getLastSequence: vi.fn(() => ref.sequence),
    generation: 0,
    ready: true,
    getViewGeneration: vi.fn(() => ref.generation),
    isRowStateReady: vi.fn(() => ref.ready),
    setSelectedIndices: vi.fn(),
  }
  return ref
}

function deps(left: ReturnType<typeof paneRef> | undefined, right: ReturnType<typeof paneRef> | undefined) {
  return {
    getPaneRef: (pane: 'left' | 'right') => (pane === 'left' ? left : right) as unknown as FilePaneAPI | undefined,
    getShowHiddenFiles: () => true,
  }
}

/** A settled answer read at sequence 0 on both sides, as the panes show. */
function answer(left: number[], right: number[], extra: Record<string, unknown> = {}) {
  return { left, right, leftSequence: 0, rightSequence: 0, settled: true, ...extra }
}

describe('compareDirectories', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('drops a delayed answer after hidden off/on or sort reconfiguration', async () => {
    const left = paneRef('L', false)
    const right = paneRef('R', false)
    ipc.compareDirectories.mockImplementation(() => {
      left.generation += 2
      return Promise.resolve(answer([0], []))
    })
    await compareDirectories(deps(left, right), 'missing')
    expect(left.setSelectedIndices).not.toHaveBeenCalled()
  })

  it('does not apply to a replaced pane even when its listing id matches', async () => {
    const left = paneRef('L', false)
    const right = paneRef('R', false)
    let current = left
    ipc.compareDirectories.mockImplementation(() => {
      current = paneRef('L', false)
      return Promise.resolve(answer([0], []))
    })
    await compareDirectories(
      {
        getPaneRef: (side) => (side === 'left' ? current : right) as unknown as FilePaneAPI,
        getShowHiddenFiles: () => true,
      },
      'missing',
    )
    expect(left.setSelectedIndices).not.toHaveBeenCalled()
    expect(current.setSelectedIndices).not.toHaveBeenCalled()
  })

  it('lets the latest comparison mode win when requests overlap', async () => {
    const left = paneRef('L', false)
    const right = paneRef('R', false)
    let finish!: (value: ReturnType<typeof answer>) => void
    ipc.compareDirectories
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            finish = resolve
          }),
      )
      .mockResolvedValueOnce(answer([2], []))
    const live = deps(left, right)
    const older = compareDirectories(live, 'missing')
    await compareDirectories(live, 'sizeAndMissing')
    finish(answer([0], []))
    await older
    expect(left.setSelectedIndices).toHaveBeenCalledExactlyOnceWith([2])
  })

  it('selects the backend rows in each pane, with each pane’s own parent offset', async () => {
    ipc.compareDirectories.mockResolvedValue(answer([0, 2], [1]))
    const left = paneRef('L', true)
    const right = paneRef('R', false)

    await compareDirectories(deps(left, right), 'newerAndMissing')

    expect(ipc.compareDirectories).toHaveBeenCalledWith('L', true, 'R', true, 'newerAndMissing')
    expect(left.setSelectedIndices).toHaveBeenCalledWith([1, 3])
    expect(right.setSelectedIndices).toHaveBeenCalledWith([1])
    expect(addToast.mock.calls[0][0]).toContain('fileExplorer.compareDirectories.marked')
    expect(addToast.mock.calls[0][0]).toContain('"leftCount":2')
  })

  it('clears both selections and says so when nothing differs', async () => {
    ipc.compareDirectories.mockResolvedValue(answer([], []))
    const left = paneRef('L', true)
    const right = paneRef('R', true)

    await compareDirectories(deps(left, right), 'missing')

    expect(left.setSelectedIndices).toHaveBeenCalledWith([])
    expect(right.setSelectedIndices).toHaveBeenCalledWith([])
    expect(addToast).toHaveBeenCalledWith('fileExplorer.compareDirectories.noDifferences.missing', { level: 'info' })
  })

  it('asks for two folders when a pane shows no listing', async () => {
    await compareDirectories(deps(paneRef('L', false), paneRef('', false)), 'newerAndMissing')

    expect(ipc.compareDirectories).not.toHaveBeenCalled()
    expect(addToast).toHaveBeenCalledWith('fileExplorer.compareDirectories.needsTwoFolders', { level: 'info' })
  })

  it('drops the answer when a pane moved to another folder meanwhile', async () => {
    const left = paneRef('L', false)
    const right = paneRef('R', false)
    ipc.compareDirectories.mockImplementation(() => {
      left.listingId = 'L2'
      return Promise.resolve(answer([0], [0]))
    })

    await compareDirectories(deps(left, right), 'newerAndMissing')

    expect(left.setSelectedIndices).not.toHaveBeenCalled()
    expect(right.setSelectedIndices).not.toHaveBeenCalled()
  })

  it('says so when the comparison ran out of time', async () => {
    ipc.compareDirectories.mockRejectedValue({ type: 'timedOut' })

    await compareDirectories(deps(paneRef('L', false), paneRef('R', false)), 'newerAndMissing')

    expect(addToast).toHaveBeenCalledWith('fileExplorer.compareDirectories.couldNotFinish', { level: 'warn' })
  })

  it('drops the answer when hidden files were toggled meanwhile', async () => {
    let showHidden = true
    const left = paneRef('L', false)
    const right = paneRef('R', false)
    ipc.compareDirectories.mockImplementation(() => {
      showHidden = false
      return Promise.resolve(answer([0], []))
    })

    await compareDirectories(
      {
        getPaneRef: (pane) => (pane === 'left' ? left : right) as unknown as FilePaneAPI,
        getShowHiddenFiles: () => showHidden,
      },
      'newerAndMissing',
    )

    expect(left.setSelectedIndices).not.toHaveBeenCalled()
  })

  it('stays quiet and selects nothing when a listing is gone', async () => {
    ipc.compareDirectories.mockRejectedValue({ type: 'gone' })
    const left = paneRef('L', false)

    await compareDirectories(deps(left, paneRef('R', false)), 'newerAndMissing')

    expect(left.setSelectedIndices).not.toHaveBeenCalled()
    expect(addToast).not.toHaveBeenCalled()
  })

  it('asks again when the answer was read from a state the panes don’t show yet', async () => {
    vi.useFakeTimers()
    const left = paneRef('L', false)
    const right = paneRef('R', false)
    // A file appeared on the left: the backend read sequence 1, the pane still shows 0.
    ipc.compareDirectories
      .mockResolvedValueOnce(answer([0], [], { leftSequence: 1 }))
      .mockResolvedValueOnce(answer([1], [], { leftSequence: 1 }))

    const done = compareDirectories(deps(left, right), 'newerAndMissing')
    await vi.advanceTimersByTimeAsync(0)
    expect(left.setSelectedIndices).not.toHaveBeenCalled()
    left.sequence = 1 // the diff landed
    await vi.advanceTimersByTimeAsync(COMPARE_RETRY_DELAY_MS)
    await done

    expect(ipc.compareDirectories).toHaveBeenCalledTimes(2)
    expect(left.setSelectedIndices).toHaveBeenCalledExactlyOnceWith([1])
    vi.useRealTimers()
  })

  it('never applies an unsettled answer, and gives up when the folders keep changing', async () => {
    vi.useFakeTimers()
    const left = paneRef('L', false)
    ipc.compareDirectories.mockResolvedValue(answer([0], [], { settled: false }))

    const done = compareDirectories(deps(left, paneRef('R', false)), 'newerAndMissing')
    await vi.advanceTimersByTimeAsync(COMPARE_RETRY_DELAY_MS * COMPARE_ATTEMPTS)
    await done

    expect(ipc.compareDirectories).toHaveBeenCalledTimes(COMPARE_ATTEMPTS)
    expect(left.setSelectedIndices).not.toHaveBeenCalled()
    expect(addToast).toHaveBeenCalledWith('fileExplorer.compareDirectories.keptChanging', { level: 'warn' })
    vi.useRealTimers()
  })
})
