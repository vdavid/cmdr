/**
 * Tests for `notifications-mode.ts` (M7).
 *
 * Pins the deep-link contract that M5's "Stop showing these" button relies on:
 *   - `setDownloadsNotificationsMode('neither')` writes the registry key.
 *   - `openSettingsToDownloadsNotifications()` navigates to the renamed section
 *     path AND carries the sub-group anchor so the link lands on the right row.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

const { getSettingMock, setSettingMock, openSettingsWindowMock } = vi.hoisted(() => ({
  getSettingMock: vi.fn(),
  setSettingMock: vi.fn(),
  openSettingsWindowMock: vi.fn<(section?: string[], anchor?: string) => Promise<void>>(),
}))

vi.mock('$lib/settings', () => ({
  getSetting: getSettingMock,
  setSetting: setSettingMock,
}))

vi.mock('$lib/settings/settings-window', () => ({
  openSettingsWindow: openSettingsWindowMock,
}))

import {
  getDownloadsNotificationsMode,
  setDownloadsNotificationsMode,
  openSettingsToDownloadsNotifications,
} from './notifications-mode'

beforeEach(() => {
  getSettingMock.mockReset()
  setSettingMock.mockReset()
  openSettingsWindowMock.mockReset().mockResolvedValue(undefined)
})

describe('getDownloadsNotificationsMode', () => {
  it('returns the stored value when valid', () => {
    getSettingMock.mockReturnValue('macos')
    expect(getDownloadsNotificationsMode()).toBe('macos')
  })

  it("falls back to 'in-app' when the stored value is invalid", () => {
    getSettingMock.mockReturnValue('garbage')
    expect(getDownloadsNotificationsMode()).toBe('in-app')
  })

  it("falls back to 'in-app' when getSetting throws (pre-M7 registry-miss path)", () => {
    getSettingMock.mockImplementation(() => {
      throw new Error('unknown setting')
    })
    expect(getDownloadsNotificationsMode()).toBe('in-app')
  })
})

describe('setDownloadsNotificationsMode', () => {
  it('writes through to the settings store', () => {
    setDownloadsNotificationsMode('neither')
    expect(setSettingMock).toHaveBeenCalledWith('behavior.fileSystemWatching.downloadsNotifications', 'neither')
  })
})

describe('openSettingsToDownloadsNotifications', () => {
  it('navigates to the renamed section path and the sub-group anchor', async () => {
    await openSettingsToDownloadsNotifications()
    expect(openSettingsWindowMock).toHaveBeenCalledTimes(1)
    const [section, anchor] = openSettingsWindowMock.mock.calls[0]
    expect(section).toEqual(['Behavior', 'File system watching'])
    expect(anchor).toBe('settings-downloads-notifications')
  })
})
