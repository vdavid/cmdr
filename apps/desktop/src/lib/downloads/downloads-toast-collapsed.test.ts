import { describe, it, expect, vi, beforeEach } from 'vitest'

const { setSettingMock, getSettingMock } = vi.hoisted(() => ({
  setSettingMock: vi.fn(),
  getSettingMock: vi.fn(),
}))

vi.mock('$lib/settings', () => ({
  setSetting: setSettingMock,
  getSetting: getSettingMock,
}))

import { getDownloadsToastCollapsed, setDownloadsToastCollapsed } from './downloads-toast-collapsed'

const COLLAPSED_KEY = 'behavior.fileSystemWatching.downloadsToastCollapsed'

describe('downloads-toast-collapsed', () => {
  beforeEach(() => {
    setSettingMock.mockReset()
    getSettingMock.mockReset()
  })

  it('reads the collapsed flag from the registry key', () => {
    getSettingMock.mockReturnValue(true)
    expect(getDownloadsToastCollapsed()).toBe(true)
    expect(getSettingMock).toHaveBeenCalledWith(COLLAPSED_KEY)
  })

  it('writes the collapsed flag to the registry key', () => {
    setDownloadsToastCollapsed(true)
    expect(setSettingMock).toHaveBeenCalledWith(COLLAPSED_KEY, true)
    setDownloadsToastCollapsed(false)
    expect(setSettingMock).toHaveBeenCalledWith(COLLAPSED_KEY, false)
  })
})
