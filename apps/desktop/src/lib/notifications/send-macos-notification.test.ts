import { describe, it, expect, vi, beforeEach } from 'vitest'

/**
 * `sendMacosNotification` is the one path every feature takes to a native
 * banner: it asks macOS whether Cmdr's notifications are on, sends when they
 * are, tells the user once when they aren't, and never lets a send failure
 * escape as an unhandled rejection.
 */

const { getNotificationPermissionMock, showNotificationMock, addToastMock, warnMock } = vi.hoisted(() => ({
  getNotificationPermissionMock: vi.fn<() => Promise<string>>(),
  showNotificationMock: vi.fn(),
  addToastMock: vi.fn(() => 'toast-id'),
  warnMock: vi.fn(),
}))

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    getNotificationPermission: getNotificationPermissionMock,
    showNotification: showNotificationMock,
  },
}))

vi.mock('$lib/ui/toast', () => ({
  addToast: addToastMock,
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: warnMock, error: vi.fn() }),
}))

import { sendMacosNotification, __resetForTests } from './send-macos-notification'

const notification = { title: 'Downloaded report.pdf', body: '' }

beforeEach(() => {
  getNotificationPermissionMock.mockReset().mockResolvedValue('allowed')
  showNotificationMock.mockReset().mockResolvedValue({ status: 'ok', data: null })
  addToastMock.mockReset().mockReturnValue('toast-id')
  warnMock.mockReset()
  __resetForTests()
})

describe('sendMacosNotification', () => {
  it('sends when macOS allows Cmdr’s notifications', async () => {
    await sendMacosNotification(notification)
    expect(showNotificationMock).toHaveBeenCalledWith('Downloaded report.pdf', '')
    expect(addToastMock).not.toHaveBeenCalled()
  })

  it('sends when macOS has not asked yet (posting is what makes macOS ask)', async () => {
    getNotificationPermissionMock.mockResolvedValue('notDetermined')
    await sendMacosNotification(notification)
    expect(showNotificationMock).toHaveBeenCalledTimes(1)
  })

  it('sends when the state is unknowable (a dev build posts as Terminal)', async () => {
    getNotificationPermissionMock.mockResolvedValue('unknown')
    await sendMacosNotification(notification)
    expect(showNotificationMock).toHaveBeenCalledTimes(1)
  })

  it('skips the send and shows one info toast when notifications are off', async () => {
    getNotificationPermissionMock.mockResolvedValue('denied')
    await sendMacosNotification(notification)
    await sendMacosNotification(notification)

    expect(showNotificationMock).not.toHaveBeenCalled()
    expect(addToastMock).toHaveBeenCalledTimes(1)
    const [message, options] = addToastMock.mock.calls[0] as unknown as [string, Record<string, unknown>]
    expect(message).toContain('System Settings')
    expect(options).toMatchObject({ level: 'info', id: 'notifications:macos-permission-denied' })
  })

  it('asks again on every send, so turning notifications back on works without a restart', async () => {
    getNotificationPermissionMock.mockResolvedValueOnce('denied').mockResolvedValueOnce('allowed')
    await sendMacosNotification(notification)
    await sendMacosNotification(notification)

    expect(getNotificationPermissionMock).toHaveBeenCalledTimes(2)
    expect(showNotificationMock).toHaveBeenCalledTimes(1)
  })

  it('tells the user again after notifications went on and then off once more', async () => {
    getNotificationPermissionMock
      .mockResolvedValueOnce('denied')
      .mockResolvedValueOnce('allowed')
      .mockResolvedValueOnce('denied')
    await sendMacosNotification(notification)
    await sendMacosNotification(notification)
    await sendMacosNotification(notification)

    expect(addToastMock).toHaveBeenCalledTimes(2)
  })

  it('logs a failed send once, without throwing, until a send works again', async () => {
    showNotificationMock.mockResolvedValue({ status: 'error', error: 'notification center unavailable' })
    await expect(sendMacosNotification(notification)).resolves.toBeUndefined()
    await sendMacosNotification(notification)
    expect(warnMock).toHaveBeenCalledTimes(1)

    showNotificationMock.mockResolvedValueOnce({ status: 'ok', data: null })
    await sendMacosNotification(notification)
    await sendMacosNotification(notification)
    expect(warnMock).toHaveBeenCalledTimes(2)
  })

  it('absorbs a rejected IPC call the same way', async () => {
    showNotificationMock.mockRejectedValue(new Error('IPC gone'))
    await expect(sendMacosNotification(notification)).resolves.toBeUndefined()
    expect(warnMock).toHaveBeenCalledTimes(1)
  })

  it('sends anyway when the permission question itself fails', async () => {
    getNotificationPermissionMock.mockRejectedValue(new Error('IPC gone'))
    await sendMacosNotification(notification)
    expect(showNotificationMock).toHaveBeenCalledTimes(1)
    expect(addToastMock).not.toHaveBeenCalled()
  })
})
