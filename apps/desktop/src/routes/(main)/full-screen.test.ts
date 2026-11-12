/**
 * The full-screen toggle drives the window's own full screen, the same one `escape-key.ts` leaves
 * and `window_state` saves, whichever road entered it.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'

const windowMock = vi.hoisted(() => ({
  isFullscreen: vi.fn(() => Promise.resolve(false)),
  setFullscreen: vi.fn(() => Promise.resolve()),
}))
const warn = vi.hoisted(() => vi.fn())

vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => windowMock }))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ info: vi.fn(), debug: vi.fn(), warn, error: vi.fn() }),
}))

import { toggleFullScreen } from './full-screen'

describe('toggleFullScreen', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('enters full screen from a normal window', async () => {
    windowMock.isFullscreen.mockResolvedValueOnce(false)
    await toggleFullScreen()
    expect(windowMock.setFullscreen).toHaveBeenCalledWith(true)
  })

  it('leaves full screen from a full-screen window', async () => {
    windowMock.isFullscreen.mockResolvedValueOnce(true)
    await toggleFullScreen()
    expect(windowMock.setFullscreen).toHaveBeenCalledWith(false)
  })

  it('logs and swallows a window that refuses, so the keypress never throws', async () => {
    windowMock.setFullscreen.mockRejectedValueOnce(new Error('no permission'))
    await expect(toggleFullScreen()).resolves.toBeUndefined()
    expect(warn).toHaveBeenCalledOnce()
  })
})
