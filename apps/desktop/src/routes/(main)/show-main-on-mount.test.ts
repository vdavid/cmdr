import { describe, it, expect, vi, beforeEach } from 'vitest'

const { showMainWindow, waitForNextPaint, warn, info, debug } = vi.hoisted(() => ({
  showMainWindow: vi.fn(async () => {}),
  waitForNextPaint: vi.fn(),
  warn: vi.fn(),
  info: vi.fn(),
  debug: vi.fn(),
}))

vi.mock('#lib/tauri-commands/index.js', () => ({ showMainWindow }))
vi.mock('#lib/utils/timing.js', () => ({ waitForNextPaint }))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn, debug, info, error: vi.fn() }),
}))

import { showMainOnMount } from './show-main-on-mount'

beforeEach(() => {
  // Reset, not clear: a leftover `mockResolvedValueOnce` would leak into the next test.
  vi.resetAllMocks()
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

  it('re-shows, then logs a late frame at info, not warn', async () => {
    // Regression anchor: a slow Mac can miss the first deadline while both panes mount, then
    // paint. A late frame isn't a stuck window, so it must not warn, or a real stuck window
    // drowns in the noise.
    waitForNextPaint.mockResolvedValueOnce('timeout').mockResolvedValueOnce('painted')
    await showMainOnMount()
    expect(showMainWindow).toHaveBeenCalledTimes(2)
    expect(info).toHaveBeenCalledOnce()
    expect(warn).not.toHaveBeenCalled()
  })

  it('warns only when no frame lands even after the repair', async () => {
    waitForNextPaint.mockResolvedValue('timeout')
    await showMainOnMount()
    expect(showMainWindow).toHaveBeenCalledTimes(2)
    expect(warn).toHaveBeenCalledOnce()
    expect(info).not.toHaveBeenCalled()
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
