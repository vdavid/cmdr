import { beforeEach, describe, expect, it, vi } from 'vitest'

const { openSettingsWindowSpy, warnSpy } = vi.hoisted(() => ({
  openSettingsWindowSpy: vi.fn(),
  warnSpy: vi.fn(),
}))

vi.mock('#lib/settings/settings-window.js', () => ({
  openSettingsWindow: openSettingsWindowSpy,
  settingAnchorId: (key: string) => `anchor:${key}`,
}))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn: warnSpy, info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { openSettingsToSmbEncryption, REQUIRE_SMB_ENCRYPTION_SETTING_KEY } from './smb-encryption-setting'

describe('openSettingsToSmbEncryption', () => {
  beforeEach(() => {
    openSettingsWindowSpy.mockReset()
    warnSpy.mockReset()
  })

  it('opens Settings at the SMB section, scrolled to the encryption row', async () => {
    openSettingsWindowSpy.mockResolvedValue(undefined)

    await openSettingsToSmbEncryption()

    expect(openSettingsWindowSpy).toHaveBeenCalledWith(
      'smb-encryption-refusal',
      ['File systems', 'SMB/Network shares'],
      `anchor:${REQUIRE_SMB_ENCRYPTION_SETTING_KEY}`,
    )
    expect(warnSpy).not.toHaveBeenCalled()
  })

  it('logs a window that will not open rather than throwing out of the button', async () => {
    const err = new Error('no window')
    openSettingsWindowSpy.mockRejectedValue(err)

    await expect(openSettingsToSmbEncryption()).resolves.toBeUndefined()

    expect(warnSpy).toHaveBeenCalledWith(expect.any(String), { err })
  })
})
