import { describe, it, expect, vi, beforeEach } from 'vitest'
import { closeSelfWindow } from './child-window-close'

const closeChildWindow = vi.hoisted(() => vi.fn<(label: string) => Promise<void>>())
const logError = vi.hoisted(() => vi.fn())

vi.mock('$lib/tauri-commands', () => ({ closeChildWindow }))
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ label: 'settings' }) }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ error: logError, debug: vi.fn(), warn: vi.fn(), info: vi.fn() }),
}))

describe('closeSelfWindow', () => {
  beforeEach(() => {
    closeChildWindow.mockReset()
    closeChildWindow.mockResolvedValue(undefined)
    logError.mockReset()
  })

  it('asks the backend to close THIS window, by its own label', async () => {
    await closeSelfWindow()

    expect(closeChildWindow).toHaveBeenCalledExactlyOnceWith('settings')
  })

  it('never reaches for the window handle to close it from here', async () => {
    // The whole point of the module. A `close()` from inside the webview being destroyed stalls
    // queued IPC on webkit2gtk and can segfault the app on macOS WebKit, and a JS timer can't be
    // trusted to fix it either: WebKit throttles timers in a hidden page to roughly 1 Hz, so a
    // window that just hid itself might not get round to closing for a second or more.
    const { getCurrentWindow } = await import('@tauri-apps/api/window')

    await closeSelfWindow()

    expect(getCurrentWindow()).not.toHaveProperty('close')
  })

  it('reports a refusal instead of leaving a half-closed window behind', async () => {
    closeChildWindow.mockRejectedValueOnce(new Error('no such window'))

    await expect(closeSelfWindow()).resolves.toBeUndefined()

    expect(logError).toHaveBeenCalledOnce()
  })
})
