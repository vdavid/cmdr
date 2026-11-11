/**
 * `openQueueWindow`: a window the OS refuses to create has to leave a log line.
 *
 * `new WebviewWindow(...)` doesn't throw on a refusal; it emits `tauri://error`
 * afterwards, so a surface that never listens for it fails silently.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const { getByLabel, once, warn, emitTo, webviewWindow, raiseChildWindow, scheduleShowFallback } = vi.hoisted(() => ({
  getByLabel: vi.fn<(label: string) => Promise<unknown>>(),
  scheduleShowFallback: vi.fn<(label: string) => void>(),
  raiseChildWindow: vi.fn<(label: string) => Promise<void>>(() => Promise.resolve()),
  once: vi.fn<(event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>>(),
  warn: vi.fn(),
  emitTo: vi.fn<(...args: unknown[]) => Promise<void>>(() => Promise.resolve()),
  webviewWindow: vi.fn<(label: string, options: { focus?: boolean; visible?: boolean }) => void>(),
}))

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  WebviewWindow: Object.assign(
    vi.fn(function constructWebviewWindow(label: string, options: { focus?: boolean }) {
      webviewWindow(label, options)
      return { once, setEffects: vi.fn(() => Promise.resolve()) }
    }),
    { getByLabel },
  ),
}))
vi.mock('@tauri-apps/api/dpi', () => ({ LogicalPosition: vi.fn() }))
vi.mock('@tauri-apps/api/event', () => ({ emitTo }))
vi.mock('@tauri-apps/api/window', () => ({ Effect: {}, EffectState: {} }))
// Reduce transparency on, so the only `once` the opener registers is the one under test.
vi.mock('#lib/tauri-commands/index.js', () => ({
  getShouldReduceTransparency: () => Promise.resolve(true),
  raiseChildWindow,
}))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn, info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))
vi.mock('#lib/intl/messages.svelte.js', () => ({ tString: (key: string) => key }))
vi.mock('#lib/text-size.svelte.js', () => ({ getEffectiveScale: () => 1 }))
vi.mock('#lib/app-mode.js', () => ({
  decorateChildWindowTitle: (title: string) => title,
  isE2eRun: () => false,
  orderChildWindowToBackInE2e: () => Promise.resolve(),
}))
vi.mock('#lib/window-positioning.js', () => ({
  readMainRect: () => Promise.resolve(null),
  readMonitors: () => Promise.resolve([]),
  readSavedRect: () => Promise.resolve(null),
  resolveChildPosition: vi.fn(),
}))
vi.mock('#lib/child-window-show.js', () => ({ scheduleShowFallback }))

import { openQueueWindow } from './queue-window'

beforeEach(() => {
  vi.clearAllMocks()
  getByLabel.mockResolvedValue(null)
  once.mockResolvedValue(() => {})
})

describe('openQueueWindow', () => {
  it('creates the window hidden, for its page to slide in, with a fallback show', async () => {
    await openQueueWindow()

    expect(webviewWindow).toHaveBeenCalledWith('queue', expect.objectContaining({ visible: false }))
    expect(scheduleShowFallback).toHaveBeenCalledWith('queue')
  })

  it('logs a window the OS refused to create, rather than failing silently', async () => {
    await openQueueWindow()

    const onError = once.mock.calls.find(([event]) => event === 'tauri://error')?.[1]
    expect(onError).toBeDefined()
    onError?.({ payload: 'a webview with label `queue` already exists' })
    expect(warn).toHaveBeenCalledTimes(1)
  })
})

/**
 * Every opener is a person asking to look (the menu command, the corner chip,
 * a toast's "Show in operation queue"), so the window takes focus. Backgrounding
 * a job never opens it at all.
 */
describe('openQueueWindow focus', () => {
  it('focuses a newly created window', async () => {
    await openQueueWindow()

    expect(webviewWindow).toHaveBeenCalledWith('queue', expect.objectContaining({ focus: true }))
  })

  // Regression anchor (ERR-APXNK): a minimized queue window ignored the old
  // `focus-self` event, because `setFocus()` is a no-op on a minimized or hidden
  // window, so every later click on the corner chip silently did nothing.
  it('has the backend raise an already-open window, minimized or not', async () => {
    getByLabel.mockResolvedValue({})

    await openQueueWindow()

    expect(raiseChildWindow).toHaveBeenCalledExactlyOnceWith('queue')
    expect(emitTo).not.toHaveBeenCalled()
    expect(webviewWindow).not.toHaveBeenCalled()
  })

  it('logs a raise the backend refused, rather than failing silently', async () => {
    getByLabel.mockResolvedValue({})
    raiseChildWindow.mockRejectedValueOnce(new Error('no main thread'))

    await openQueueWindow()

    expect(warn).toHaveBeenCalledTimes(1)
  })
})
