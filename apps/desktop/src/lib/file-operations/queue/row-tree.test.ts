import { describe, it, expect } from 'vitest'
import type { OperationSnapshot, WriteOperationPhase } from '#lib/ipc/bindings.js'
import type { OperationRow } from './operations-store.svelte'
import { rowTreeOperation, treesCollapsible } from './row-tree'

function row(
  over: Partial<OperationSnapshot> = {},
  phase: WriteOperationPhase | null = 'copying',
  operationId = 'op-1',
): OperationRow {
  const snapshot: OperationSnapshot = {
    operationId,
    operationType: 'copy',
    status: 'running',
    source: '/Users/me/Documents',
    destination: '/Volumes/Backup',
    rollbackSupport: 'supported',
    reverses: null,
    error: null,
    canRetry: false,
    conflictPolicy: null,
    items: null,
    ...over,
  }
  return {
    snapshot,
    progress:
      phase === null
        ? null
        : {
            operationId,
            operationType: snapshot.operationType,
            phase,
            currentFile: null,
            filesDone: 1,
            filesTotal: 10,
            bytesDone: 1,
            bytesTotal: 10,
          },
  }
}

describe('rowTreeOperation', () => {
  it('shows a tree on a running or paused transfer past its scan', () => {
    expect(rowTreeOperation(row())).toBe('copy')
    expect(rowTreeOperation(row({ status: 'paused', operationType: 'move' }))).toBe('move')
    expect(rowTreeOperation(row({ operationType: 'compress' }, 'compressing'))).toBe('compress')
  })

  it('shows none on a waiting or failed row, or one that has ended', () => {
    for (const status of ['queued', 'failed', 'done', 'cancelled'] as const) {
      expect(rowTreeOperation(row({ status }))).toBeNull()
    }
  })

  it('shows none while the operation counts, or before its first tick', () => {
    expect(rowTreeOperation(row({}, 'scanning'))).toBeNull()
    expect(rowTreeOperation(row({}, null))).toBeNull()
  })

  it('shows none for a delete, or for an operation that is undoing something', () => {
    expect(rowTreeOperation(row({ operationType: 'delete' }, 'deleting'))).toBeNull()
    expect(rowTreeOperation(row({}, 'rolling_back'))).toBeNull()
    expect(rowTreeOperation(row({ operationType: 'move', reverses: 'move' }))).toBeNull()
  })
})

describe('treesCollapsible', () => {
  it('offers the toggle once two rows show a tree', () => {
    expect(treesCollapsible([row({}, 'copying', 'a'), row({ status: 'paused' }, 'copying', 'b')])).toBe(true)
  })

  it('offers none for a lone tree, whatever else is in the list', () => {
    expect(treesCollapsible([row({}, 'copying', 'a')])).toBe(false)
    expect(
      treesCollapsible([
        row({}, 'copying', 'a'),
        row({ operationType: 'delete' }, 'deleting', 'b'),
        row({ status: 'queued' }, null, 'c'),
        row({}, 'scanning', 'd'),
      ]),
    ).toBe(false)
  })
})
