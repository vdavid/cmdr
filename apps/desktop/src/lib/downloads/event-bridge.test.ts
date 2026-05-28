import { describe, it, expect, vi, beforeEach } from 'vitest'

/**
 * The bridge mounts one `download-detected` listener and dispatches to the
 * in-app toast and/or the macOS native notification based on the settings
 * enum. These tests pump a single listener callback (the one the bridge
 * registers) and assert the resulting calls.
 */

type DetectedListener = (ev: { payload: DownloadDetectedPayload }) => void
interface DownloadDetectedPayload {
  path: string
  parentDir: string
  fileName: string
  observedAtMs: number
  inSubdir: boolean
  sizeBytes: number | null
}

const {
  listenMock,
  getDownloadsNotificationsModeMock,
  isPermissionGrantedMock,
  requestPermissionMock,
  sendNotificationMock,
  addToastMock,
  getEffectiveShortcutsMock,
  downloadsWatcherStatusMock,
} = vi.hoisted(() => ({
  listenMock: vi.fn(),
  getDownloadsNotificationsModeMock: vi.fn<() => 'in-app' | 'macos' | 'both' | 'neither'>(),
  isPermissionGrantedMock: vi.fn<() => Promise<boolean>>(),
  requestPermissionMock: vi.fn<() => Promise<'granted' | 'denied' | 'default'>>(),
  sendNotificationMock: vi.fn(),
  addToastMock: vi.fn(() => 'toast-id'),
  getEffectiveShortcutsMock: vi.fn<(id: string) => string[]>(),
  downloadsWatcherStatusMock: vi.fn(),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: listenMock,
}))

vi.mock('@tauri-apps/plugin-notification', () => ({
  isPermissionGranted: isPermissionGrantedMock,
  requestPermission: requestPermissionMock,
  sendNotification: sendNotificationMock,
}))

vi.mock('./notifications-mode', () => ({
  getDownloadsNotificationsMode: getDownloadsNotificationsModeMock,
  // openSettingsToDownloadsNotifications + setDownloadsNotificationsMode are
  // not used by the bridge itself; left out on purpose.
}))

vi.mock('$lib/ui/toast', () => ({
  addToast: addToastMock,
}))

vi.mock('$lib/shortcuts', () => ({
  getEffectiveShortcuts: getEffectiveShortcutsMock,
}))

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    downloadsWatcherStatus: downloadsWatcherStatusMock,
  },
}))

import { startDownloadsEventBridge, __resetPermissionCacheForTests } from './event-bridge.svelte'

/**
 * Wait until every queued microtask + promise chain has settled. Each
 * `Promise.resolve()` yields one microtask tick; we yield generously so the
 * bridge's `await commands.downloadsWatcherStatus()` chain finishes before
 * we assert.
 */
async function flushAsync(): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await Promise.resolve()
  }
}

function payload(overrides: Partial<DownloadDetectedPayload> = {}): DownloadDetectedPayload {
  return {
    path: '/Users/me/Downloads/report.pdf',
    parentDir: '/Users/me/Downloads',
    fileName: 'report.pdf',
    observedAtMs: 1_700_000_000_000,
    inSubdir: false,
    sizeBytes: 1024,
    ...overrides,
  }
}

async function startBridgeAndCaptureListener(): Promise<DetectedListener> {
  let captured: DetectedListener | null = null
  listenMock.mockImplementation((_event: string, handler: DetectedListener) => {
    captured = handler
    return Promise.resolve(() => {})
  })
  await startDownloadsEventBridge(undefined)
  if (!captured) throw new Error('Bridge did not register a listener')
  return captured
}

describe('startDownloadsEventBridge', () => {
  beforeEach(() => {
    listenMock.mockReset()
    getDownloadsNotificationsModeMock.mockReset().mockReturnValue('in-app')
    isPermissionGrantedMock.mockReset().mockResolvedValue(true)
    requestPermissionMock.mockReset().mockResolvedValue('granted')
    sendNotificationMock.mockReset()
    addToastMock.mockReset().mockReturnValue('toast-id')
    getEffectiveShortcutsMock.mockReset().mockReturnValue(['⌘J'])
    downloadsWatcherStatusMock.mockReset().mockResolvedValue({
      status: 'ok',
      data: { running: true, downloadsDir: '/Users/me/Downloads', fdaPending: false, lastDetected: null },
    })
    __resetPermissionCacheForTests()
  })

  it('mode "in-app" dispatches an in-app toast only', async () => {
    getDownloadsNotificationsModeMock.mockReturnValue('in-app')
    const listener = await startBridgeAndCaptureListener()
    listener({ payload: payload() })
    await flushAsync()

    expect(addToastMock).toHaveBeenCalledTimes(1)
    const [, options] = addToastMock.mock.calls[0] as unknown as [unknown, Record<string, unknown>]
    expect(options).toMatchObject({
      toastGroup: 'downloads',
      level: 'info',
      timeoutMs: 10_000,
    })
    expect(sendNotificationMock).not.toHaveBeenCalled()
  })

  it('mode "macos" sends a native notification only, with no shortcut hint in the body', async () => {
    getDownloadsNotificationsModeMock.mockReturnValue('macos')
    const listener = await startBridgeAndCaptureListener()
    listener({ payload: payload({ inSubdir: false }) })
    await flushAsync()

    expect(addToastMock).not.toHaveBeenCalled()
    expect(sendNotificationMock).toHaveBeenCalledTimes(1)
    const [arg] = sendNotificationMock.mock.calls[0] as unknown as [{ title: string; body?: string }]
    expect(arg.title).toContain('report.pdf')
    // The hint is intentionally absent from the OS notification.
    expect(arg.body ?? '').not.toContain('⌘')
  })

  it('mode "both" fires the toast AND the native notification', async () => {
    getDownloadsNotificationsModeMock.mockReturnValue('both')
    const listener = await startBridgeAndCaptureListener()
    listener({ payload: payload() })
    await flushAsync()

    expect(addToastMock).toHaveBeenCalledTimes(1)
    expect(sendNotificationMock).toHaveBeenCalledTimes(1)
  })

  it('mode "neither" does nothing', async () => {
    getDownloadsNotificationsModeMock.mockReturnValue('neither')
    const listener = await startBridgeAndCaptureListener()
    listener({ payload: payload() })
    await flushAsync()

    expect(addToastMock).not.toHaveBeenCalled()
    expect(sendNotificationMock).not.toHaveBeenCalled()
  })

  it('subdir payload renders the body as "in Downloads/<subdir>/"', async () => {
    getDownloadsNotificationsModeMock.mockReturnValue('macos')
    const listener = await startBridgeAndCaptureListener()
    listener({
      payload: payload({
        parentDir: '/Users/me/Downloads/Chrome',
        path: '/Users/me/Downloads/Chrome/setup.dmg',
        fileName: 'setup.dmg',
        inSubdir: true,
      }),
    })
    await flushAsync()

    const [arg] = sendNotificationMock.mock.calls[0] as unknown as [{ title: string; body?: string }]
    expect(arg.body).toContain('Chrome')
  })

  it('snapshots the shortcut hint at toast creation time (not reactive)', async () => {
    getDownloadsNotificationsModeMock.mockReturnValue('in-app')
    getEffectiveShortcutsMock.mockReturnValue(['⌘J'])
    const listener = await startBridgeAndCaptureListener()
    listener({ payload: payload() })
    await flushAsync()

    // Simulate the user remapping AFTER the toast was created. The already-
    // emitted toast must not change its hint.
    getEffectiveShortcutsMock.mockReturnValue(['⌘K'])

    const firstCall = addToastMock.mock.calls[0] as unknown as [unknown, { props?: { shortcutHint?: string } }]
    expect(firstCall[1]?.props?.shortcutHint).toBe('⌘J')
  })

  it('skips notification dispatch entirely while the FDA gate is pending', async () => {
    // Defense in depth: the watcher won't fire under a closed gate, but if a
    // stale event leaks through we must not surface a toast or a macOS popup
    // before the gate clears.
    downloadsWatcherStatusMock.mockResolvedValue({
      status: 'ok',
      data: { running: false, downloadsDir: null, fdaPending: true, lastDetected: null },
    })
    getDownloadsNotificationsModeMock.mockReturnValue('both')
    const listener = await startBridgeAndCaptureListener()
    listener({ payload: payload() })
    await flushAsync()

    expect(addToastMock).not.toHaveBeenCalled()
    expect(sendNotificationMock).not.toHaveBeenCalled()
  })
})

describe('startDownloadsEventBridge — permission flow', () => {
  beforeEach(() => {
    listenMock.mockReset()
    getDownloadsNotificationsModeMock.mockReset().mockReturnValue('macos')
    isPermissionGrantedMock.mockReset()
    requestPermissionMock.mockReset()
    sendNotificationMock.mockReset()
    addToastMock.mockReset().mockReturnValue('toast-id')
    getEffectiveShortcutsMock.mockReset().mockReturnValue(['⌘J'])
    downloadsWatcherStatusMock.mockReset().mockResolvedValue({
      status: 'ok',
      data: { running: true, downloadsDir: '/Users/me/Downloads', fdaPending: false, lastDetected: null },
    })
    __resetPermissionCacheForTests()
  })

  it("asks for OS permission once when it isn't already granted, then fires the notification", async () => {
    isPermissionGrantedMock.mockResolvedValue(false)
    requestPermissionMock.mockResolvedValue('granted')

    const listener = await startBridgeAndCaptureListener()
    listener({ payload: { ...payload() } })
    // Two microtask flushes for the chained awaits.
    await flushAsync()
    await Promise.resolve()

    expect(requestPermissionMock).toHaveBeenCalledTimes(1)
    expect(sendNotificationMock).toHaveBeenCalledTimes(1)
  })

  it('shows an INFO toast and does not fire when the user denies the OS prompt', async () => {
    isPermissionGrantedMock.mockResolvedValue(false)
    requestPermissionMock.mockResolvedValue('denied')

    const listener = await startBridgeAndCaptureListener()
    listener({ payload: { ...payload() } })
    await flushAsync()
    await Promise.resolve()

    expect(sendNotificationMock).not.toHaveBeenCalled()
    // One INFO toast surfaces with the dedup id.
    expect(addToastMock).toHaveBeenCalled()
    const calls = addToastMock.mock.calls as unknown as [unknown, Record<string, unknown>][]
    const hasInfoToast = calls.some(([, options]) => options.level === 'info')
    expect(hasInfoToast).toBe(true)
  })
})
