/**
 * Tests for `notifications-mode.ts`.
 *
 * Pins the contracts the warn toast and the settings-applier rely on:
 *   - `setLowDiskSpaceNotificationsMode('off')` writes the registry key.
 *   - `openSettingsToLowDiskSpace()` navigates to the Notifications section AND
 *     carries the sub-group anchor so the "Disable these notifications" link
 *     lands on the right row.
 *   - `pushLowDiskSpaceConfigToBackend()` re-reads both settings fresh and
 *     maps the mode to the backend's enabled flag.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const { getSettingMock, setSettingMock, openSettingsWindowMock, setLowDiskSpaceConfigMock } = vi.hoisted(() => ({
  getSettingMock: vi.fn(),
  setSettingMock: vi.fn(),
  openSettingsWindowMock: vi.fn<(section?: string[], anchor?: string) => Promise<void>>(),
  setLowDiskSpaceConfigMock: vi.fn<(enabled: boolean, thresholdPercent: number) => Promise<void>>(),
}))

vi.mock('$lib/settings', () => ({
  getSetting: getSettingMock,
  setSetting: setSettingMock,
}))

vi.mock('$lib/settings/settings-window', () => ({
  openSettingsWindow: openSettingsWindowMock,
}))

vi.mock('$lib/tauri-commands', () => ({
  setLowDiskSpaceConfig: setLowDiskSpaceConfigMock,
}))

import {
  getLowDiskSpaceNotificationsMode,
  setLowDiskSpaceNotificationsMode,
  getLowDiskSpaceThresholdPercent,
  pushLowDiskSpaceConfigToBackend,
  openSettingsToLowDiskSpace,
  LOW_DISK_SPACE_NOTIFICATIONS_SETTING_KEY,
  LOW_DISK_SPACE_THRESHOLD_SETTING_KEY,
} from './notifications-mode'

beforeEach(() => {
  getSettingMock.mockReset()
  setSettingMock.mockReset()
  openSettingsWindowMock.mockReset().mockResolvedValue(undefined)
  setLowDiskSpaceConfigMock.mockReset().mockResolvedValue(undefined)
})

describe('getLowDiskSpaceNotificationsMode', () => {
  it('returns the stored value when valid', () => {
    getSettingMock.mockReturnValue('macos')
    expect(getLowDiskSpaceNotificationsMode()).toBe('macos')
  })

  it("falls back to 'in-app' when the stored value is invalid", () => {
    getSettingMock.mockReturnValue('garbage')
    expect(getLowDiskSpaceNotificationsMode()).toBe('in-app')
  })

  it("falls back to 'in-app' when getSetting throws (registry-miss safety path)", () => {
    getSettingMock.mockImplementation(() => {
      throw new Error('unknown setting')
    })
    expect(getLowDiskSpaceNotificationsMode()).toBe('in-app')
  })
})

describe('setLowDiskSpaceNotificationsMode', () => {
  it('writes through to the settings store', () => {
    setLowDiskSpaceNotificationsMode('off')
    expect(setSettingMock).toHaveBeenCalledWith(LOW_DISK_SPACE_NOTIFICATIONS_SETTING_KEY, 'off')
  })
})

describe('getLowDiskSpaceThresholdPercent', () => {
  it('returns the stored value when valid', () => {
    getSettingMock.mockReturnValue(10)
    expect(getLowDiskSpaceThresholdPercent()).toBe(10)
  })

  it('falls back to the default when the stored value is invalid', () => {
    getSettingMock.mockReturnValue('garbage')
    expect(getLowDiskSpaceThresholdPercent()).toBe(5)
  })

  it('falls back to the default when getSetting throws', () => {
    getSettingMock.mockImplementation(() => {
      throw new Error('unknown setting')
    })
    expect(getLowDiskSpaceThresholdPercent()).toBe(5)
  })
})

describe('pushLowDiskSpaceConfigToBackend', () => {
  function stubSettings(mode: unknown, threshold: unknown): void {
    getSettingMock.mockImplementation((id: string) => {
      if (id === LOW_DISK_SPACE_NOTIFICATIONS_SETTING_KEY) return mode
      if (id === LOW_DISK_SPACE_THRESHOLD_SETTING_KEY) return threshold
      throw new Error(`unexpected setting read: ${id}`)
    })
  }

  it("maps 'in-app' to enabled", async () => {
    stubSettings('in-app', 5)
    await pushLowDiskSpaceConfigToBackend()
    expect(setLowDiskSpaceConfigMock).toHaveBeenCalledWith(true, 5)
  })

  it("maps 'macos' to enabled", async () => {
    stubSettings('macos', 8)
    await pushLowDiskSpaceConfigToBackend()
    expect(setLowDiskSpaceConfigMock).toHaveBeenCalledWith(true, 8)
  })

  it("maps 'off' to disabled", async () => {
    stubSettings('off', 5)
    await pushLowDiskSpaceConfigToBackend()
    expect(setLowDiskSpaceConfigMock).toHaveBeenCalledWith(false, 5)
  })

  it('swallows IPC failures (logs, never throws)', async () => {
    stubSettings('in-app', 5)
    setLowDiskSpaceConfigMock.mockRejectedValue(new Error('ipc down'))
    await expect(pushLowDiskSpaceConfigToBackend()).resolves.toBeUndefined()
  })
})

describe('openSettingsToLowDiskSpace', () => {
  it('navigates to the section path and the sub-group anchor', async () => {
    await openSettingsToLowDiskSpace()
    expect(openSettingsWindowMock).toHaveBeenCalledTimes(1)
    const [section, anchor] = openSettingsWindowMock.mock.calls[0]
    expect(section).toEqual(['Behavior', 'Notifications'])
    expect(anchor).toBe('settings-low-disk-space')
  })
})
