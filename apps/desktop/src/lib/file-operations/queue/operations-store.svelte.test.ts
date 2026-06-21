import { describe, it, expect, vi } from 'vitest'
import { flushSync } from 'svelte'
import type { OperationSnapshot, WriteProgressEvent } from '$lib/ipc/bindings'

// The store subscribes to Tauri events through `$lib/tauri-commands`. Mock those
// so `init()` doesn't reach a backend; the reducer tests drive the store through
// its `_test*` seams instead.
vi.mock('$lib/tauri-commands', () => ({
  listOperations: vi.fn(() => Promise.resolve([])),
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
}))

import { createOperationsStore, isTerminalStatus } from './operations-store.svelte'

function snapshot(id: string, status: OperationSnapshot['status'], over: Partial<OperationSnapshot> = {}): OperationSnapshot {
  return {
    operationId: id,
    operationType: 'copy',
    status,
    source: '/src/file',
    destination: '/dst/file',
    ...over,
  }
}

function progress(id: string, over: Partial<WriteProgressEvent> = {}): WriteProgressEvent {
  return {
    operationId: id,
    operationType: 'copy',
    phase: 'copying',
    currentFile: 'file',
    filesDone: 1,
    filesTotal: 2,
    bytesDone: 50,
    bytesTotal: 100,
    ...over,
  }
}

describe('operations store reducers', () => {
  it('reduces an operations-changed snapshot into rows', () => {
    const cleanup = $effect.root(() => {
      const store = createOperationsStore()
      store._testApplySnapshot([snapshot('a', 'running'), snapshot('b', 'queued')])
      flushSync()
      expect(store.operations.map((r) => r.snapshot.operationId)).toEqual(['a', 'b'])
      expect(store.operations.every((r) => r.progress === null)).toBe(true)
    })
    cleanup()
  })

  it('merges write-progress onto the matching row, ignoring unknown ops', () => {
    const cleanup = $effect.root(() => {
      const store = createOperationsStore()
      store._testApplySnapshot([snapshot('a', 'running')])
      store._testApplyProgress(progress('a', { bytesDone: 75 }))
      // Progress for an op not in the snapshot is dropped.
      store._testApplyProgress(progress('ghost'))
      flushSync()
      const rows = store.operations
      expect(rows).toHaveLength(1)
      expect(rows[0].progress?.bytesDone).toBe(75)
    })
    cleanup()
  })

  it('prunes progress for ops that leave the snapshot', () => {
    const cleanup = $effect.root(() => {
      const store = createOperationsStore()
      store._testApplySnapshot([snapshot('a', 'running'), snapshot('b', 'running')])
      store._testApplyProgress(progress('a'))
      store._testApplyProgress(progress('b'))
      flushSync()
      // `a` finishes and leaves the snapshot; its progress must not linger.
      store._testApplySnapshot([snapshot('b', 'running')])
      store._testApplyProgress(progress('b', { bytesDone: 90 }))
      flushSync()
      const rows = store.operations
      expect(rows.map((r) => r.snapshot.operationId)).toEqual(['b'])
      expect(rows[0].progress?.bytesDone).toBe(90)
    })
    cleanup()
  })

  it('tracks running and paused presence for the global toolbar', () => {
    const cleanup = $effect.root(() => {
      const store = createOperationsStore()
      store._testApplySnapshot([snapshot('a', 'running'), snapshot('b', 'queued')])
      flushSync()
      expect(store.hasRunning).toBe(true)
      expect(store.hasPaused).toBe(false)

      store._testApplySnapshot([snapshot('a', 'paused'), snapshot('b', 'queued')])
      flushSync()
      expect(store.hasRunning).toBe(false)
      expect(store.hasPaused).toBe(true)
    })
    cleanup()
  })
})

describe('isTerminalStatus', () => {
  it('treats done/cancelled/failed as terminal and the rest as live', () => {
    expect(isTerminalStatus('done')).toBe(true)
    expect(isTerminalStatus('cancelled')).toBe(true)
    expect(isTerminalStatus('failed')).toBe(true)
    expect(isTerminalStatus('running')).toBe(false)
    expect(isTerminalStatus('queued')).toBe(false)
    expect(isTerminalStatus('paused')).toBe(false)
  })
})
