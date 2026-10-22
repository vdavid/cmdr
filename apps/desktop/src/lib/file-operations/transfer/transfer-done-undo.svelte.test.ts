/**
 * The done state's Undo (#484): the operation log's own finished-operation
 * rollback, offered from the progress dialog. It reads the journal only once
 * `write-settled` says its rows are there, offers exactly what the journal admits,
 * asks before it acts, and hands the reversal off like the history dialog does.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import type { OperationRow, RollbackRefusal } from '#lib/ipc/bindings.js'

const { getOperationLogDetail, rollbackOperation } = vi.hoisted(() => ({
  getOperationLogDetail: vi.fn(),
  rollbackOperation: vi.fn(),
}))
vi.mock('#lib/tauri-commands/index.js', () => ({ getOperationLogDetail, rollbackOperation }))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))

import { createDoneUndo } from './transfer-done-undo.svelte'
import { RollbackRefusalFailure } from '#lib/operation-log/rollback-refusal.js'

function row(over: Partial<OperationRow> = {}): OperationRow {
  return {
    opId: 'op-1',
    kind: 'copy',
    archiveSubkind: null,
    initiator: 'user',
    executionStatus: 'completed',
    rollbackState: 'rollbackable',
    notRollbackableReason: null,
    rollsBackOpId: null,
    inverseOpId: null,
    sourceVolumeId: 'root',
    destVolumeId: 'root',
    startedAt: 1,
    endedAt: 2,
    itemCount: 1,
    itemsDone: 1,
    bytesTotal: 10,
    searchCoverage: 'complete',
    searchCoverageReason: null,
    devSummary: null,
    ...over,
  } as OperationRow
}

async function drain(): Promise<void> {
  for (let i = 0; i < 10; i++) await Promise.resolve()
  flushSync()
}

let dispose: (() => void) | null = null
const flags = { offered: true, settled: false }
const onDispatched = vi.fn()

function make() {
  let undo!: ReturnType<typeof createDoneUndo>
  dispose = $effect.root(() => {
    undo = createDoneUndo({
      operationId: () => 'op-1',
      offered: () => flags.offered,
      settled: () => flags.settled,
      onDispatched,
    })
  })
  return undo
}

beforeEach(() => {
  vi.clearAllMocks()
  flags.offered = true
  flags.settled = false
})

afterEach(() => {
  dispose?.()
  dispose = null
})

describe('createDoneUndo', () => {
  it('waits, without reading the journal, until the operation settles', async () => {
    const undo = make()
    await drain()

    expect(undo.state.kind).toBe('waiting')
    expect(getOperationLogDetail).not.toHaveBeenCalled()
  })

  it('offers Undo once the journal says the operation can be rolled back', async () => {
    getOperationLogDetail.mockResolvedValue({ operation: row(), items: [], totalItems: 0 })
    flags.settled = true
    const undo = make()
    await drain()

    expect(getOperationLogDetail).toHaveBeenCalledWith('op-1', 0, 0)
    expect(undo.state).toEqual({ kind: 'ready', opKind: 'copy', finishing: false })
  })

  it('says why when the journal says it can’t be undone', async () => {
    getOperationLogDetail.mockResolvedValue({
      operation: row({ rollbackState: 'notRollbackable', notRollbackableReason: 'directoryMerge' }),
      items: [],
      totalItems: 0,
    })
    flags.settled = true
    const undo = make()
    await drain()

    expect(undo.state.kind).toBe('blocked')
  })

  it('offers nothing for an operation the journal never recorded', async () => {
    getOperationLogDetail.mockResolvedValue(null)
    flags.settled = true
    const undo = make()
    await drain()

    expect(undo.state.kind).toBe('none')
  })

  it('offers nothing where the dialog doesn’t offer it at all', async () => {
    flags.offered = false
    flags.settled = true
    const undo = make()
    await drain()

    expect(undo.state.kind).toBe('none')
    expect(getOperationLogDetail).not.toHaveBeenCalled()
  })

  it('asks first, then hands the reversal off', async () => {
    getOperationLogDetail.mockResolvedValue({ operation: row(), items: [], totalItems: 0 })
    rollbackOperation.mockResolvedValue({ inverseOpId: 'op-2' })
    flags.settled = true
    const undo = make()
    await drain()

    undo.request()
    expect(undo.confirming).toBe(true)
    expect(rollbackOperation).not.toHaveBeenCalled()

    await undo.confirm()

    expect(rollbackOperation).toHaveBeenCalledWith('op-1')
    expect(onDispatched).toHaveBeenCalledTimes(1)
  })

  it('words a refusal and stays put', async () => {
    getOperationLogDetail.mockResolvedValue({ operation: row(), items: [], totalItems: 0 })
    const refusal: RollbackRefusal = { kind: 'alreadyRollingBack' }
    rollbackOperation.mockRejectedValue(new RollbackRefusalFailure(refusal))
    flags.settled = true
    const undo = make()
    await drain()

    undo.request()
    await undo.confirm()

    expect(onDispatched).not.toHaveBeenCalled()
    expect(undo.refusal).toBe('operationLog.rollback.refusalAlreadyRollingBack')
  })
})
