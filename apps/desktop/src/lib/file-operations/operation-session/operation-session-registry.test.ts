import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { OperationSnapshot, WriteProgressEvent } from '$lib/ipc/bindings'

const { listOperationsMock, cancelOperationMock } = vi.hoisted(() => ({
  listOperationsMock: vi.fn<() => Promise<OperationSnapshot[]>>(() => Promise.resolve([])),
  cancelOperationMock: vi.fn<(id: string) => Promise<void>>(() => Promise.resolve()),
}))

vi.mock('$lib/tauri-commands', () => ({
  listOperations: listOperationsMock,
  pauseOperation: vi.fn(() => Promise.resolve()),
  resumeOperation: vi.fn(() => Promise.resolve()),
  cancelOperation: cancelOperationMock,
  cancelWriteOperation: vi.fn(() => Promise.resolve()),
  resolveWriteConflict: vi.fn(() => Promise.resolve('resolved')),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
  onWriteComplete: vi.fn(() => Promise.resolve(() => {})),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflict: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflictResolved: vi.fn(() => Promise.resolve(() => {})),
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
}))

import * as progressReadout from '../progress-readout'
import { createOperationSessionRegistry } from './operation-session-registry'

function snapshot(id: string, status: OperationSnapshot['status'] = 'running'): OperationSnapshot {
  return {
    operationId: id,
    operationType: 'copy',
    status,
    source: '/src',
    destination: '/dst',
    supportsRollback: true,
    error: null,
  }
}

/** Lets every session's seed, had it taken one, run to its continuation. */
async function flushSeeds(): Promise<void> {
  await Promise.resolve()
  await Promise.resolve()
}

function progress(id: string, over: Partial<WriteProgressEvent> = {}): WriteProgressEvent {
  return {
    operationId: id,
    operationType: 'copy',
    phase: 'copying',
    currentFile: 'file',
    filesDone: 1,
    filesTotal: 10,
    bytesDone: 50,
    bytesTotal: 1000,
    ...over,
  }
}

beforeEach(() => {
  listOperationsMock.mockReset()
  listOperationsMock.mockResolvedValue([])
  cancelOperationMock.mockReset()
  cancelOperationMock.mockResolvedValue(undefined)
  vi.restoreAllMocks()
})

describe('the session registry', () => {
  it('hands two views of one operation the same session', () => {
    const registry = createOperationSessionRegistry()

    const forTheChip = registry.acquire('a')
    const forTheRow = registry.acquire('a')

    expect(forTheRow).toBe(forTheChip)
    registry.dispose()
  })

  it('builds exactly one ETA smoother per operation, however many views watch it', () => {
    const smootherSpy = vi.spyOn(progressReadout, 'createEtaSmoother')
    const registry = createOperationSessionRegistry()

    registry.acquire('a')
    registry.acquire('a')
    registry.acquire('b')

    expect(smootherSpy).toHaveBeenCalledTimes(2)
    registry.dispose()
  })

  it('keeps the session alive until the LAST view releases it', () => {
    const registry = createOperationSessionRegistry()
    const session = registry.acquire('a')
    registry.acquire('a')

    registry.release('a')
    registry._testEmit({ kind: 'progress', event: progress('a', { bytesDone: 60 }) })
    expect(session.readout?.bytesDone).toBe(60)

    registry.release('a')
    registry._testEmit({ kind: 'progress', event: progress('a', { bytesDone: 999 }) })
    expect(session.readout?.bytesDone).toBe(60)
    registry.dispose()
  })

  it('builds a fresh session after the last release, rather than reviving a dead one', () => {
    const registry = createOperationSessionRegistry()
    const first = registry.acquire('a')
    registry.release('a')

    const second = registry.acquire('a')

    expect(second).not.toBe(first)
    registry._testEmit({ kind: 'progress', event: progress('a', { bytesDone: 42 }) })
    expect(second.readout?.bytesDone).toBe(42)
    registry.dispose()
  })

  it('ignores a release for an operation it never handed out', () => {
    const registry = createOperationSessionRegistry()
    expect(() => {
      registry.release('never-seen')
    }).not.toThrow()
    registry.dispose()
  })

  it('shows one view the command another view issued', async () => {
    // "The operation doesn't care where a command comes from" is only true if
    // the surfaces agree about what has been asked. A Cancel pressed on a queue
    // row has to reach the chip watching the same transfer, and later an MCP
    // call has to reach both.
    const registry = createOperationSessionRegistry()
    const rowsView = registry.acquire('a')
    const chipView = registry.acquire('a')

    await rowsView.cancel()

    expect(cancelOperationMock).toHaveBeenCalledWith('a')
    expect(chipView.cancelling).toBe(true)
    // And the second view can't send it again.
    expect(await chipView.cancel()).toBe(false)
    expect(cancelOperationMock).toHaveBeenCalledTimes(1)
    registry.dispose()
  })

  it('opens a cold window on ONE list_operations call, however many rows it shows', async () => {
    // What the operation queue does on a cold open: init, then a session per
    // row, before any `operations-changed` has been broadcast. The fan-out's own
    // seed is the answer to all of them, so no row pays for its own round trip.
    const rows = ['a', 'b', 'c', 'd', 'e']
    listOperationsMock.mockResolvedValue(rows.map((id) => snapshot(id, 'running')))
    const registry = createOperationSessionRegistry()
    await registry.init()

    const sessions = rows.map((id) => registry.acquire(id))
    await flushSeeds()

    expect(listOperationsMock).toHaveBeenCalledTimes(1)
    expect(sessions.map((session) => session.status)).toEqual(rows.map(() => 'running'))
    registry.dispose()
  })

  it('still resolves a row whose operation ended before the window opened', async () => {
    // The seed can't answer for an id it doesn't carry, so that session asks for
    // itself and resolves as already over rather than sitting empty.
    listOperationsMock.mockResolvedValue([snapshot('a')])
    const registry = createOperationSessionRegistry()
    await registry.init()

    const ghost = registry.acquire('ended-already')
    await flushSeeds()

    expect(ghost.outcome).toEqual({ kind: 'gone' })
    expect(listOperationsMock).toHaveBeenCalledTimes(2)
    registry.dispose()
  })

  it('drops every session on window teardown', () => {
    const registry = createOperationSessionRegistry()
    const session = registry.acquire('a')

    registry.dispose()
    registry._testEmit({ kind: 'progress', event: progress('a', { bytesDone: 7 }) })

    expect(session.readout).toBeNull()
  })
})
