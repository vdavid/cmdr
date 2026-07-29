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
  getGlobalGoToLatestEnabledMock,
  getGlobalGoToLatestBindingMock,
  getDownloadsToastCollapsedMock,
  downloadsWatcherStatusMock,
} = vi.hoisted(() => ({
  listenMock: vi.fn(),
  getDownloadsNotificationsModeMock: vi.fn<() => 'in-app' | 'macos' | 'both' | 'neither'>(),
  isPermissionGrantedMock: vi.fn<() => Promise<boolean>>(),
  requestPermissionMock: vi.fn<() => Promise<'granted' | 'denied' | 'default'>>(),
  sendNotificationMock: vi.fn(),
  addToastMock: vi.fn(() => 'toast-id'),
  getEffectiveShortcutsMock: vi.fn<(id: string) => string[]>(),
  getGlobalGoToLatestEnabledMock: vi.fn<() => boolean>(),
  getGlobalGoToLatestBindingMock: vi.fn<() => string>(),
  getDownloadsToastCollapsedMock: vi.fn<() => boolean>(),
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

vi.mock('$lib/shortcuts', async () => ({
  getEffectiveShortcuts: getEffectiveShortcutsMock,
  // Real display formatter (`key-capture` is a dependency-free leaf), so the
  // asserted hint text is what a user would actually read.
  toDisplayShortcut: (await import('$lib/shortcuts/key-capture')).toDisplayShortcut,
}))

vi.mock('./global-shortcut-setting', () => ({
  getGlobalGoToLatestEnabled: getGlobalGoToLatestEnabledMock,
  getGlobalGoToLatestBinding: getGlobalGoToLatestBindingMock,
}))

vi.mock('./downloads-toast-collapsed', () => ({
  getDownloadsToastCollapsed: getDownloadsToastCollapsedMock,
}))

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    downloadsWatcherStatus: downloadsWatcherStatusMock,
  },
  // The bridge subscribes via the typed `onDownloadDetected` wrapper, which
  // calls `events.downloadDetected.listen`. Route it into `listenMock` under
  // the wire name so the existing capture closure still works.
  events: {
    downloadDetected: {
      listen: (cb: DetectedListener): Promise<() => void> => listenMock('download-detected', cb) as Promise<() => void>,
    },
  },
}))

import { startDownloadsEventBridge } from './event-bridge.svelte'
import { __resetPermissionCacheForTests } from '$lib/notifications/macos-notification-permission'

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

/**
 * Settle the macOS notification path: the bridge holds a burst for
 * `MACOS_COALESCE_MS` before sending one banner, so a test asserting on
 * `sendNotification` has to let that window elapse. Real timers, so the wait is
 * the actual window plus slack; the in-app toast path is synchronous and needs
 * only `flushAsync`.
 */
async function flushMacosBurst(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 600))
  await flushAsync()
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
  // `captured` is assigned inside the `listen` mock's closure, so TS's
  // control-flow analysis still sees its initialized `null` here. The runtime
  // check is the actual contract.
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
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
    getGlobalGoToLatestEnabledMock.mockReset().mockReturnValue(true)
    getGlobalGoToLatestBindingMock.mockReset().mockReturnValue('⌃⌥⌘J')
    getDownloadsToastCollapsedMock.mockReset().mockReturnValue(false)
    downloadsWatcherStatusMock.mockReset().mockResolvedValue({
      status: 'ok',
      data: { running: true, downloadsDir: '/Users/me/Downloads', fdaPending: false },
    })
    __resetPermissionCacheForTests()
  })

  it('mode "in-app" dispatches an in-app toast only', async () => {
    getDownloadsNotificationsModeMock.mockReturnValue('in-app')
    const listener = await startBridgeAndCaptureListener()
    listener({ payload: payload() })
    await flushAsync()

    expect(addToastMock).toHaveBeenCalledTimes(1)
    const [, options] = addToastMock.mock.calls[0] as unknown as [
      unknown,
      Record<string, unknown> & { props: { initialCollapsed: boolean } },
    ]
    expect(options).toMatchObject({
      toastGroup: 'downloads',
      // Only one downloads toast at a time: a new detection evicts the previous
      // one via the store's group cap, so the visible toast is always the newest
      // file.
      maxInGroup: 1,
      level: 'info',
      // Transient: auto-hides on a 10s timer, and wider than the default to fit
      // the keyboard animation.
      timeoutMs: 10_000,
      widthPx: 432,
    })
    // No persistent dismissal: it's a normal auto-hiding toast now.
    expect(options.dismissal).toBeUndefined()
    // The persisted collapse state seeds the toast.
    expect(options.props.initialCollapsed).toBe(false)
    expect(sendNotificationMock).not.toHaveBeenCalled()
  })

  it('mode "macos" sends a native notification only, with no shortcut hint in the body', async () => {
    getDownloadsNotificationsModeMock.mockReturnValue('macos')
    const listener = await startBridgeAndCaptureListener()
    listener({ payload: payload({ inSubdir: false }) })
    await flushMacosBurst()

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
    await flushMacosBurst()

    expect(addToastMock).toHaveBeenCalledTimes(1)
    expect(sendNotificationMock).toHaveBeenCalledTimes(1)
  })

  it('mode "neither" does nothing', async () => {
    getDownloadsNotificationsModeMock.mockReturnValue('neither')
    const listener = await startBridgeAndCaptureListener()
    listener({ payload: payload() })
    await flushMacosBurst()

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
    await flushMacosBurst()

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

    const firstCall = addToastMock.mock.calls[0] as unknown as [unknown, { props: { shortcutHint: string } }]
    expect(firstCall[1].props.shortcutHint).toBe('⌘J')
  })

  it('passes the global hotkey binding when it is enabled and bound', async () => {
    getGlobalGoToLatestEnabledMock.mockReturnValue(true)
    getGlobalGoToLatestBindingMock.mockReturnValue('⌃⌥⌘J')
    const listener = await startBridgeAndCaptureListener()
    listener({ payload: payload() })
    await flushAsync()

    const [, options] = addToastMock.mock.calls[0] as unknown as [unknown, { props: { globalBinding: string } }]
    expect(options.props.globalBinding).toBe('⌃⌥⌘J')
  })

  it('passes an empty global binding when the hotkey is disabled (no global hint to teach)', async () => {
    getGlobalGoToLatestEnabledMock.mockReturnValue(false)
    getGlobalGoToLatestBindingMock.mockReturnValue('⌃⌥⌘J')
    const listener = await startBridgeAndCaptureListener()
    listener({ payload: payload() })
    await flushAsync()

    const [, options] = addToastMock.mock.calls[0] as unknown as [unknown, { props: { globalBinding: string } }]
    expect(options.props.globalBinding).toBe('')
  })

  it('skips the toast entirely when neither go-to-latest shortcut is set', async () => {
    // In-app unbound AND global off: the toast has nothing to teach, so it
    // doesn't appear even though the mode is 'in-app' (not 'neither').
    getDownloadsNotificationsModeMock.mockReturnValue('in-app')
    getEffectiveShortcutsMock.mockReturnValue([])
    getGlobalGoToLatestEnabledMock.mockReturnValue(false)
    const listener = await startBridgeAndCaptureListener()
    listener({ payload: payload() })
    await flushAsync()

    expect(addToastMock).not.toHaveBeenCalled()
  })

  it('in "both" mode with no shortcuts set, still fires the macOS notification but no toast', async () => {
    // The skip only drops the in-app toast (the shortcut teacher); the native
    // notification is a separate surface and never carried a hint.
    getDownloadsNotificationsModeMock.mockReturnValue('both')
    getEffectiveShortcutsMock.mockReturnValue([])
    getGlobalGoToLatestEnabledMock.mockReturnValue(false)
    const listener = await startBridgeAndCaptureListener()
    listener({ payload: payload() })
    await flushMacosBurst()

    expect(addToastMock).not.toHaveBeenCalled()
    expect(sendNotificationMock).toHaveBeenCalledTimes(1)
  })

  it('coalesces a burst into ONE macOS notification naming the newest file', async () => {
    // The OS surface can't dedup its own banners (no identifier reaches macOS
    // through the plugin), so the bridge holds a burst and sends one.
    getDownloadsNotificationsModeMock.mockReturnValue('macos')
    const listener = await startBridgeAndCaptureListener()
    listener({ payload: payload({ fileName: 'first.pdf' }) })
    listener({ payload: payload({ fileName: 'second.pdf' }) })
    listener({ payload: payload({ fileName: 'third.pdf' }) })
    await flushMacosBurst()

    expect(sendNotificationMock).toHaveBeenCalledTimes(1)
    const [arg] = sendNotificationMock.mock.calls[0] as unknown as [{ title: string; body?: string }]
    // The newest file is the one the user is told about.
    expect(arg.body).toContain('third.pdf')
    expect(arg.body).not.toContain('first.pdf')
    // ...and the count of what else landed is not swallowed.
    expect(arg.title).toContain('3')
  })

  it('a lone detection keeps the single-file wording (no count, no "most recent")', async () => {
    getDownloadsNotificationsModeMock.mockReturnValue('macos')
    const listener = await startBridgeAndCaptureListener()
    listener({ payload: payload({ fileName: 'only.pdf' }) })
    await flushMacosBurst()

    expect(sendNotificationMock).toHaveBeenCalledTimes(1)
    const [arg] = sendNotificationMock.mock.calls[0] as unknown as [{ title: string; body?: string }]
    expect(arg.title).toBe('Downloaded only.pdf')
  })

  it('a later burst gets its own notification (the window does not swallow it)', async () => {
    getDownloadsNotificationsModeMock.mockReturnValue('macos')
    const listener = await startBridgeAndCaptureListener()
    listener({ payload: payload({ fileName: 'first.pdf' }) })
    await flushMacosBurst()
    listener({ payload: payload({ fileName: 'later.pdf' }) })
    await flushMacosBurst()

    expect(sendNotificationMock).toHaveBeenCalledTimes(2)
    const [second] = sendNotificationMock.mock.calls[1] as unknown as [{ title: string }]
    expect(second.title).toBe('Downloaded later.pdf')
  })

  it('coalescing does NOT throttle the in-app toast: every detection still gets one', async () => {
    // The toast has its own one-at-a-time rule (the store's group cap); it must
    // not also inherit the macOS burst window, or a toast would arrive late.
    getDownloadsNotificationsModeMock.mockReturnValue('both')
    const listener = await startBridgeAndCaptureListener()
    listener({ payload: payload({ fileName: 'first.pdf' }) })
    listener({ payload: payload({ fileName: 'second.pdf' }) })
    await flushAsync()

    expect(addToastMock).toHaveBeenCalledTimes(2)
    await flushMacosBurst()
    expect(sendNotificationMock).toHaveBeenCalledTimes(1)
  })

  it('skips notification dispatch entirely while the FDA gate is pending', async () => {
    // Defense in depth: the watcher won't fire under a closed gate, but if a
    // stale event leaks through we must not surface a toast or a macOS popup
    // before the gate clears.
    downloadsWatcherStatusMock.mockResolvedValue({
      status: 'ok',
      data: { running: false, downloadsDir: null, fdaPending: true },
    })
    getDownloadsNotificationsModeMock.mockReturnValue('both')
    const listener = await startBridgeAndCaptureListener()
    listener({ payload: payload() })
    await flushMacosBurst()

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
    getGlobalGoToLatestEnabledMock.mockReset().mockReturnValue(true)
    getGlobalGoToLatestBindingMock.mockReset().mockReturnValue('⌃⌥⌘J')
    getDownloadsToastCollapsedMock.mockReset().mockReturnValue(false)
    downloadsWatcherStatusMock.mockReset().mockResolvedValue({
      status: 'ok',
      data: { running: true, downloadsDir: '/Users/me/Downloads', fdaPending: false },
    })
    __resetPermissionCacheForTests()
  })

  it("asks for OS permission once when it isn't already granted, then fires the notification", async () => {
    isPermissionGrantedMock.mockResolvedValue(false)
    requestPermissionMock.mockResolvedValue('granted')

    const listener = await startBridgeAndCaptureListener()
    listener({ payload: { ...payload() } })
    await flushMacosBurst()

    expect(requestPermissionMock).toHaveBeenCalledTimes(1)
    expect(sendNotificationMock).toHaveBeenCalledTimes(1)
  })

  it('shows an INFO toast and does not fire when the user denies the OS prompt', async () => {
    isPermissionGrantedMock.mockResolvedValue(false)
    requestPermissionMock.mockResolvedValue('denied')

    const listener = await startBridgeAndCaptureListener()
    listener({ payload: { ...payload() } })
    await flushMacosBurst()

    expect(sendNotificationMock).not.toHaveBeenCalled()
    // One INFO toast surfaces with the dedup id.
    expect(addToastMock).toHaveBeenCalled()
    const calls = addToastMock.mock.calls as unknown as [unknown, Record<string, unknown>][]
    const hasInfoToast = calls.some(([, options]) => options.level === 'info')
    expect(hasInfoToast).toBe(true)
  })
})
