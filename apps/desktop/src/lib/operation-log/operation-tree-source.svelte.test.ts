/**
 * The Operation log's tree source: what it asks `get_operation_log_tree` for, and which
 * answers it believes. The backend owns the tree, so everything pinned here is about
 * fast input staying honest (generation-tagged answers, a settling cursor), the
 * selection travelling back exactly as normalized, and an old operation's backfill.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type {
  OperationRow,
  OperationTreeRequest,
  OperationTreeView,
  TreeBackfillEvent,
  TreeBackfillStart,
} from '#lib/ipc/bindings.js'
import { fileRow, folderRow, tally, treeView } from './operation-tree-test-fixtures'

const ipc = vi.hoisted(() => ({
  getOperationLogTree: vi.fn<(id: string, request: OperationTreeRequest) => Promise<OperationTreeView | null>>(),
  startOperationLogTreeBackfill: vi.fn<(id: string) => Promise<TreeBackfillStart>>(),
  cancelOperationLogTreeBackfill: vi.fn<(id: string) => Promise<void>>(() => Promise.resolve()),
  backfillListener: null as ((event: TreeBackfillEvent) => void) | null,
  indexesListener: null as (() => void) | null,
}))

vi.mock('#lib/tauri-commands/operation-log.js', () => ({
  getOperationLogTree: ipc.getOperationLogTree,
  startOperationLogTreeBackfill: ipc.startOperationLogTreeBackfill,
  cancelOperationLogTreeBackfill: ipc.cancelOperationLogTreeBackfill,
  onOperationLogTreeBackfill: (handler: (event: TreeBackfillEvent) => void) => {
    ipc.backfillListener = handler
    return Promise.resolve(() => {
      ipc.backfillListener = null
    })
  },
  onOperationLogTreeIndexesReady: (handler: () => void) => {
    ipc.indexesListener = handler
    return Promise.resolve(() => {
      ipc.indexesListener = null
    })
  },
}))

vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import { createOperationTreeSource, SETTLE_MS, type OperationTreeSource } from './operation-tree-source.svelte'

function op(opId: string): OperationRow {
  return {
    opId,
    kind: 'copy',
    archiveSubkind: null,
    initiator: 'user',
    executionStatus: 'done',
    rollbackState: 'rollbackable',
    notRollbackableReason: null,
    rollsBackOpId: null,
    inverseOpId: null,
    sourceVolumeId: 'root',
    destVolumeId: 'root',
    startedAt: 1_700_000_000,
    endedAt: 1_700_000_010,
    itemCount: 2,
    itemsDone: 2,
    bytesTotal: 0,
    searchCoverage: 'full',
    searchCoverageReason: null,
    devSummary: null,
  }
}

/** A promise this test resolves by hand, so answers can land out of order. */
function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve: (value: T) => void = () => {}
  const promise = new Promise<T>((r) => {
    resolve = r
  })
  return { promise, resolve }
}

/** Let queued promise callbacks run (fake timers don't advance microtasks by themselves). */
async function flush(): Promise<void> {
  for (let i = 0; i < 5; i++) await Promise.resolve()
}

let source: OperationTreeSource

beforeEach(() => {
  vi.useFakeTimers()
  ipc.getOperationLogTree.mockReset()
  ipc.startOperationLogTreeBackfill.mockReset()
  ipc.cancelOperationLogTreeBackfill.mockClear()
  // Many top rows, so a fresh tree doesn't open its folders by itself unless a test asks.
  ipc.getOperationLogTree.mockResolvedValue(treeView([fileRow(1), fileRow(2)]))
  source = createOperationTreeSource()
})

afterEach(() => {
  source.dispose()
  vi.useRealTimers()
})

async function settleOn(opId: string): Promise<void> {
  source.show(op(opId))
  vi.advanceTimersByTime(SETTLE_MS)
  await flush()
}

function lastRequest(): OperationTreeRequest {
  const call = ipc.getOperationLogTree.mock.calls.at(-1)
  if (call === undefined) throw new Error('the tree was never asked for')
  return call[1]
}

describe('the cursor settles before the tree loads', () => {
  it('reads one tree for ten fast ↓ presses, the one the cursor rested on', async () => {
    for (let i = 0; i < 10; i++) {
      source.show(op(`op-${String(i)}`))
      vi.advanceTimersByTime(SETTLE_MS / 3)
    }
    expect(ipc.getOperationLogTree).not.toHaveBeenCalled()

    vi.advanceTimersByTime(SETTLE_MS)
    await flush()

    expect(ipc.getOperationLogTree).toHaveBeenCalledTimes(1)
    expect(ipc.getOperationLogTree.mock.calls[0][0]).toBe('op-9')
    expect(source.opId).toBe('op-9')
  })

  it('starts at most one backfill for ten fast ↓ presses over old operations', async () => {
    ipc.getOperationLogTree.mockResolvedValue(
      treeView([], { summary: { kind: 'unbuilt', reason: 'notBuilt' }, totalRows: 0 }),
    )
    ipc.startOperationLogTreeBackfill.mockResolvedValue({ kind: 'started' })
    for (let i = 0; i < 10; i++) {
      source.show(op(`op-${String(i)}`))
      vi.advanceTimersByTime(SETTLE_MS / 3)
    }
    vi.advanceTimersByTime(SETTLE_MS)
    await flush()

    expect(ipc.startOperationLogTreeBackfill).toHaveBeenCalledTimes(1)
    expect(ipc.startOperationLogTreeBackfill).toHaveBeenCalledWith('op-9')
  })

  it('asks for everything by default: no selection, so a plain press behaves as before', async () => {
    await settleOn('op-1')
    expect(lastRequest().selection).toBeNull()
    expect(source.narrowed).toBe(false)
    expect(source.allCheckbox).toBe('on')
    expect(source.rows.count).toBe(2)
  })
})

describe('fast toggles stay honest', () => {
  it('keeps both of two toggles made before the first answer lands', async () => {
    await settleOn('op-1')
    const first = deferred<OperationTreeView | null>()
    // The second answer stays out, so what's checked below is the first answer's effect alone.
    ipc.getOperationLogTree.mockReturnValueOnce(first.promise).mockReturnValueOnce(new Promise(() => {}))
    const rowA = fileRow(1)
    const rowB = fileRow(2)

    source.toggleChecked(rowA)
    source.toggleChecked(rowB)
    // Both show their new state at once, before any answer.
    expect(source.checkboxOf(rowA)).toBe('off')
    expect(source.checkboxOf(rowB)).toBe('off')

    // The first answer knows only the first toggle: adopting it would wipe the second.
    first.resolve(
      treeView([fileRow(1, { checkbox: 'off' }), fileRow(2)], {
        selection: { base: 'everything', toggles: [{ kind: 'item', seq: 1, on: false }] },
      }),
    )
    await flush()
    expect(source.checkboxOf(rowB)).toBe('off')

    // The queued request carries both, in the order they were made.
    expect(lastRequest().selection).toEqual({
      base: 'everything',
      toggles: [
        { kind: 'item', seq: 1, on: false },
        { kind: 'item', seq: 2, on: false },
      ],
    })
  })

  it('sends back exactly the selection the backend normalized, never its own guess', async () => {
    await settleOn('op-1')
    const normalized = { base: 'everything' as const, toggles: [{ kind: 'folder' as const, dirId: 4, on: false }] }
    ipc.getOperationLogTree.mockResolvedValueOnce(treeView([fileRow(1)], { selection: normalized }))
    source.toggleChecked(folderRow(4))
    await flush()
    expect(source.selection).toEqual(normalized)

    source.toggleChecked(fileRow(1))
    await flush()
    expect(lastRequest().selection).toEqual({
      base: 'everything',
      toggles: [...normalized.toggles, { kind: 'item', seq: 1, on: false }],
    })
  })

  it('turns a mixed folder fully on, and an on one off', async () => {
    await settleOn('op-1')
    source.toggleChecked(folderRow(4, { checkbox: 'mixed' }))
    expect(lastRequest().selection?.toggles.at(-1)).toEqual({ kind: 'folder', dirId: 4, on: true })
  })

  it('refuses a pick past the toggle cap, keeps the previous selection, and says so', async () => {
    await settleOn('op-1')
    const previous = { base: 'nothing' as const, toggles: [{ kind: 'item' as const, seq: 1, on: true }] }
    ipc.getOperationLogTree.mockResolvedValueOnce(treeView([fileRow(1)], { selection: previous, togglesCapped: true }))
    source.toggleChecked(fileRow(2))
    await flush()
    expect(source.togglesCapped).toBe(true)
    expect(source.selection).toEqual(previous)
  })

  it('flips the whole operation from the header box, dropping every hand toggle', async () => {
    await settleOn('op-1')
    source.toggleAll()
    expect(source.allCheckbox).toBe('off')
    expect(lastRequest().selection).toEqual({ base: 'nothing', toggles: [] })
  })

  it('treats a selection normalized back to everything as untouched', async () => {
    await settleOn('op-1')
    ipc.getOperationLogTree.mockResolvedValueOnce(treeView([fileRow(1)]))
    source.toggleChecked(fileRow(1))
    await flush()
    expect(source.narrowed).toBe(false)
  })

  it('offers no pick on a read-only operation (one that can’t, or is busy, rolling back)', async () => {
    ipc.getOperationLogTree.mockResolvedValue(treeView([fileRow(1, { checkbox: 'none' })], { readOnly: true }))
    await settleOn('op-1')
    const calls = ipc.getOperationLogTree.mock.calls.length
    source.toggleChecked(fileRow(1))
    source.toggleAll()
    expect(ipc.getOperationLogTree).toHaveBeenCalledTimes(calls)
    expect(source.allCheckbox).toBe('none')
  })

  it('never lets a row inside a folder unit pick for itself: it comes back with its folder', async () => {
    await settleOn('op-1')
    const calls = ipc.getOperationLogTree.mock.calls.length
    source.toggleChecked(fileRow(3, { role: 'contents' }))
    expect(ipc.getOperationLogTree).toHaveBeenCalledTimes(calls)
  })
})

describe('the cursor stays on its row', () => {
  it('follows its row when a folder above it opens', async () => {
    // More rows than the window holds, so the tree doesn't open its folders by itself.
    ipc.getOperationLogTree.mockResolvedValue(treeView([folderRow(1), fileRow(5), fileRow(6)], { totalRows: 50 }))
    await settleOn('op-1')
    source.setCursor(2)
    ipc.getOperationLogTree.mockResolvedValueOnce(
      treeView([folderRow(1, { expanded: true }), fileRow(10, { level: 2 }), fileRow(5), fileRow(6)], {
        cursorIndex: 3,
      }),
    )
    source.toggleExpanded(folderRow(1))
    await flush()

    expect(lastRequest().cursor).toEqual({ kind: 'item', seq: 6 })
    expect(lastRequest().expanded).toEqual([{ kind: 'folder', dirId: 1 }])
    expect(source.cursorIndex).toBe(3)
  })

  it('opens a small fresh tree’s top folders by itself, as the mockup reads', async () => {
    ipc.getOperationLogTree.mockResolvedValue(treeView([folderRow(1), folderRow(2)]))
    await settleOn('op-1')
    expect(lastRequest().expanded).toEqual([
      { kind: 'folder', dirId: 1 },
      { kind: 'folder', dirId: 2 },
    ])
  })

  it('finds a row’s parent in the window, or above it in the answer’s ancestors', async () => {
    ipc.getOperationLogTree.mockResolvedValue(
      treeView([fileRow(7, { level: 2 }), folderRow(3, { level: 1, expanded: true }), fileRow(8, { level: 2 })], {
        offset: 40,
        totalRows: 60,
        ancestors: [{ ...folderRow(2, { expanded: true }), index: 12 }],
      }),
    )
    await settleOn('op-1')
    expect(source.parentIndexOf(42)).toBe(41)
    expect(source.parentIndexOf(40)).toBe(12)
    expect(source.parentIndexOf(41)).toBeNull()
  })
})

describe('an old operation backfills before its tree opens', () => {
  it('starts the backfill, shows its progress, and reads the tree once it’s built', async () => {
    ipc.getOperationLogTree.mockResolvedValueOnce(
      treeView([], { summary: { kind: 'unbuilt', reason: 'notBuilt' }, totalRows: 0 }),
    )
    ipc.startOperationLogTreeBackfill.mockResolvedValue({ kind: 'started' })
    await settleOn('op-old')
    expect(ipc.startOperationLogTreeBackfill).toHaveBeenCalledWith('op-old')
    expect(source.blank?.kind).toBe('backfilling')

    ipc.backfillListener?.({ opId: 'op-old', step: { kind: 'progress', detail: { done: 4_096, total: 20_000 } } })
    expect(source.blank).toEqual({ kind: 'backfilling', done: 4_096, total: 20_000 })
    // Another operation's backfill isn't this tree's news.
    ipc.backfillListener?.({ opId: 'op-other', step: { kind: 'progress', detail: { done: 1, total: 2 } } })
    expect(source.blank).toEqual({ kind: 'backfilling', done: 4_096, total: 20_000 })

    ipc.backfillListener?.({ opId: 'op-old', step: { kind: 'finished', detail: { kind: 'built' } } })
    await flush()
    expect(source.blank).toBeNull()
    expect(source.rows.count).toBe(2)
  })

  it('earns a progress bar only past ten seconds', async () => {
    ipc.getOperationLogTree.mockResolvedValueOnce(
      treeView([], { summary: { kind: 'unbuilt', reason: 'notBuilt' }, totalRows: 0 }),
    )
    ipc.startOperationLogTreeBackfill.mockResolvedValue({ kind: 'started' })
    await settleOn('op-old')
    vi.advanceTimersByTime(9_000)
    expect(source.backfillSlow).toBe(false)
    vi.advanceTimersByTime(1_000)
    expect(source.backfillSlow).toBe(true)
  })

  it('cancels the backfill when the cursor moves on, and when the dialog closes', async () => {
    ipc.getOperationLogTree.mockResolvedValueOnce(
      treeView([], { summary: { kind: 'unbuilt', reason: 'notBuilt' }, totalRows: 0 }),
    )
    ipc.startOperationLogTreeBackfill.mockResolvedValue({ kind: 'started' })
    await settleOn('op-old')
    await settleOn('op-new')
    expect(ipc.cancelOperationLogTreeBackfill).toHaveBeenCalledWith('op-old')

    ipc.getOperationLogTree.mockResolvedValueOnce(
      treeView([], { summary: { kind: 'unbuilt', reason: 'notBuilt' }, totalRows: 0 }),
    )
    await settleOn('op-older')
    source.dispose()
    expect(ipc.cancelOperationLogTreeBackfill).toHaveBeenCalledWith('op-older')
  })

  it('says why a running operation has no tree yet, and starts nothing', async () => {
    ipc.getOperationLogTree.mockResolvedValueOnce(
      treeView([], { summary: { kind: 'unbuilt', reason: 'operationRunning' }, totalRows: 0 }),
    )
    await settleOn('op-busy')
    expect(source.blank).toEqual({ kind: 'unbuilt', reason: 'operationRunning' })
    expect(ipc.startOperationLogTreeBackfill).not.toHaveBeenCalled()
  })

  it('waits for the backend to say the journal built its indexes, then reads again, with no timer', async () => {
    ipc.getOperationLogTree.mockResolvedValueOnce(treeView([], { summary: { kind: 'indexesBuilding' }, totalRows: 0 }))
    await settleOn('op-1')
    expect(source.blank?.kind).toBe('indexesBuilding')

    vi.advanceTimersByTime(60_000)
    await flush()
    expect(ipc.getOperationLogTree).toHaveBeenCalledTimes(1)
    expect(source.blank?.kind).toBe('indexesBuilding')

    ipc.indexesListener?.()
    await flush()
    expect(ipc.getOperationLogTree).toHaveBeenCalledTimes(2)
    expect(source.blank).toBeNull()
  })

  it('reads nothing extra on the indexes event when its tree already shows', async () => {
    await settleOn('op-1')
    expect(ipc.getOperationLogTree).toHaveBeenCalledTimes(1)
    ipc.indexesListener?.()
    await flush()
    expect(ipc.getOperationLogTree).toHaveBeenCalledTimes(1)
  })

  it('says the operation is gone when retention took it', async () => {
    ipc.getOperationLogTree.mockResolvedValueOnce(null)
    await settleOn('op-pruned')
    expect(source.blank).toEqual({ kind: 'gone' })
  })
})

describe('a reversal that ended', () => {
  it('re-reads the tree and drops the picks, which described a state that’s gone', async () => {
    await settleOn('op-1')
    ipc.getOperationLogTree.mockResolvedValueOnce(
      treeView([fileRow(1)], { selection: { base: 'nothing', toggles: [] }, tally: tally({ selectable: 1 }) }),
    )
    source.toggleAll()
    await flush()
    expect(source.narrowed).toBe(true)

    source.refresh({ resetSelection: true })
    expect(lastRequest().selection).toBeNull()
  })
})
