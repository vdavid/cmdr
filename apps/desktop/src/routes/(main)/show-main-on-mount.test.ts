import { describe, it, expect, vi, beforeEach } from 'vitest'

const { showMainWindow, waitForNextPaint, warn, debug } = vi.hoisted(() => ({
  showMainWindow: vi.fn(async () => {}),
  waitForNextPaint: vi.fn(),
  warn: vi.fn(),
  debug: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({ showMainWindow }))
vi.mock('$lib/utils/timing', () => ({ waitForNextPaint }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn, debug, info: vi.fn(), error: vi.fn() }),
}))

import { showMainOnMount } from './show-main-on-mount'

beforeEach(() => {
  vi.clearAllMocks()
})

describe('showMainOnMount', () => {
  it('shows the window without waiting for a paint first', async () => {
    // Pre-fix this would have failed: the old version awaited `waitForNextPaint`
    // before showing, which in a hidden window always burned the full timeout.
    let resolvePaint: (v: 'painted') => void = () => {}
    waitForNextPaint.mockReturnValue(
      new Promise<'painted'>((r) => {
        resolvePaint = r
      }),
    )

    const done = showMainOnMount()
    await Promise.resolve()
    await Promise.resolve()
    expect(showMainWindow).toHaveBeenCalledExactlyOnceWith('launch')

    resolvePaint('painted')
    await done
  })

  it('logs debug and does not re-show when a frame lands', async () => {
    waitForNextPaint.mockResolvedValue('painted')
    await showMainOnMount()
    expect(showMainWindow).toHaveBeenCalledExactlyOnceWith('launch')
    expect(debug).toHaveBeenCalledOnce()
    expect(warn).not.toHaveBeenCalled()
  })

  it('re-shows to force a repaint when no frame lands', async () => {
    waitForNextPaint.mockResolvedValue('timeout')
    await showMainOnMount()
    expect(showMainWindow).toHaveBeenCalledTimes(2)
    expect(warn).toHaveBeenCalledOnce()
    expect(debug).not.toHaveBeenCalled()
  })

  it('asks the repair re-show not to take the front back', async () => {
    // The launch show activates Cmdr; the repair one must not, or a user who switched apps in
    // the second it waited gets yanked back.
    waitForNextPaint.mockResolvedValue('timeout')
    await showMainOnMount()
    expect(showMainWindow).toHaveBeenNthCalledWith(1, 'launch')
    expect(showMainWindow).toHaveBeenNthCalledWith(2, 'repaint-repair')
  })
})
