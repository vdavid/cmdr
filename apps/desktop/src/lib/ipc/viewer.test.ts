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
 * we can pin the wire format. That's the contract that drift can break independently
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
  encoding: 'utf8',
  kind: 'text',
  mediaToken: null,
  mediaDimensions: null,
}

describe('commands.viewerOpen', () => {
  it('invokes viewer_open with the path and window-label positional args', async () => {
    const ipc = installIpcMock()
    ipc.mock('viewer_open', () => openResult)

    const result = await commands.viewerOpen('/path/to/README.md', 'viewer-123')

    expect(result).toEqual({ status: 'ok', data: openResult })
    expect(ipc.lastCall('viewer_open')?.payload).toEqual({ path: '/path/to/README.md', windowLabel: 'viewer-123' })
  })

  it('surfaces IpcError on the error branch (timedOut: false for non-blocking errors)', async () => {
    const ipc = installIpcMock()
    ipc.mock('viewer_open', () => {
      throw { message: 'File not found', timedOut: false }
    })

    const result = await commands.viewerOpen('/nope.txt', 'viewer-123')

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
  it('search_start sends sessionId + query + mode (camelCase)', async () => {
    const ipc = installIpcMock()
    ipc.mock('viewer_search_start', () => null)

    const mode = { useRegex: false, caseSensitive: true } as const
    await commands.viewerSearchStart('sess-2', 'TODO', mode)

    expect(ipc.lastCall('viewer_search_start')?.payload).toEqual({
      sessionId: 'sess-2',
      query: 'TODO',
      mode,
    })
  })

  it('search_start carries regex + case toggles for a regex query', async () => {
    const ipc = installIpcMock()
    ipc.mock('viewer_search_start', () => null)

    const mode = { useRegex: true, caseSensitive: false } as const
    await commands.viewerSearchStart('sess-2a', String.raw`\d+`, mode)

    expect(ipc.lastCall('viewer_search_start')?.payload).toEqual({
      sessionId: 'sess-2a',
      query: String.raw`\d+`,
      mode,
    })
  })

  it('search_poll surfaces invalidQuery as a tagged variant with message', async () => {
    const ipc = installIpcMock()
    const pollResult: SearchPollResult = {
      status: { status: 'invalidQuery', message: 'Invalid regex: error parsing' },
      newMatches: [],
      totalMatchCount: 0,
      totalBytes: 100,
      bytesScanned: 0,
      matchLimitReached: false,
    }
    ipc.mock('viewer_search_poll', () => pollResult)

    const result = await commands.viewerSearchPoll('sess-2', 0)
    expect(result).toEqual({ status: 'ok', data: pollResult })
  })

  it('search_poll delta protocol: sinceIndex on the wire matches the FE-tracked offset', async () => {
    const ipc = installIpcMock()
    const pollResult: SearchPollResult = {
      status: { status: 'running' },
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

describe('commands.viewerReadRange', () => {
  it('forwards sessionId, readId, anchor, focus as camelCase payload keys', async () => {
    const ipc = installIpcMock()
    ipc.mock('viewer_read_range', () => 'hello world')

    const sessionId = 'sess-4'
    const readId = 17
    const anchor = { kind: 'line', line: 0, offset: 0 } as const
    const focus = { kind: 'line', line: 0, offset: 11 } as const

    const result = await commands.viewerReadRange(sessionId, readId, anchor, focus)

    expect(result).toEqual({ status: 'ok', data: 'hello world' })
    expect(ipc.lastCall('viewer_read_range')?.payload).toEqual({
      sessionId,
      readId,
      anchor,
      focus,
    })
  })

  it('passes through RangeEnd::Eof as a tagged variant', async () => {
    const ipc = installIpcMock()
    ipc.mock('viewer_read_range', () => 'whole file content')

    const sessionId = 'sess-5'
    const readId = 0
    const anchor = { kind: 'line', line: 0, offset: 0 } as const
    const focus = { kind: 'eof' } as const

    await commands.viewerReadRange(sessionId, readId, anchor, focus)

    expect(ipc.lastCall('viewer_read_range')?.payload).toEqual({
      sessionId,
      readId,
      anchor,
      focus,
    })
  })

  it('surfaces a typed `Cancelled` error on the error branch', async () => {
    const ipc = installIpcMock()
    ipc.mock('viewer_read_range', () => {
      throw { kind: 'cancelled' }
    })

    const sessionId = 'sess-6'
    const readId = 1
    const anchor = { kind: 'line', line: 0, offset: 0 } as const
    const focus = { kind: 'line', line: 100, offset: 0 } as const

    const result = await commands.viewerReadRange(sessionId, readId, anchor, focus)

    expect(result.status).toBe('error')
    if (result.status === 'error') {
      expect(result.error).toEqual({ kind: 'cancelled' })
    }
  })

  it('surfaces a typed `TimedOut` error on the error branch', async () => {
    const ipc = installIpcMock()
    ipc.mock('viewer_read_range', () => {
      throw { kind: 'timedOut' }
    })

    const sessionId = 'sess-7'
    const readId = 1
    const anchor = { kind: 'line', line: 0, offset: 0 } as const
    const focus = { kind: 'eof' } as const

    const result = await commands.viewerReadRange(sessionId, readId, anchor, focus)

    expect(result.status).toBe('error')
    if (result.status === 'error') {
      expect(result.error).toEqual({ kind: 'timedOut' })
    }
  })
})

describe('commands.viewerCancelRead', () => {
  it('takes sessionId and readId as camelCase payload keys', async () => {
    const ipc = installIpcMock()
    ipc.mock('viewer_cancel_read', () => null)

    const sessionId = 'sess-8'
    const readId = 99
    const result = await commands.viewerCancelRead(sessionId, readId)

    expect(result).toEqual({ status: 'ok', data: null })
    expect(ipc.lastCall('viewer_cancel_read')?.payload).toEqual({ sessionId, readId })
  })
})

describe('commands.viewerSetTailMode', () => {
  it('sends sessionId and enabled as camelCase payload keys', async () => {
    const ipc = installIpcMock()
    ipc.mock('viewer_set_tail_mode', () => null)

    const sessionId = 'sess-tail-1'
    const result = await commands.viewerSetTailMode(sessionId, true)
    expect(result).toEqual({ status: 'ok', data: null })
    expect(ipc.lastCall('viewer_set_tail_mode')?.payload).toEqual({ sessionId, enabled: true })
  })

  it('round-trips disabling tail mode', async () => {
    const ipc = installIpcMock()
    ipc.mock('viewer_set_tail_mode', () => null)

    await commands.viewerSetTailMode('sess-tail-2', false)
    expect(ipc.lastCall('viewer_set_tail_mode')?.payload).toEqual({ sessionId: 'sess-tail-2', enabled: false })
  })
})

describe('commands.viewerReload', () => {
  it('sends sessionId as camelCase payload key', async () => {
    const ipc = installIpcMock()
    ipc.mock('viewer_reload', () => null)

    const result = await commands.viewerReload('sess-reload-1')
    expect(result).toEqual({ status: 'ok', data: null })
    expect(ipc.lastCall('viewer_reload')?.payload).toEqual({ sessionId: 'sess-reload-1' })
  })
})
