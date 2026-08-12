import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { OperationSnapshot, WriteProgressEvent } from '$lib/ipc/bindings'

const { listOperationsMock } = vi.hoisted(() => ({
  listOperationsMock: vi.fn<() => Promise<OperationSnapshot[]>>(() => Promise.resolve([])),
}))

vi.mock('$lib/tauri-commands', () => ({
  listOperations: listOperationsMock,
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
  onWriteComplete: vi.fn(() => Promise.resolve(() => {})),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflict: vi.fn(() => Promise.resolve(() => {})),
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
}))

import * as progressReadout from '../progress-readout'
import { createOperationSessionRegistry } from './operation-session-registry'

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

  it('drops every session on window teardown', () => {
    const registry = createOperationSessionRegistry()
    const session = registry.acquire('a')

    registry.dispose()
    registry._testEmit({ kind: 'progress', event: progress('a', { bytesDone: 7 }) })

    expect(session.readout).toBeNull()
  })
})
