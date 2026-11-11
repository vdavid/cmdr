import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { scheduleShowFallback, showSelfWindow } from './child-window-show'

const showChildWindow = vi.hoisted(() => vi.fn<(label: string) => Promise<void>>())
const logWarn = vi.hoisted(() => vi.fn())

vi.mock('#lib/tauri-commands/index.js', () => ({ showChildWindow }))
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ label: 'settings' }) }))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ error: vi.fn(), debug: vi.fn(), warn: logWarn, info: vi.fn() }),
}))

/** Lets the rejected show's `catch` run. */
const settle = () => vi.advanceTimersByTimeAsync(0)

describe('child window show', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    showChildWindow.mockReset()
    showChildWindow.mockResolvedValue(undefined)
    logWarn.mockReset()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  describe('showSelfWindow', () => {
    it('asks the backend to show THIS window, by its own label', () => {
      showSelfWindow()

      expect(showChildWindow).toHaveBeenCalledExactlyOnceWith('settings')
    })

    it('logs a refusal instead of throwing into the page mount', async () => {
      showChildWindow.mockRejectedValueOnce(new Error('no such window'))

      expect(() => {
        showSelfWindow()
      }).not.toThrow()
      await settle()

      expect(logWarn).toHaveBeenCalledOnce()
    })
  })

  describe('scheduleShowFallback', () => {
    it('leaves the page two seconds to show its own window first', async () => {
      scheduleShowFallback('queue')

      await vi.advanceTimersByTimeAsync(1999)
      expect(showChildWindow).not.toHaveBeenCalled()

      await vi.advanceTimersByTimeAsync(1)
      expect(showChildWindow).toHaveBeenCalledExactlyOnceWith('queue')
    })

    it('logs a refusal, since the opener has nothing better to show', async () => {
      showChildWindow.mockRejectedValueOnce(new Error('no such window'))

      scheduleShowFallback('queue')
      await vi.advanceTimersByTimeAsync(2000)
      await settle()

      expect(logWarn).toHaveBeenCalledOnce()
    })
  })
})
