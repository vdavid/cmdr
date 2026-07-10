/**
 * Behaviour tests for the Operation-log dialog trigger: the IPC-fetch + paging
 * state on top of `getRecentOperationLogEntries`. Pins open (page 1), load-more
 * (append the next page, offset = current length, no dupes), the short-page
 * `hasMore` flip, and the open-on-failure-with-notice path.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { OperationRow } from '$lib/ipc/bindings'

const getRecentMock = vi.fn<(limit: number, offset: number) => Promise<OperationRow[]>>()
vi.mock('$lib/tauri-commands', () => ({
  getRecentOperationLogEntries: (limit: number, offset: number) => getRecentMock(limit, offset),
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import {
  operationLogState,
  openOperationLog,
  loadMoreOperations,
  closeOperationLog,
  OPERATION_LOG_PAGE,
} from './operation-log-trigger.svelte'

/** A minimal `OperationRow` with a distinct `opId`, enough for paging assertions. */
function row(opId: string): OperationRow {
  return {
    opId,
    kind: 'copy',
    archiveSubkind: null,
    initiator: 'user',
    executionStatus: 'done',
    rollbackState: 'rollbackable',
    notRollbackableReason: null,
    rollsBackOpId: null,
    sourceVolumeId: 'root',
    destVolumeId: null,
    startedAt: 1_700_000_000,
    endedAt: 1_700_000_010,
    itemCount: 3,
    itemsDone: 3,
    bytesTotal: 0,
    searchCoverage: 'full',
    searchCoverageReason: null,
    devSummary: null,
  }
}

/** A full page of `OPERATION_LOG_PAGE` distinct rows, ids prefixed for uniqueness. */
function fullPage(prefix: string): OperationRow[] {
  return Array.from({ length: OPERATION_LOG_PAGE }, (_, i) => row(`${prefix}-${String(i)}`))
}

describe('openOperationLog', () => {
  beforeEach(() => {
    getRecentMock.mockReset()
    closeOperationLog()
    operationLogState.entries = []
    operationLogState.hasMore = false
    operationLogState.loadError = false
    operationLogState.loadingMore = false
  })

  it('opens and loads page 1 (offset 0), flagging hasMore on a full page', async () => {
    getRecentMock.mockResolvedValue(fullPage('a'))
    await openOperationLog()

    expect(getRecentMock).toHaveBeenCalledWith(OPERATION_LOG_PAGE, 0)
    expect(operationLogState.open).toBe(true)
    expect(operationLogState.entries).toHaveLength(OPERATION_LOG_PAGE)
    expect(operationLogState.hasMore).toBe(true)
    expect(operationLogState.loadError).toBe(false)
  })

  it('leaves hasMore false on a short first page', async () => {
    getRecentMock.mockResolvedValue([row('a'), row('b')])
    await openOperationLog()

    expect(operationLogState.hasMore).toBe(false)
    expect(operationLogState.entries).toHaveLength(2)
  })

  it('is idempotent: a second open while already open does not refetch', async () => {
    getRecentMock.mockResolvedValue([row('a')])
    await openOperationLog()
    await openOperationLog()

    expect(getRecentMock).toHaveBeenCalledTimes(1)
  })

  it('still opens on a read failure and flags loadError', async () => {
    getRecentMock.mockRejectedValue(new Error('db locked'))
    await openOperationLog()

    expect(operationLogState.open).toBe(true)
    expect(operationLogState.loadError).toBe(true)
    expect(operationLogState.entries).toHaveLength(0)
  })
})

describe('loadMoreOperations', () => {
  beforeEach(() => {
    getRecentMock.mockReset()
    closeOperationLog()
    operationLogState.entries = []
    operationLogState.hasMore = false
    operationLogState.loadingMore = false
  })

  it('appends the next page at offset = current length, without duplicates', async () => {
    const page1 = fullPage('a')
    const page2 = fullPage('b')
    getRecentMock.mockResolvedValueOnce(page1).mockResolvedValueOnce(page2)

    await openOperationLog()
    await loadMoreOperations()

    // Second fetch is offset by the first page's length.
    expect(getRecentMock).toHaveBeenNthCalledWith(2, OPERATION_LOG_PAGE, OPERATION_LOG_PAGE)
    expect(operationLogState.entries).toHaveLength(OPERATION_LOG_PAGE * 2)

    // No duplicate opIds across the two appended pages.
    const ids = operationLogState.entries.map((e) => e.opId)
    expect(new Set(ids).size).toBe(ids.length)
    expect(operationLogState.hasMore).toBe(true)
  })

  it('flips hasMore false when the appended page is short', async () => {
    getRecentMock.mockResolvedValueOnce(fullPage('a')).mockResolvedValueOnce([row('b-0'), row('b-1')])

    await openOperationLog()
    await loadMoreOperations()

    expect(operationLogState.entries).toHaveLength(OPERATION_LOG_PAGE + 2)
    expect(operationLogState.hasMore).toBe(false)
  })

  it('does nothing when there is no more to load', async () => {
    getRecentMock.mockResolvedValue([row('a')])
    await openOperationLog() // short page → hasMore false
    getRecentMock.mockClear()

    await loadMoreOperations()
    expect(getRecentMock).not.toHaveBeenCalled()
  })
})
