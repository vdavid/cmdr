/**
 * The permission wrappers let a real failure reach the caller.
 *
 * They used to swallow every rejection behind a "command not available (non-macOS)" fallback, which outlived its reason:
 * every platform registers these commands (`src-tauri/src/ipc.rs`). So a System Settings that didn't open vanished,
 * and onboarding flipped to "Restart Cmdr" as if it had opened, and a probe that broke answered "granted".
 */

import { describe, expect, it, vi } from 'vitest'

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: {
    openPrivacySettings: vi.fn(),
    checkFullDiskAccess: vi.fn(),
    checkFullDiskAccessQuiet: vi.fn(),
    getOsMajorVersion: vi.fn(),
  },
  events: {},
}))

import { commands } from '#lib/ipc/bindings.js'
import { checkFullDiskAccess, checkFullDiskAccessQuiet, getMacosMajorVersion, openPrivacySettings } from './storage'

describe('permission wrappers', () => {
  it('rejects when System Settings didn’t open, so the caller can show the way there', async () => {
    vi.mocked(commands.openPrivacySettings).mockResolvedValueOnce({ status: 'error', error: '`open` exited with 1' })
    await expect(openPrivacySettings()).rejects.toThrow()
  })

  it('resolves when System Settings opened', async () => {
    vi.mocked(commands.openPrivacySettings).mockResolvedValueOnce({ status: 'ok', data: null })
    await expect(openPrivacySettings()).resolves.toBeUndefined()
  })

  it('rejects a full disk access probe that broke, instead of answering granted', async () => {
    vi.mocked(commands.checkFullDiskAccess).mockRejectedValueOnce(new Error('IPC bridge gone'))
    await expect(checkFullDiskAccess()).rejects.toThrow()
  })

  it('rejects a quiet probe that broke, instead of answering granted', async () => {
    vi.mocked(commands.checkFullDiskAccessQuiet).mockRejectedValueOnce(new Error('IPC bridge gone'))
    await expect(checkFullDiskAccessQuiet()).rejects.toThrow()
  })

  it('rejects a macOS version read that broke, instead of answering 0', async () => {
    vi.mocked(commands.getOsMajorVersion).mockRejectedValueOnce(new Error('IPC bridge gone'))
    await expect(getMacosMajorVersion()).rejects.toThrow()
  })
})
