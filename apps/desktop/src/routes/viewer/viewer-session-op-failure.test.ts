import { describe, expect, it, vi } from 'vitest'

import type { ViewerError } from '#lib/tauri-commands/index.js'
import type { Logger } from '#lib/logging/logger.js'
import { logSessionOpFailure, sessionOpFailureKey } from './viewer-session-op-failure'

const timedOut: ViewerError = { kind: 'timedOut' }
const notFound: ViewerError = { kind: 'notFound', path: '/tmp/gone.log' }
const io: ViewerError = { kind: 'io', message: 'Permission denied (os error 13)' }
const noSession: ViewerError = { kind: 'sessionNotFound', sessionId: 'sess-1' }

describe('sessionOpFailureKey', () => {
  it('says a slow disk is slow, whatever the operation', () => {
    expect(sessionOpFailureKey('reload', timedOut)).toBe('viewer.sessionOp.slow')
    expect(sessionOpFailureKey('tailOn', timedOut)).toBe('viewer.sessionOp.slow')
    expect(sessionOpFailureKey('encoding', timedOut)).toBe('viewer.sessionOp.slow')
  })

  it('tells a reload of a deleted file apart from one the OS refused', () => {
    expect(sessionOpFailureKey('reload', notFound)).toBe('viewer.reloadToast.gone')
    expect(sessionOpFailureKey('reload', io)).toBe('viewer.reloadToast.couldNotReload')
  })

  it('names the tail direction that did not take', () => {
    expect(sessionOpFailureKey('tailOn', noSession)).toBe('viewer.tail.couldNotTurnOn')
    expect(sessionOpFailureKey('tailOff', noSession)).toBe('viewer.tail.couldNotTurnOff')
  })

  it('reads an untyped failure as the operation’s generic copy', () => {
    expect(sessionOpFailureKey('reload', null)).toBe('viewer.reloadToast.couldNotReload')
    expect(sessionOpFailureKey('encoding', null)).toBe('viewer.encoding.couldNotSwitch')
  })
})

describe('logSessionOpFailure', () => {
  function fakeLog() {
    return { warn: vi.fn(), error: vi.fn() }
  }

  it('logs the world’s doing at warn, so a slow mount files no error report', () => {
    const log = fakeLog()
    logSessionOpFailure(log as unknown as Logger, 'reload', timedOut)
    logSessionOpFailure(log as unknown as Logger, 'reload', notFound)
    expect(log.warn).toHaveBeenCalledTimes(2)
    expect(log.error).not.toHaveBeenCalled()
  })

  it('logs a failure our own code caused, or an untyped one, at error', () => {
    const log = fakeLog()
    logSessionOpFailure(log as unknown as Logger, 'tailOn', noSession)
    logSessionOpFailure(log as unknown as Logger, 'encoding', null)
    expect(log.error).toHaveBeenCalledTimes(2)
  })
})
