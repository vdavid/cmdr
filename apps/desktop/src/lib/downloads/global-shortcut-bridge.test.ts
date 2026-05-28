import { describe, it, expect, vi, beforeEach } from 'vitest'

const {
  listenMock,
  revealLatestDownloadMock,
  setGlobalRevealShortcutMock,
  addToastMock,
  dismissToastMock,
  getSettingMock,
  setSettingMock,
} = vi.hoisted(() => ({
  listenMock: vi.fn(),
  revealLatestDownloadMock: vi.fn(),
  setGlobalRevealShortcutMock: vi.fn(),
  addToastMock: vi.fn<(content: unknown, options?: Record<string, unknown>) => string>(() => 'toast-id'),
  dismissToastMock: vi.fn(),
  getSettingMock: vi.fn(),
  setSettingMock: vi.fn(),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: listenMock,
}))

vi.mock('./reveal', () => ({
  revealLatestDownload: revealLatestDownloadMock,
}))

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    setGlobalRevealShortcut: setGlobalRevealShortcutMock,
  },
}))

vi.mock('$lib/ui/toast', () => ({
  addToast: addToastMock,
  dismissToast: dismissToastMock,
}))

vi.mock('$lib/settings', () => ({
  getSetting: getSettingMock,
  setSetting: setSettingMock,
}))

vi.mock('./GlobalShortcutWarnToastContent.svelte', () => ({
  default: { __toastContent: 'GlobalShortcutWarnToastContent' },
  setWarnToastHandlers: vi.fn(),
}))

import { startGlobalShortcutBridge, GLOBAL_SHORTCUT_FIRED_EVENT } from './global-shortcut-bridge.svelte'
import GlobalShortcutWarnToastContent from './GlobalShortcutWarnToastContent.svelte'

interface FakeEvent {
  payload: unknown
}

async function mountBridgeAndCapturePayloadHandler(): Promise<(payload?: unknown) => Promise<void>> {
  let handler: ((event: FakeEvent) => void) | undefined
  listenMock.mockImplementation((eventName: string, cb: (event: FakeEvent) => void) => {
    if (eventName === GLOBAL_SHORTCUT_FIRED_EVENT) handler = cb
    return Promise.resolve(() => {})
  })
  await startGlobalShortcutBridge(undefined)
  if (!handler) throw new Error('bridge did not subscribe to ' + GLOBAL_SHORTCUT_FIRED_EVENT)
  const capturedHandler = handler
  return async (payload: unknown = {}) => {
    capturedHandler({ payload })
    // Let any awaited internal microtasks settle before assertions read state.
    await new Promise((r) => setTimeout(r, 0))
  }
}

describe('startGlobalShortcutBridge', () => {
  beforeEach(() => {
    listenMock.mockReset()
    revealLatestDownloadMock.mockReset().mockResolvedValue(undefined)
    setGlobalRevealShortcutMock.mockReset().mockResolvedValue({ status: 'ok', data: null })
    addToastMock.mockReset().mockReturnValue('toast-id')
    dismissToastMock.mockReset()
    getSettingMock.mockReset()
    setSettingMock.mockReset()
  })

  it('calls revealLatestDownload on every global-shortcut-fired event', async () => {
    getSettingMock.mockImplementation((id: string) => {
      if (id === 'behavior.fileSystemWatching.globalRevealShortcut.acknowledged') return true
      if (id === 'behavior.fileSystemWatching.globalRevealShortcut.binding') return '⌃⌥⌘J'
      return undefined
    })
    const fire = await mountBridgeAndCapturePayloadHandler()
    await fire()
    expect(revealLatestDownloadMock).toHaveBeenCalledTimes(1)
  })

  it('fires the warn toast and flips acknowledged=true when acknowledged is false', async () => {
    getSettingMock.mockImplementation((id: string) => {
      if (id === 'behavior.fileSystemWatching.globalRevealShortcut.acknowledged') return false
      if (id === 'behavior.fileSystemWatching.globalRevealShortcut.binding') return '⌃⌥⌘J'
      return undefined
    })
    const fire = await mountBridgeAndCapturePayloadHandler()
    await fire()

    expect(addToastMock).toHaveBeenCalledTimes(1)
    const [content, options] = addToastMock.mock.calls[0]
    expect(content).toBe(GlobalShortcutWarnToastContent)
    expect(options?.level).toBe('warn')
    expect(options?.dismissal).toBe('persistent')

    expect(setSettingMock).toHaveBeenCalledWith('behavior.fileSystemWatching.globalRevealShortcut.acknowledged', true)
    expect(revealLatestDownloadMock).toHaveBeenCalledTimes(1)
  })

  it('does NOT fire the warn toast when acknowledged is already true', async () => {
    getSettingMock.mockImplementation((id: string) => {
      if (id === 'behavior.fileSystemWatching.globalRevealShortcut.acknowledged') return true
      if (id === 'behavior.fileSystemWatching.globalRevealShortcut.binding') return '⌃⌥⌘J'
      return undefined
    })
    const fire = await mountBridgeAndCapturePayloadHandler()
    await fire()

    expect(addToastMock).not.toHaveBeenCalled()
    expect(setSettingMock).not.toHaveBeenCalledWith(
      'behavior.fileSystemWatching.globalRevealShortcut.acknowledged',
      true,
    )
    expect(revealLatestDownloadMock).toHaveBeenCalledTimes(1)
  })
})
