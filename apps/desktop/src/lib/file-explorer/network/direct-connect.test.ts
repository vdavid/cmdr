/**
 * Tests for the shared "Connect directly" flow.
 *
 * The contract every caller leans on: `connectDirectly` never resolves without
 * having told the user something. A button wired straight to it can't produce a
 * press that looks like it did nothing.
 *
 * ❗ The credential ask is the one sign-in sheet, so the tests assert the REQUEST
 * it makes and drive the retry through the `attempt` it handed over.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { UpgradeResult } from '$lib/tauri-commands'

const {
  upgradeToSmbVolume,
  upgradeToSmbVolumeUsingSavedPassword,
  upgradeToSmbVolumeWithCredentials,
  systemHasSavedSmbPassword,
} = vi.hoisted(() => ({
  upgradeToSmbVolume: vi.fn<() => Promise<UpgradeResult>>(),
  upgradeToSmbVolumeUsingSavedPassword: vi.fn<() => Promise<UpgradeResult>>(),
  upgradeToSmbVolumeWithCredentials: vi.fn<() => Promise<UpgradeResult>>(),
  systemHasSavedSmbPassword: vi.fn<() => Promise<boolean>>(),
}))
vi.mock('$lib/tauri-commands', () => ({
  upgradeToSmbVolume,
  upgradeToSmbVolumeUsingSavedPassword,
  upgradeToSmbVolumeWithCredentials,
  systemHasSavedSmbPassword,
  getUsernameHint: vi.fn<() => Promise<string | null>>(() => Promise.resolve(null)),
  getKnownShareByName: vi.fn(() => Promise.resolve(null)),
}))

const { openSignInSheet } = vi.hoisted(() => ({ openSignInSheet: vi.fn() }))
vi.mock('$lib/servers/sign-in-sheet-state.svelte', () => ({ openSignInSheet }))

const { requestVolumeRefresh } = vi.hoisted(() => ({ requestVolumeRefresh: vi.fn() }))

const { ask } = vi.hoisted(() => ({ ask: vi.fn<() => Promise<boolean>>() }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ ask }))

const { addToast, dismissToast } = vi.hoisted(() => ({
  addToast: vi.fn<(content: unknown, options?: { level?: string }) => string>(),
  dismissToast: vi.fn<(id: string) => void>(),
}))
vi.mock('$lib/ui/toast', () => ({ addToast, dismissToast }))

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

/** The sheet request under test, as much of it as these assert. */
interface SheetRequest {
  mode: string
  shape: { kind: string; guestAllowed?: boolean }
  endpoint: { address: string; host: string; username?: string }
  refusal?: string
  attempt: (submission: {
    mode: 'sign-in'
    secret: { secret: string; remember: boolean } | null
    username: string | null
  }) => Promise<{ kind: string; refusal?: string }>
}

/** The one request the flow made of the sign-in sheet. */
async function sheetRequest(): Promise<SheetRequest> {
  await vi.waitFor(() => {
    expect(openSignInSheet, 'the sheet to have been asked for').toHaveBeenCalledTimes(1)
  })
  return openSignInSheet.mock.calls[0][0] as SheetRequest
}

beforeEach(() => {
  vi.clearAllMocks()
  addToast.mockReturnValue('toast-id')
  systemHasSavedSmbPassword.mockResolvedValue(false)
  openSignInSheet.mockResolvedValue({ kind: 'cancelled' })
})

describe('connectDirectly', () => {
  it('confirms a direct connection and refreshes the volume list', async () => {
    upgradeToSmbVolume.mockResolvedValue({ status: 'success' })

    await expect(connectDirectly('smb-archive')).resolves.toBe('connected')
    expect(requestVolumeRefresh).toHaveBeenCalled()
  })

  it('names the reason a reachable-but-uncooperative server stayed on the OS mount', async () => {
    upgradeToSmbVolume.mockResolvedValue({ status: 'networkError', reason: 'unreachable', displayName: 'Naspolya' })

    await expect(connectDirectly('smb-archive')).resolves.toBe('stillOnOsMount')
    expect(errorToasts()).toHaveLength(1)
  })

  it('still asks when the remembered-username lookup breaks down', async () => {
    // ❗ This runs BEFORE the sheet opens and nothing here can await it, so a
    // rejection would be an unhandled one AND a prompt that never appeared, over
    // a pre-fill nobody would miss.
    const { getUsernameHint } = await import('$lib/tauri-commands')
    vi.mocked(getUsernameHint).mockRejectedValueOnce(new Error('ipc down'))
    upgradeToSmbVolume.mockResolvedValue(credentialsNeeded)

    await expect(connectDirectly('smb-archive')).resolves.toBe('askingForCredentials')

    const request = await sheetRequest()
    expect(request.endpoint.username).toBeUndefined()
  })

  it('asks for a password on the one sign-in sheet, naming the share', async () => {
    upgradeToSmbVolume.mockResolvedValue(credentialsNeeded)

    await expect(connectDirectly('smb-archive')).resolves.toBe('askingForCredentials')

    const request = await sheetRequest()
    expect(request.mode).toBe('sign-in')
    // ❗ No guest option: a connection with no credential is exactly what just
    // came back needing one.
    expect(request.shape).toEqual({ kind: 'username_password', guestAllowed: false })
    expect(request.endpoint.address).toBe('smb://Naspolya/archive')
    expect(request.refusal).toBe('needs_credentials')
    expect(errorToasts()).toHaveLength(0)
  })

  it('upgrades with what the user typed, and says the share is fast now', async () => {
    upgradeToSmbVolume.mockResolvedValue(credentialsNeeded)
    upgradeToSmbVolumeWithCredentials.mockResolvedValue({ status: 'success' })

    await connectDirectly('smb-archive')
    const request = await sheetRequest()
    const outcome = await request.attempt({
      mode: 'sign-in',
      secret: { secret: 'hunter2', remember: true },
      username: 'david',
    })

    expect(outcome).toEqual({ kind: 'handed_off' })
    expect(upgradeToSmbVolumeWithCredentials).toHaveBeenCalledWith('smb-archive', 'david', 'hunter2', true)
    expect(requestVolumeRefresh).toHaveBeenCalled()
  })

  it('keeps the sheet open on a password the server refused', async () => {
    upgradeToSmbVolume.mockResolvedValue(credentialsNeeded)
    upgradeToSmbVolumeWithCredentials.mockResolvedValue(credentialsNeeded)

    await connectDirectly('smb-archive')
    const request = await sheetRequest()

    await expect(
      request.attempt({ mode: 'sign-in', secret: { secret: 'wrong', remember: true }, username: 'david' }),
    ).resolves.toEqual({ kind: 'refused', refusal: 'authentication_rejected' })
  })

  it('closes the sheet and names the reason when the server itself is the problem', async () => {
    // The sheet's vocabulary is about credentials. A server that stopped
    // answering has nothing a password can fix, so the sheet closes and the toast
    // says what happened.
    upgradeToSmbVolume.mockResolvedValue(credentialsNeeded)
    upgradeToSmbVolumeWithCredentials.mockResolvedValue({
      status: 'networkError',
      reason: 'unreachable',
      displayName: 'Naspolya',
    })

    await connectDirectly('smb-archive')
    const request = await sheetRequest()

    await expect(
      request.attempt({ mode: 'sign-in', secret: { secret: 'hunter2', remember: false }, username: 'david' }),
    ).resolves.toEqual({ kind: 'handed_off' })
    expect(errorToasts()).toHaveLength(1)
  })

  it('says so out loud when the attempt itself breaks down', async () => {
    upgradeToSmbVolume.mockRejectedValue(new Error('boom'))

    await expect(connectDirectly('smb-archive')).resolves.toBe('stillOnOsMount')
    expect(errorToasts()).toHaveLength(1)
  })

  it('dismisses its progress toast on every path, so no spinner outlives the attempt', async () => {
    upgradeToSmbVolume.mockRejectedValue(new Error('boom'))

    await connectDirectly('smb-archive')

    expect(dismissToast).toHaveBeenCalledWith('toast-id')
  })

  it('reuses the password macOS already saved before asking anyone to type one', async () => {
    upgradeToSmbVolume.mockResolvedValue(credentialsNeeded)
    systemHasSavedSmbPassword.mockResolvedValue(true)
    ask.mockResolvedValue(true)
    upgradeToSmbVolumeUsingSavedPassword.mockResolvedValue({ status: 'success' })
    await expect(connectDirectly('smb-archive')).resolves.toBe('connected')
    expect(openSignInSheet).not.toHaveBeenCalled()
  })

  it('falls to the login form when the saved password no longer works', async () => {
    upgradeToSmbVolume.mockResolvedValue(credentialsNeeded)
    systemHasSavedSmbPassword.mockResolvedValue(true)
    ask.mockResolvedValue(true)
    upgradeToSmbVolumeUsingSavedPassword.mockResolvedValue(credentialsNeeded)
    await expect(connectDirectly('smb-archive')).resolves.toBe('askingForCredentials')
    expect(openSignInSheet).toHaveBeenCalled()
  })

  it('goes to the login form when the user would rather type the password', async () => {
    upgradeToSmbVolume.mockResolvedValue(credentialsNeeded)
    systemHasSavedSmbPassword.mockResolvedValue(true)
    ask.mockResolvedValue(false)
    await expect(connectDirectly('smb-archive')).resolves.toBe('askingForCredentials')
    expect(upgradeToSmbVolumeUsingSavedPassword).not.toHaveBeenCalled()
  })
})
