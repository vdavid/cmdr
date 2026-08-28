/**
 * Tests for the uncaught-error forwarder.
 *
 * The point of the module is that a frontend crash becomes a log line, so these
 * assert the two window events reach `log.error`, that a resource-load `error`
 * event does NOT (it isn't a crash), and that neither listener swallows the event.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

const h = vi.hoisted(() => ({ error: vi.fn() }))

vi.mock('./logger', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: h.error }),
}))

import { registerUncaughtErrorLogging } from './uncaught-errors'

// The module registers once per process, so register here and let every case
// dispatch into the same listeners — which also proves the idempotence guard
// doesn't stop the FIRST registration from working.
registerUncaughtErrorLogging()

beforeEach(() => {
  h.error.mockClear()
})

describe('uncaught error logging', () => {
  it('logs an uncaught error with its origin and stack', () => {
    const boom = new Error('kaboom')
    boom.stack = 'Error: kaboom\n    at somewhere'
    window.dispatchEvent(
      new ErrorEvent('error', { error: boom, message: 'kaboom', filename: 'app.js', lineno: 12, colno: 3 }),
    )

    expect(h.error).toHaveBeenCalledTimes(1)
    const [, ctx] = h.error.mock.calls[0] as [string, { source: string; detail: string }]
    expect(ctx.source).toBe('app.js:12:3')
    expect(ctx.detail).toContain('kaboom')
  })

  it('logs an unhandled rejection', () => {
    // jsdom doesn't construct PromiseRejectionEvent, so dispatch the shape the
    // listener reads. A real rejection carries the same two fields.
    const event = new Event('unhandledrejection') as Event & { reason: unknown }
    event.reason = new Error('promise went bad')
    window.dispatchEvent(event)

    expect(h.error).toHaveBeenCalledTimes(1)
    const [, ctx] = h.error.mock.calls[0] as [string, { detail: string }]
    expect(ctx.detail).toContain('promise went bad')
  })

  it('ignores a resource-load error, which is not a crash', () => {
    // A failed `<img>` / `<script>` load fires `error` with no `error` and no message.
    window.dispatchEvent(new ErrorEvent('error', { message: '' }))

    expect(h.error).not.toHaveBeenCalled()
  })

  it('does not swallow the event, so browser reporting and hmr-recovery still see it', () => {
    const event = new ErrorEvent('error', { error: new Error('x'), message: 'x', cancelable: true })
    window.dispatchEvent(event)

    expect(h.error).toHaveBeenCalledTimes(1)
    expect(event.defaultPrevented).toBe(false)
  })

  it('registers only once, so an HMR re-import cannot double-log', () => {
    registerUncaughtErrorLogging()
    registerUncaughtErrorLogging()

    window.dispatchEvent(new ErrorEvent('error', { error: new Error('once'), message: 'once' }))

    expect(h.error).toHaveBeenCalledTimes(1)
  })
})
