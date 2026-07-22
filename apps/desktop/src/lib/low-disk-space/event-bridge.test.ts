import { describe, it, expect, vi, beforeEach } from 'vitest'

/**
 * The bridge mounts one `low-disk-space` listener and dispatches per the
 * settings enum. On the low edge (`isLow: true`) it shows a persistent warn
 * toast OR sends the macOS native notification; on the recovery edge
 * (`isLow: false`) it dismisses the toast (macOS no-ops). These tests pump the
 * listener callback the bridge registers and assert the resulting calls.
 */

type LowDiskSpaceListener = (payload: LowDiskSpacePayload) => void
interface LowDiskSpacePayload {
  volumeId: string
  totalBytes: number
  availableBytes: number
  freePercent: number
  thresholdPercent: number
  isLow: boolean
}

const {
  onLowDiskSpaceMock,
  getLowDiskSpaceNotificationsModeMock,
  ensureMacosNotificationPermissionMock,
  sendNotificationMock,
  addToastMock,
  dismissToastMock,
} = vi.hoisted(() => ({
  onLowDiskSpaceMock: vi.fn(),
  getLowDiskSpaceNotificationsModeMock: vi.fn<() => 'in-app' | 'macos' | 'off'>(),
  ensureMacosNotificationPermissionMock: vi.fn<() => Promise<boolean>>(),
  sendNotificationMock: vi.fn(),
  addToastMock: vi.fn<(content: unknown, options?: Record<string, unknown>) => string>(() => 'toast-id'),
  dismissToastMock: vi.fn<(id: string) => void>(),
}))

vi.mock('$lib/tauri-commands', () => ({
  onLowDiskSpace: onLowDiskSpaceMock,
}))

vi.mock('@tauri-apps/plugin-notification', () => ({
  sendNotification: sendNotificationMock,
}))

vi.mock('$lib/notifications/macos-notification-permission', () => ({
  ensureMacosNotificationPermission: ensureMacosNotificationPermissionMock,
}))

vi.mock('./notifications-mode', () => ({
  getLowDiskSpaceNotificationsMode: getLowDiskSpaceNotificationsModeMock,
}))

vi.mock('$lib/ui/toast', () => ({
  addToast: addToastMock,
  dismissToast: dismissToastMock,
}))

vi.mock('$lib/settings/format-utils', () => ({
  formatFileSizeWithFormat: (bytes: number) => `${String(bytes)} B`,
}))

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'binary',
}))

import { startLowDiskSpaceEventBridge } from './event-bridge.svelte'

async function flushAsync(): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await Promise.resolve()
  }
}

function payload(overrides: Partial<LowDiskSpacePayload> = {}): LowDiskSpacePayload {
  return {
    volumeId: 'root',
    totalBytes: 1_000_000_000_000,
    availableBytes: 42_000_000_000,
    freePercent: 4.2,
    thresholdPercent: 5,
    isLow: true,
    ...overrides,
  }
}

async function startBridgeAndCaptureListener(): Promise<LowDiskSpaceListener> {
  let captured: LowDiskSpaceListener | null = null
  onLowDiskSpaceMock.mockImplementation((handler: LowDiskSpaceListener) => {
    captured = handler
    return Promise.resolve(() => {})
  })
  await startLowDiskSpaceEventBridge()
  // `captured` is assigned inside the `onLowDiskSpace` mock's closure, so TS's
  // control-flow analysis still sees its initialized `null` here. The runtime
  // check is the actual contract.
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
  if (!captured) throw new Error('Bridge did not register a listener')
  return captured
}

describe('startLowDiskSpaceEventBridge', () => {
  beforeEach(() => {
    onLowDiskSpaceMock.mockReset()
    getLowDiskSpaceNotificationsModeMock.mockReset().mockReturnValue('in-app')
    ensureMacosNotificationPermissionMock.mockReset().mockResolvedValue(true)
    sendNotificationMock.mockReset()
    addToastMock.mockReset().mockReturnValue('toast-id')
    dismissToastMock.mockReset()
  })

  it('subscribes to the low-disk-space event', async () => {
    await startBridgeAndCaptureListener()
    expect(onLowDiskSpaceMock).toHaveBeenCalledWith(expect.any(Function))
  })

  it("mode 'in-app' dispatches a persistent warn toast with a per-volume dedup id and the boot volume's live-follow props", async () => {
    const listener = await startBridgeAndCaptureListener()
    listener(payload())
    await flushAsync()

    expect(addToastMock).toHaveBeenCalledTimes(1)
    const options = addToastMock.mock.calls[0][1]
    expect(options?.level).toBe('warn')
    expect(options?.dismissal).toBe('persistent')
    expect(options?.id).toBe('low-disk-space:root')
    expect(options?.props).toMatchObject({
      volumeId: 'root',
      availableBytes: 42_000_000_000,
      totalBytes: 1_000_000_000_000,
    })
    expect(dismissToastMock).not.toHaveBeenCalled()
    expect(sendNotificationMock).not.toHaveBeenCalled()
  })

  it("mode 'in-app' dismisses the toast on the recovery edge (isLow false)", async () => {
    const listener = await startBridgeAndCaptureListener()
    listener(payload({ isLow: false, freePercent: 6.5 }))
    await flushAsync()

    expect(dismissToastMock).toHaveBeenCalledWith('low-disk-space:root')
    expect(addToastMock).not.toHaveBeenCalled()
  })

  it("mode 'macos' sends a native notification only", async () => {
    getLowDiskSpaceNotificationsModeMock.mockReturnValue('macos')
    const listener = await startBridgeAndCaptureListener()
    listener(payload())
    await flushAsync()

    expect(sendNotificationMock).toHaveBeenCalledTimes(1)
    const [notification] = sendNotificationMock.mock.calls[0] as [{ title: string; body: string }]
    expect(notification.title).toBe('Low disk space')
    expect(notification.body).toContain('4.2%')
    expect(addToastMock).not.toHaveBeenCalled()
  })

  it("mode 'macos' no-ops on the recovery edge (a delivered notification can't be recalled)", async () => {
    getLowDiskSpaceNotificationsModeMock.mockReturnValue('macos')
    const listener = await startBridgeAndCaptureListener()
    listener(payload({ isLow: false, freePercent: 6.5 }))
    await flushAsync()

    expect(sendNotificationMock).not.toHaveBeenCalled()
    expect(dismissToastMock).not.toHaveBeenCalled()
    expect(addToastMock).not.toHaveBeenCalled()
  })

  it("mode 'macos' skips the notification when permission is denied", async () => {
    getLowDiskSpaceNotificationsModeMock.mockReturnValue('macos')
    ensureMacosNotificationPermissionMock.mockResolvedValue(false)
    const listener = await startBridgeAndCaptureListener()
    listener(payload())
    await flushAsync()

    expect(sendNotificationMock).not.toHaveBeenCalled()
    expect(addToastMock).not.toHaveBeenCalled()
  })

  it("mode 'off' dispatches nothing (defense in depth against a stale event)", async () => {
    getLowDiskSpaceNotificationsModeMock.mockReturnValue('off')
    const listener = await startBridgeAndCaptureListener()
    listener(payload())
    await flushAsync()

    expect(addToastMock).not.toHaveBeenCalled()
    expect(sendNotificationMock).not.toHaveBeenCalled()
  })
})
