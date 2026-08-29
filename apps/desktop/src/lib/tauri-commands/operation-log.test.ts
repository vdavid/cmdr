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
const rollbackMock = vi.fn<(id: string) => Promise<Res<unknown, { kind: string }>>>()
vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    getRecentOperationLogEntries: (limit: number, offset: number) => getRecentMock({ limit, offset }),
    getOperationLogDetail: (id: string, l: number, o: number) =>
      getDetailMock({ operationId: id, itemLimit: l, itemOffset: o }),
    undoOperations: (ids: string[]) => undoMock(ids),
    rollbackOperation: (id: string) => rollbackMock(id),
  },
}))

import {
  getRecentOperationLogEntries,
  getOperationLogDetail,
  undoOperations,
  rollbackOperation,
} from './operation-log'
import { asRollbackRefusal } from '$lib/operation-log/rollback-refusal'

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
    expect(await rollbackOperation('op-1')).toEqual({ inverseOpId: 'op-2' })
    expect(rollbackMock).toHaveBeenCalledWith('op-1')
  })

  it('throws the refusal TYPED, so the caller can word each reason itself', async () => {
    rollbackMock.mockResolvedValue({ status: 'error', error: { kind: 'alreadyRollingBack' } })
    // A flattened string here would put wire JSON in front of the user; the typed
    // value is what lets the history dialog pick the right sentence.
    const thrown = await rollbackOperation('op-1').catch((e: unknown) => e)
    expect(asRollbackRefusal(thrown)).toEqual({ kind: 'alreadyRollingBack' })
  })
})
