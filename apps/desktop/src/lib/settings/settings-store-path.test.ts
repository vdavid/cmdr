/**
 * Tests for the settings store path resolver.
 *
 * The store path must redirect to the backend-supplied isolated path when one
 * is returned (dev / per-worktree dev / E2E), and stay byte-identical to the
 * bare `'settings.json'` name in production (backend returns `null`).
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    getIsolatedSettingsPath: vi.fn(),
  },
}))

import { commands } from '$lib/ipc/bindings'
import { resolveSettingsStorePath, SETTINGS_STORE_NAME } from './settings-store-path'

describe('resolveSettingsStorePath', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('returns the isolated path when the backend supplies one', async () => {
    const isolated = '/tmp/cmdr-e2e-data-mtp-123/settings.json'
    vi.mocked(commands.getIsolatedSettingsPath).mockResolvedValueOnce(isolated)
    expect(await resolveSettingsStorePath()).toBe(isolated)
  })

  it('falls back to the bare store name in production (backend returns null)', async () => {
    vi.mocked(commands.getIsolatedSettingsPath).mockResolvedValueOnce(null)
    expect(await resolveSettingsStorePath()).toBe(SETTINGS_STORE_NAME)
    expect(SETTINGS_STORE_NAME).toBe('settings.json')
  })

  it('falls back to the bare store name and reports when the command rejects', async () => {
    const err = new Error('boom')
    vi.mocked(commands.getIsolatedSettingsPath).mockRejectedValueOnce(err)
    const onError = vi.fn()
    expect(await resolveSettingsStorePath(onError)).toBe(SETTINGS_STORE_NAME)
    expect(onError).toHaveBeenCalledWith(err)
  })

  it('does not require an onError callback when the command rejects', async () => {
    vi.mocked(commands.getIsolatedSettingsPath).mockRejectedValueOnce(new Error('boom'))
    expect(await resolveSettingsStorePath()).toBe(SETTINGS_STORE_NAME)
  })
})
