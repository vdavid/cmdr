/**
 * Pins how a failed viewer open is logged and what it shows. An ERROR log counts toward an
 * auto-sent error report, so it's for failures that are ours to fix. An environmental
 * outcome the window already renders with a way forward (a timeout, a file that's gone) logs
 * at warn: at error level those filed reports of their own (ERR-GW3BE, ERR-XV6SN).
 */

import { describe, expect, it, vi } from 'vitest'

import { handleOpenFailure } from './viewer-open-failure'
import { tString } from '$lib/intl/messages.svelte'
import type { Logger } from '$lib/logging/logger'
import type { ViewerError } from '$lib/ipc/bindings'

function fakeLog() {
  const log = { trace: vi.fn(), debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() }
  return { log, asLogger: log as unknown as Logger }
}

/** The shape `viewerOpen` throws: an `Error` carrying the typed `ViewerError`. */
function viewerFailure(viewerError: ViewerError): Error {
  return Object.assign(new Error(viewerError.kind), { viewerError })
}

describe('handleOpenFailure log level', () => {
  // What the world did to the open, and the window says so.
  const environmental: ViewerError[] = [
    { kind: 'timedOut' },
    { kind: 'stoppedResponding' },
    { kind: 'notFound', path: '/gone.txt' },
    { kind: 'isDirectory' },
    { kind: 'tooLargeToPreview', size: 2_000_000_000, cap: 1_000_000_000 },
    { kind: 'archive', failure: 'unreadable', message: 'encrypted entry' },
    { kind: 'cancelled' },
    { kind: 'io', message: 'Permission denied (os error 13)' },
  ]
  // Can't happen on an open unless our own code is wrong.
  const defects: ViewerError[] = [
    { kind: 'sessionNotFound', sessionId: 'sess-1' },
    { kind: 'outOfRange' },
    { kind: 'destinationIsReadOnly' },
  ]

  it.each(environmental.map((e) => [e.kind, e] as const))('logs %s at warn', (_kind, viewerError) => {
    const { log, asLogger } = fakeLog()

    handleOpenFailure(asLogger, 'Open', viewerFailure(viewerError))

    expect(log.error).not.toHaveBeenCalled()
    expect(log.warn).toHaveBeenCalledTimes(1)
  })

  it.each(defects.map((e) => [e.kind, e] as const))('keeps %s at error, since it is a defect', (_kind, viewerError) => {
    const { log, asLogger } = fakeLog()

    handleOpenFailure(asLogger, 'Open', viewerFailure(viewerError))

    expect(log.error).toHaveBeenCalledTimes(1)
    expect(log.warn).not.toHaveBeenCalled()
  })

  it('keeps a failure that never reached the typed path at error, since that one is a defect', () => {
    const { log, asLogger } = fakeLog()

    const failure = handleOpenFailure(asLogger, 'Open', new TypeError('undefined is not an object'))

    expect(failure).toEqual({ message: tString('viewer.error.readFailed'), canRetry: true })
    expect(log.error).toHaveBeenCalledTimes(1)
    expect(log.warn).not.toHaveBeenCalled()
  })
})

describe('handleOpenFailure copy', () => {
  function copyFor(viewerError: ViewerError) {
    return handleOpenFailure(fakeLog().asLogger, 'Open', viewerFailure(viewerError))
  }

  it('offers Retry for a timed-out open', () => {
    expect(copyFor({ kind: 'timedOut' })).toEqual({ message: tString('viewer.error.timeout'), canRetry: true })
  })

  it('says a file that is gone was moved or deleted, not that reading it went wrong', () => {
    expect(copyFor({ kind: 'notFound', path: '/gone.txt' })).toEqual({
      message: tString('viewer.error.notFound'),
      canRetry: false,
    })
  })

  it('says a folder has nothing to view', () => {
    expect(copyFor({ kind: 'isDirectory' })).toEqual({ message: tString('viewer.error.isDirectory'), canRetry: false })
  })
})
