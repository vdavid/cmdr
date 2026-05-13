/**
 * IPC contract tests for the file viewer command surface (`viewer_open`,
 * `viewer_get_lines`, `viewer_search_start`, `viewer_search_poll`,
 * `viewer_search_cancel`, `viewer_close`).
 *
 * The viewer runs in a **separate Tauri window** (`viewer-*` label) with its own
 * capability file (`src-tauri/capabilities/viewer.json`). The coverage report flagged
 * this group as entirely untested at the IPC layer (9/9 untested) and noted that
 * permission drift in the capability file would be invisible until a user opens the
 * viewer. mockIPC can't simulate Tauri's permission gate (the gate is on the Rust
 * side; the mock patches `__TAURI_INTERNALS__.invoke` *before* it gets there), but
 * we can pin the wire format — that's the contract that drift can break independently
 * of the permission system.
 */

import { afterEach, describe, expect, it } from 'vitest'

import { commands } from '$lib/ipc/bindings'
import type { LineChunk, SearchPollResult, ViewerOpenResult } from '$lib/ipc/bindings'
import { clearIpcMocks, installIpcMock } from '$lib/ipc/test-helpers'

afterEach(() => {
  clearIpcMocks()
})

const initialLines: LineChunk = {
  lines: ['line 0', 'line 1', 'line 2'],
  firstLineNumber: 0,
  byteOffset: 0,
  totalLines: 3,
  totalBytes: 21,
}

const openResult: ViewerOpenResult = {
  sessionId: 'sess-1',
  fileName: 'README.md',
  totalBytes: 21,
  totalLines: 3,
  estimatedTotalLines: 3,
  backendType: 'fullLoad',
  capabilities: {
    supportsLineSeek: true,
    supportsByteSeek: false,
    supportsFractionSeek: false,
    knowsTotalLines: true,
  },
  initialLines,
  isIndexing: false,
}

describe('commands.viewerOpen', () => {
  it('invokes viewer_open with the path positional arg', async () => {
    const ipc = installIpcMock()
    ipc.mock('viewer_open', () => openResult)

    const result = await commands.viewerOpen('/path/to/README.md')

    expect(result).toEqual({ status: 'ok', data: openResult })
    expect(ipc.lastCall('viewer_open')?.payload).toEqual({ path: '/path/to/README.md' })
  })

  it('surfaces IpcError on the error branch (timedOut: false for non-blocking errors)', async () => {
    const ipc = installIpcMock()
    ipc.mock('viewer_open', () => {
      throw { message: 'File not found', timedOut: false }
    })

    const result = await commands.viewerOpen('/nope.txt')

    expect(result.status).toBe('error')
    if (result.status === 'error') {
      expect(result.error).toEqual({ message: 'File not found', timedOut: false })
    }
  })
})

describe('commands.viewerGetLines', () => {
  it('forwards sessionId, targetType, targetValue, count as camelCase payload keys', async () => {
    const ipc = installIpcMock()
    const chunk: LineChunk = {
      lines: ['x'],
      firstLineNumber: 100,
      byteOffset: 500,
      totalLines: null,
      totalBytes: 1000,
    }
    ipc.mock('viewer_get_lines', () => chunk)

    const targetType = 'byte'
    const targetValue = 500
    const count = 1
    await commands.viewerGetLines('sess-1', targetType, targetValue, count)

    expect(ipc.lastCall('viewer_get_lines')?.payload).toEqual({
      sessionId: 'sess-1',
      targetType,
      targetValue,
      count,
    })
  })
})

describe('commands.viewerSearchStart and viewerSearchPoll', () => {
  it('search_start sends sessionId + query', async () => {
    const ipc = installIpcMock()
    ipc.mock('viewer_search_start', () => null)

    await commands.viewerSearchStart('sess-2', 'TODO')

    expect(ipc.lastCall('viewer_search_start')?.payload).toEqual({
      sessionId: 'sess-2',
      query: 'TODO',
    })
  })

  it('search_poll delta protocol: sinceIndex on the wire matches the FE-tracked offset', async () => {
    const ipc = installIpcMock()
    const pollResult: SearchPollResult = {
      status: 'running',
      newMatches: [{ line: 5, column: 0, length: 4, byteOffset: 80 }],
      totalMatchCount: 6,
      totalBytes: 1000,
      bytesScanned: 800,
      matchLimitReached: false,
    }
    ipc.mock('viewer_search_poll', () => pollResult)

    const result = await commands.viewerSearchPoll('sess-2', 5)

    expect(result).toEqual({ status: 'ok', data: pollResult })
    expect(ipc.lastCall('viewer_search_poll')?.payload).toEqual({
      sessionId: 'sess-2',
      sinceIndex: 5,
    })
  })

  it('search_cancel takes only sessionId', async () => {
    const ipc = installIpcMock()
    ipc.mock('viewer_search_cancel', () => null)

    await commands.viewerSearchCancel('sess-2')

    expect(ipc.lastCall('viewer_search_cancel')?.payload).toEqual({ sessionId: 'sess-2' })
  })
})

describe('commands.viewerClose', () => {
  it('takes only sessionId and resolves to data: null on success', async () => {
    const ipc = installIpcMock()
    ipc.mock('viewer_close', () => null)

    const result = await commands.viewerClose('sess-3')

    expect(result).toEqual({ status: 'ok', data: null })
    expect(ipc.lastCall('viewer_close')?.payload).toEqual({ sessionId: 'sess-3' })
  })

  it('surfaces a string error on the error branch (viewer_close uses Result<_, String>)', async () => {
    const ipc = installIpcMock()
    ipc.mock('viewer_close', () => {
      throw 'session not found'
    })

    const result = await commands.viewerClose('bogus')

    expect(result.status).toBe('error')
    if (result.status === 'error') {
      expect(result.error).toBe('session not found')
    }
  })
})
