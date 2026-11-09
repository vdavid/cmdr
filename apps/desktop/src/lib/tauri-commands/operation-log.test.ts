/**
 * Unit tests for the operation-log IPC wrappers: they forward to the typed
 * `commands.*` bindings and unwrap the `Result<T, string>` shape (ok → data,
 * error → throw). The dialog relies on the throw so a read failure surfaces as a
 * caught error, not a silent empty result.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

/**
 * The tauri-specta `Result<T, E>` wire shape the bindings return. `E` defaults to
 * `string`, which is what most commands carry; `rollbackOperation`'s is a typed
 * refusal object.
 */
type Res<T, E = string> = { status: 'ok'; data: T } | { status: 'error'; error: E }

const getRecentMock = vi.fn<(payload: { limit: number; offset: number }) => Promise<Res<unknown>>>()
const getDetailMock =
  vi.fn<(payload: { operationId: string; itemLimit: number; itemOffset: number }) => Promise<Res<unknown>>>()
const undoMock = vi.fn<(ids: string[]) => Promise<Res<unknown>>>()
const rollbackMock =
  vi.fn<(args: { id: string; initiator: string; selection: unknown }) => Promise<Res<unknown, { kind: string }>>>()
const treeMock = vi.fn<(args: { id: string; request: unknown }) => Promise<Res<unknown>>>()
const startBackfillMock = vi.fn<(id: string) => Promise<Res<unknown>>>()
const cancelBackfillMock = vi.fn<(id: string) => Promise<void>>()
type Listener = (event: { payload: unknown }) => void
const backfillListen = vi.fn<(handler: Listener) => Promise<() => void>>()
const indexesSettledListen = vi.fn<(handler: Listener) => Promise<() => void>>()
vi.mock('#lib/ipc/bindings.js', () => ({
  commands: {
    getRecentOperationLogEntries: (limit: number, offset: number) => getRecentMock({ limit, offset }),
    getOperationLogDetail: (id: string, l: number, o: number) =>
      getDetailMock({ operationId: id, itemLimit: l, itemOffset: o }),
    undoOperations: (ids: string[]) => undoMock(ids),
    rollbackOperation: (id: string, initiator: string, selection: unknown) =>
      rollbackMock({ id, initiator, selection }),
    getOperationLogTree: (id: string, request: unknown) => treeMock({ id, request }),
    startOperationLogTreeBackfill: (id: string) => startBackfillMock(id),
    cancelOperationLogTreeBackfill: (id: string) => cancelBackfillMock(id),
  },
  events: {
    operationLogTreeBackfill: { listen: (handler: Listener) => backfillListen(handler) },
    operationLogTreeIndexesSettled: { listen: (handler: Listener) => indexesSettledListen(handler) },
  },
}))

import {
  getRecentOperationLogEntries,
  getOperationLogDetail,
  undoOperations,
  rollbackOperation,
  getOperationLogTree,
  startOperationLogTreeBackfill,
  cancelOperationLogTreeBackfill,
  onOperationLogTreeBackfill,
  onOperationLogTreeIndexesSettled,
  type OperationTreeRequest,
} from './operation-log'
import { asRollbackRefusal } from '#lib/operation-log/rollback-refusal.js'

describe('getRecentOperationLogEntries', () => {
  beforeEach(() => getRecentMock.mockReset())

  it('forwards limit/offset and returns the data on ok', async () => {
    getRecentMock.mockResolvedValue({ status: 'ok', data: [{ opId: 'a' }] })
    const rows = await getRecentOperationLogEntries(50, 100)
    expect(getRecentMock).toHaveBeenCalledWith({ limit: 50, offset: 100 })
    expect(rows).toEqual([{ opId: 'a' }])
  })

  it('throws on an error result', async () => {
    getRecentMock.mockResolvedValue({ status: 'error', error: 'db locked' })
    await expect(getRecentOperationLogEntries(50, 0)).rejects.toThrow('db locked')
  })
})

describe('getOperationLogDetail', () => {
  beforeEach(() => getDetailMock.mockReset())

  it('returns the detail on ok (and null when the op is absent)', async () => {
    getDetailMock.mockResolvedValue({ status: 'ok', data: null })
    expect(await getOperationLogDetail('op-1', 200, 0)).toBeNull()
    expect(getDetailMock).toHaveBeenCalledWith({ operationId: 'op-1', itemLimit: 200, itemOffset: 0 })

    const detail = { operation: { opId: 'op-1' }, items: [], totalItems: 0 }
    getDetailMock.mockResolvedValue({ status: 'ok', data: detail })
    expect(await getOperationLogDetail('op-1', 200, 0)).toEqual(detail)
  })

  it('throws on an error result', async () => {
    getDetailMock.mockResolvedValue({ status: 'error', error: 'gone' })
    await expect(getOperationLogDetail('op-1', 200, 0)).rejects.toThrow('gone')
  })
})

describe('undoOperations', () => {
  beforeEach(() => undoMock.mockReset())

  it('passes the ids through UNCHANGED and returns the tally', async () => {
    const report = {
      operations: [
        { operationId: 'op-3', restored: 8, skipped: 0, finalState: 'rolledBack', refusal: null },
        { operationId: 'op-1', restored: 10, skipped: 2, finalState: 'partiallyRolledBack', refusal: null },
      ],
      restored: 18,
      skipped: 2,
    }
    undoMock.mockResolvedValue({ status: 'ok', data: report })

    expect(await undoOperations(['op-1', 'op-3'])).toEqual(report)
    // APPLY order, untouched: the backend reverses newest-first, and reordering here
    // would silently undo oldest-first (see `rollback/order.rs`).
    expect(undoMock).toHaveBeenCalledWith(['op-1', 'op-3'])
  })

  it('throws on an error result rather than reporting an undo of nothing', async () => {
    undoMock.mockResolvedValue({ status: 'error', error: 'journal is locked' })
    await expect(undoOperations(['op-1'])).rejects.toThrow('journal is locked')
  })
})

describe('rollbackOperation', () => {
  beforeEach(() => rollbackMock.mockReset())

  it("returns the inverse operation's id once the reversal is queued", async () => {
    rollbackMock.mockResolvedValue({ status: 'ok', data: { inverseOpId: 'op-2' } })
    expect(await rollbackOperation('op-1', 'agent')).toEqual({ inverseOpId: 'op-2' })
    // No selection means the whole operation, which is what every caller but the tree wants.
    expect(rollbackMock).toHaveBeenCalledWith({ id: 'op-1', initiator: 'agent', selection: null })
  })

  it("passes the tree's selection through untouched", async () => {
    rollbackMock.mockResolvedValue({ status: 'ok', data: { inverseOpId: 'op-2' } })
    const selection = { base: 'everything' as const, toggles: [{ kind: 'item' as const, seq: 3, on: false }] }
    await rollbackOperation('op-1', 'user', selection)
    expect(rollbackMock).toHaveBeenCalledWith({ id: 'op-1', initiator: 'user', selection })
  })

  it('throws the refusal TYPED, so the caller can word each reason itself', async () => {
    rollbackMock.mockResolvedValue({ status: 'error', error: { kind: 'alreadyRollingBack' } })
    // A flattened string here would put wire JSON in front of the user; the typed
    // value is what lets the history dialog pick the right sentence.
    const thrown = await rollbackOperation('op-1', 'user').catch((e: unknown) => e)
    expect(asRollbackRefusal(thrown)).toEqual({ kind: 'alreadyRollingBack' })
  })
})

describe('getOperationLogTree', () => {
  beforeEach(() => treeMock.mockReset())

  // The wrapper never builds or reads the request, so a cast placeholder keeps the test about forwarding.
  const request = { offset: 0, limit: 50 } as unknown as OperationTreeRequest

  it('forwards the request untouched and returns the window (null when the op is gone)', async () => {
    const view = { rows: [], totalRows: 0 }
    treeMock.mockResolvedValue({ status: 'ok', data: view })
    expect(await getOperationLogTree('op-1', request)).toEqual(view)
    expect(treeMock).toHaveBeenCalledWith({ id: 'op-1', request })

    treeMock.mockResolvedValue({ status: 'ok', data: null })
    expect(await getOperationLogTree('op-1', request)).toBeNull()
  })

  it('throws on an error result rather than showing an empty tree', async () => {
    treeMock.mockResolvedValue({ status: 'error', error: 'db locked' })
    await expect(getOperationLogTree('op-1', request)).rejects.toThrow('db locked')
  })
})

describe('tree backfill', () => {
  beforeEach(() => {
    startBackfillMock.mockReset()
    cancelBackfillMock.mockReset()
    backfillListen.mockReset()
    indexesSettledListen.mockReset()
  })

  it('starts a backfill and returns how the backend answered', async () => {
    startBackfillMock.mockResolvedValue({ status: 'ok', data: { kind: 'started' } })
    expect(await startOperationLogTreeBackfill('op-1')).toEqual({ kind: 'started' })
    expect(startBackfillMock).toHaveBeenCalledWith('op-1')
  })

  it('throws when the backfill request errors', async () => {
    startBackfillMock.mockResolvedValue({ status: 'error', error: 'journal closed' })
    await expect(startOperationLogTreeBackfill('op-1')).rejects.toThrow('journal closed')
  })

  it("cancels the named operation's backfill", async () => {
    cancelBackfillMock.mockResolvedValue(undefined)
    await cancelOperationLogTreeBackfill('op-1')
    expect(cancelBackfillMock).toHaveBeenCalledWith('op-1')
  })

  it('hands each backfill step to the handler as its bare payload, and returns the unlisten', async () => {
    const unlisten = vi.fn()
    backfillListen.mockResolvedValue(unlisten)
    const handler = vi.fn()
    expect(await onOperationLogTreeBackfill(handler)).toBe(unlisten)

    const payload = { opId: 'op-1', step: { kind: 'finished' } }
    backfillListen.mock.calls[0][0]({ payload })
    expect(handler).toHaveBeenCalledWith(payload)
  })

  it('calls the indexes-settled handler with no arguments, and returns the unlisten', async () => {
    const unlisten = vi.fn()
    indexesSettledListen.mockResolvedValue(unlisten)
    const handler = vi.fn()
    expect(await onOperationLogTreeIndexesSettled(handler)).toBe(unlisten)

    indexesSettledListen.mock.calls[0][0]({ payload: null })
    expect(handler).toHaveBeenCalledWith()
  })
})
