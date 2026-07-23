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

import { showMainWhenPainted } from './show-main-when-painted'

beforeEach(() => {
  vi.clearAllMocks()
})

describe('showMainWhenPainted', () => {
  it('shows the window and logs debug (no warn) when the first paint is confirmed', async () => {
    waitForNextPaint.mockResolvedValue('painted')
    await showMainWhenPainted()
    expect(showMainWindow).toHaveBeenCalledOnce()
    expect(debug).toHaveBeenCalledOnce()
    expect(warn).not.toHaveBeenCalled()
  })

  it('shows the window and warns when the first paint times out', async () => {
    waitForNextPaint.mockResolvedValue('timeout')
    await showMainWhenPainted()
    expect(showMainWindow).toHaveBeenCalledOnce()
    expect(warn).toHaveBeenCalledOnce()
    expect(debug).not.toHaveBeenCalled()
  })

  it('waits for the paint result before showing the window', async () => {
    let resolvePaint: (v: 'painted') => void = () => {}
    waitForNextPaint.mockReturnValue(
      new Promise<'painted'>((r) => {
        resolvePaint = r
      }),
    )
    const done = showMainWhenPainted()
    await Promise.resolve() // let any premature show() slip through
    expect(showMainWindow).not.toHaveBeenCalled()
    resolvePaint('painted')
    await done
    expect(showMainWindow).toHaveBeenCalledOnce()
  })
})
