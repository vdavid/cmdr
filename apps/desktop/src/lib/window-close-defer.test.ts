import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { WINDOW_CLOSE_DEFER_MS, deferWindowClose } from './window-close-defer'

describe('deferWindowClose', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('does not close synchronously', () => {
    const close = vi.fn()
    deferWindowClose(close)
    // The whole point: a webview must not destroy itself from inside the
    // handler that asked it to (GTK IPC stall + macOS WebKit teardown crash).
    expect(close).not.toHaveBeenCalled()
  })

  it('does not close before the full delay has elapsed', () => {
    const close = vi.fn()
    deferWindowClose(close)
    vi.advanceTimersByTime(WINDOW_CLOSE_DEFER_MS - 1)
    expect(close).not.toHaveBeenCalled()
  })

  it('closes once the delay elapses', () => {
    const close = vi.fn()
    deferWindowClose(close)
    vi.advanceTimersByTime(WINDOW_CLOSE_DEFER_MS)
    expect(close).toHaveBeenCalledTimes(1)
  })

  it('defaults to a real delay, not a next-tick defer', () => {
    // Pre-fix this was `setTimeout(…, 0)`, which covers the Linux GTK stall
    // but still let macOS WebKit segfault the app mid-teardown.
    expect(WINDOW_CLOSE_DEFER_MS).toBeGreaterThan(0)
  })

  it('can be cancelled before it fires', () => {
    const close = vi.fn()
    clearTimeout(deferWindowClose(close))
    vi.advanceTimersByTime(WINDOW_CLOSE_DEFER_MS * 2)
    expect(close).not.toHaveBeenCalled()
  })
})
