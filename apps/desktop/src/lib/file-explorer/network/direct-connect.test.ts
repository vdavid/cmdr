/**
 * Tests for the shared "Connect directly" flow.
 *
 * The contract every caller leans on: `connectDirectly` never resolves without
 * having told the user something. A button wired straight to it can't produce a
 * press that looks like it did nothing.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { UpgradeResult } from '$lib/tauri-commands'

const { upgradeToSmbVolume, upgradeToSmbVolumeUsingSavedPassword, systemHasSavedSmbPassword } = vi.hoisted(() => ({
  upgradeToSmbVolume: vi.fn<() => Promise<UpgradeResult>>(),
  upgradeToSmbVolumeUsingSavedPassword: vi.fn<() => Promise<UpgradeResult>>(),
  systemHasSavedSmbPassword: vi.fn<() => Promise<boolean>>(),
}))
vi.mock('$lib/tauri-commands', () => ({
  upgradeToSmbVolume,
  upgradeToSmbVolumeUsingSavedPassword,
  systemHasSavedSmbPassword,
}))

const { ask } = vi.hoisted(() => ({ ask: vi.fn<() => Promise<boolean>>() }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ ask }))

const { addToast, dismissToast } = vi.hoisted(() => ({
  addToast: vi.fn<(content: unknown, options?: { level?: string }) => string>(),
  dismissToast: vi.fn<(id: string) => void>(),
}))
vi.mock('$lib/ui/toast', () => ({ addToast, dismissToast }))

const { requestVolumeRefresh } = vi.hoisted(() => ({ requestVolumeRefresh: vi.fn() }))
vi.mock('$lib/stores/volume-store.svelte', () => ({ requestVolumeRefresh }))

vi.mock('./lazy-trigger', () => ({ triggerNetworkDiscovery: vi.fn() }))

import { connectDirectly } from './direct-connect'

const credentialsNeeded: UpgradeResult = {
  status: 'credentialsNeeded',
  server: 'naspolya',
  share: 'archive',
  port: 445,
  displayName: 'Naspolya',
  usernameHint: null,
  message: null,
}

/** Toasts the user would actually read as bad news. */
function errorToasts(): unknown[] {
  return addToast.mock.calls.filter((call) => call[1]?.level === 'error').map((call) => call[0])
}

beforeEach(() => {
  vi.clearAllMocks()
  addToast.mockReturnValue('toast-id')
  systemHasSavedSmbPassword.mockResolvedValue(false)
})

describe('connectDirectly', () => {
  it('confirms a direct connection and refreshes the volume list', async () => {
    upgradeToSmbVolume.mockResolvedValue({ status: 'success' })

    await expect(connectDirectly('smb-archive', () => true)).resolves.toBe('connected')
    expect(requestVolumeRefresh).toHaveBeenCalled()
  })

  it('names the reason a reachable-but-uncooperative server stayed on the OS mount', async () => {
    upgradeToSmbVolume.mockResolvedValue({ status: 'networkError', reason: 'unreachable', displayName: 'Naspolya' })

    await expect(connectDirectly('smb-archive', () => true)).resolves.toBe('stillOnOsMount')
    expect(errorToasts()).toHaveLength(1)
  })

  it('hands a credentials request to whoever can render the form', async () => {
    upgradeToSmbVolume.mockResolvedValue(credentialsNeeded)
    const raise = vi.fn(() => true)

    await expect(connectDirectly('smb-archive', raise)).resolves.toBe('askingForCredentials')
    expect(raise).toHaveBeenCalledWith(credentialsNeeded, 'smb-archive')
    expect(errorToasts()).toHaveLength(0)
  })

  it('says so out loud when nothing can host the credential form', async () => {
    // Without this the press would end in silence: the form never appears, and
    // the flow would report a state nobody can see.
    upgradeToSmbVolume.mockResolvedValue(credentialsNeeded)

    await expect(connectDirectly('smb-archive', () => false)).resolves.toBe('stillOnOsMount')
    expect(errorToasts()).toHaveLength(1)
  })

  it('says so out loud when the attempt itself breaks down', async () => {
    upgradeToSmbVolume.mockRejectedValue(new Error('boom'))

    await expect(connectDirectly('smb-archive', () => true)).resolves.toBe('stillOnOsMount')
    expect(errorToasts()).toHaveLength(1)
  })

  it('dismisses its progress toast on every path, so no spinner outlives the attempt', async () => {
    upgradeToSmbVolume.mockRejectedValue(new Error('boom'))

    await connectDirectly('smb-archive', () => true)

    expect(dismissToast).toHaveBeenCalledWith('toast-id')
  })

  it('reuses the password macOS already saved before asking anyone to type one', async () => {
    upgradeToSmbVolume.mockResolvedValue(credentialsNeeded)
    systemHasSavedSmbPassword.mockResolvedValue(true)
    ask.mockResolvedValue(true)
    upgradeToSmbVolumeUsingSavedPassword.mockResolvedValue({ status: 'success' })
    const raise = vi.fn(() => true)

    await expect(connectDirectly('smb-archive', raise)).resolves.toBe('connected')
    expect(raise).not.toHaveBeenCalled()
  })

  it('falls to the login form when the saved password no longer works', async () => {
    upgradeToSmbVolume.mockResolvedValue(credentialsNeeded)
    systemHasSavedSmbPassword.mockResolvedValue(true)
    ask.mockResolvedValue(true)
    upgradeToSmbVolumeUsingSavedPassword.mockResolvedValue(credentialsNeeded)
    const raise = vi.fn(() => true)

    await expect(connectDirectly('smb-archive', raise)).resolves.toBe('askingForCredentials')
    expect(raise).toHaveBeenCalled()
  })

  it('goes to the login form when the user would rather type the password', async () => {
    upgradeToSmbVolume.mockResolvedValue(credentialsNeeded)
    systemHasSavedSmbPassword.mockResolvedValue(true)
    ask.mockResolvedValue(false)
    const raise = vi.fn(() => true)

    await expect(connectDirectly('smb-archive', raise)).resolves.toBe('askingForCredentials')
    expect(upgradeToSmbVolumeUsingSavedPassword).not.toHaveBeenCalled()
  })
})
