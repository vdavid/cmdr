/**
 * `openQueueWindow`: a window the OS refuses to create has to leave a log line.
 *
 * `new WebviewWindow(...)` doesn't throw on a refusal; it emits `tauri://error`
 * afterwards, so a surface that never listens for it fails silently.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const { getByLabel, once, warn, emitTo, webviewWindow } = vi.hoisted(() => ({
  getByLabel: vi.fn<(label: string) => Promise<unknown>>(),
  once: vi.fn<(event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>>(),
  warn: vi.fn(),
  emitTo: vi.fn<(...args: unknown[]) => Promise<void>>(() => Promise.resolve()),
  webviewWindow: vi.fn<(label: string, options: { focus?: boolean }) => void>(),
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
vi.mock('$lib/tauri-commands', () => ({ getShouldReduceTransparency: () => Promise.resolve(true) }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn, info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))
vi.mock('$lib/intl/messages.svelte', () => ({ tString: (key: string) => key }))
vi.mock('$lib/text-size.svelte', () => ({ getEffectiveScale: () => 1 }))
vi.mock('$lib/app-mode', () => ({
  decorateChildWindowTitle: (title: string) => title,
  isE2eRun: () => false,
  orderChildWindowToBackInE2e: () => Promise.resolve(),
}))
vi.mock('$lib/window-positioning', () => ({
  readMainRect: () => Promise.resolve(null),
  readMonitors: () => Promise.resolve([]),
  readSavedRect: () => Promise.resolve(null),
  resolveChildPosition: vi.fn(),
}))

import { openQueueWindow } from './queue-window'

beforeEach(() => {
  vi.clearAllMocks()
  getByLabel.mockResolvedValue(null)
  once.mockResolvedValue(() => {})
})

describe('openQueueWindow', () => {
  it('logs a window the OS refused to create, rather than failing silently', async () => {
    await openQueueWindow()

    const onError = once.mock.calls.find(([event]) => event === 'tauri://error')?.[1]
    expect(onError).toBeDefined()
    onError?.({ payload: 'a webview with label `queue` already exists' })
    expect(warn).toHaveBeenCalledTimes(1)
  })
})

/**
 * Backgrounding a transfer shows the queue window WITHOUT taking focus: the
 * person sent the job away to keep working in the main window, and a window that
 * grabs the keyboard makes them click back. The menu command and the corner chip
 * still open it focused, because there the person asked to look at it.
 */
describe('openQueueWindow focus', () => {
  it('focuses a newly created window by default', async () => {
    await openQueueWindow()

    expect(webviewWindow).toHaveBeenCalledWith('queue', expect.objectContaining({ focus: true }))
  })

  it('asks an already-open window to focus itself by default', async () => {
    getByLabel.mockResolvedValue({})

    await openQueueWindow()

    expect(emitTo).toHaveBeenCalledWith('queue', 'focus-self')
  })

  it('creates the window unfocused when the caller keeps focus', async () => {
    await openQueueWindow({ focus: false })

    expect(webviewWindow).toHaveBeenCalledWith('queue', expect.objectContaining({ focus: false }))
  })

  it('leaves an already-open window where it is when the caller keeps focus', async () => {
    getByLabel.mockResolvedValue({})

    await openQueueWindow({ focus: false })

    expect(emitTo).not.toHaveBeenCalled()
    expect(webviewWindow).not.toHaveBeenCalled()
  })
})
