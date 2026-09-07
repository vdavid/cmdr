/**
 * Which move the connect flow makes for a place, and what it does with the
 * answer.
 *
 * ❗ The three arms are the point. Picking one wrong is SILENT: re-dialing a
 * registered volume registers a second one under a second id, and dialing a
 * volume the backoff loop already owns races it. The cells below pin each arm to
 * the IPC it may and may not send, over the real bindings.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { clearIpcMocks, installIpcMock, type IpcRecorder } from '$lib/ipc/test-helpers'

const startCycle = vi.fn()
vi.mock('$lib/file-explorer/network/smb-reconnect-manager.svelte', () => ({
  smbReconnectManager: {
    startCycle: (...args: unknown[]) => {
      startCycle(...(args as []))
    },
  },
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { cancelPlaceConnect, connectPlace } from './connect-flow'

const VOLUME_ID = 'sftp-nas-local-22-ada'

let ipc: IpcRecorder

beforeEach(() => {
  vi.clearAllMocks()
  ipc = installIpcMock()
})
afterEach(() => {
  clearIpcMocks()
})

describe('arm 1: a registered place whose session dropped', () => {
  it('hands it to the reconnect manager and ❌ never dials', async () => {
    const result = await connectPlace({ volumeId: VOLUME_ID, connectionState: 'disconnected' })
    expect(result).toEqual({ kind: 'reconnecting' })
    expect(startCycle).toHaveBeenCalledWith(VOLUME_ID)
    // A dial here would register a SECOND volume under a second id.
    expect(ipc.callCount('connect_saved_place')).toBe(0)
  })
})

describe('arm 2: a registered place asking for a credential', () => {
  it('refuses with the reason when the caller supplies no sheet, and ❌ dials nothing', async () => {
    const result = await connectPlace({ volumeId: VOLUME_ID, connectionState: 'needs_sign_in' })
    // ❗ Not `authentication_rejected`: nothing was offered, so nothing is wrong.
    expect(result).toEqual({ kind: 'refused', refusal: 'needs_credentials' })
    expect(ipc.callCount('connect_saved_place')).toBe(0)
    expect(startCycle).not.toHaveBeenCalled()
  })

  it('hands it to the sheet as a REGISTERED place, so the sheet mends rather than dials', async () => {
    const openSignIn = vi.fn(() => Promise.resolve({ signedIn: true as const, volumeId: VOLUME_ID }))
    const result = await connectPlace({ volumeId: VOLUME_ID, connectionState: 'needs_sign_in', openSignIn })
    // ❗ `registered: true` is what sends the sheet to
    // `reconnect_volume_with_credentials`. A dial would register a SECOND volume.
    expect(openSignIn).toHaveBeenCalledWith({ volumeId: VOLUME_ID, registered: true })
    expect(result).toEqual({ kind: 'connected', volumeId: VOLUME_ID })
    expect(ipc.callCount('connect_saved_place')).toBe(0)
  })

  it('a sheet the user closed reads as cancelled, which says nothing', async () => {
    const openSignIn = vi.fn(() => Promise.resolve({ signedIn: false as const }))
    const result = await connectPlace({ volumeId: VOLUME_ID, connectionState: 'needs_sign_in', openSignIn })
    expect(result).toEqual({ kind: 'cancelled' })
  })
})

describe('arm 3: a saved place with nothing registered', () => {
  it('dials by saved entry and returns the live volume', async () => {
    ipc.mock('connect_saved_place', () => ({ outcome: 'connected', volumeId: VOLUME_ID }))
    const result = await connectPlace({ volumeId: VOLUME_ID, connectionState: 'saved' })
    expect(result).toEqual({ kind: 'connected', volumeId: VOLUME_ID })
    expect(ipc.lastCall('connect_saved_place')?.payload).toMatchObject({ volumeId: VOLUME_ID, secret: null })
    expect(startCycle).not.toHaveBeenCalled()
  })

  it('hands the attempt id out BEFORE the dial, so a cancel has something to aim at', async () => {
    let idAtDialTime: string | undefined
    let armed: string | undefined
    ipc.mock('connect_saved_place', (payload) => {
      idAtDialTime = (payload as { attemptId: string }).attemptId
      // The button was armed before this call ran.
      expect(armed).toBe(idAtDialTime)
      return { outcome: 'connected', volumeId: VOLUME_ID }
    })
    await connectPlace({
      volumeId: VOLUME_ID,
      connectionState: 'saved',
      onAttemptStarted: (id) => {
        armed = id
      },
    })
    expect(armed).toBeDefined()
    expect(idAtDialTime).toBe(armed)
  })

  it('reads every outcome as its own reason, never as a wrong password', async () => {
    const cases = [
      [{ outcome: 'authentication_rejected' }, 'authentication_rejected'],
      [{ outcome: 'needs_credentials' }, 'needs_credentials'],
      [{ outcome: 'auth_method_unsupported' }, 'auth_method_unsupported'],
      [{ outcome: 'certificate_untrusted' }, 'certificate_untrusted'],
      [{ outcome: 'not_a_webdav_server' }, 'not_a_webdav_server'],
      [{ outcome: 'invalid_url' }, 'invalid_url'],
      [{ outcome: 'timed_out' }, 'timed_out'],
      [{ outcome: 'unreachable' }, 'unreachable'],
      [{ outcome: 'host_key_revoked', algorithm: 'ssh-ed25519', fingerprint: 'SHA256:x' }, 'host_key_revoked'],
      [
        {
          outcome: 'needs_host_key_approval',
          host: 'nas.local',
          port: 22,
          algorithm: 'ssh-ed25519',
          fingerprint: 'SHA256:x',
          kind: 'unknown',
        },
        'host_key_untrusted',
      ],
    ] as const
    for (const [outcome, refusal] of cases) {
      ipc.mock('connect_saved_place', () => outcome)
      const result = await connectPlace({ volumeId: VOLUME_ID, connectionState: 'saved' })
      expect(result, refusal).toEqual({ kind: 'refused', refusal })
    }
  })

  it('a cancelled dial says nothing: the user pressed the button', async () => {
    ipc.mock('connect_saved_place', () => ({ outcome: 'cancelled' }))
    expect(await connectPlace({ volumeId: VOLUME_ID, connectionState: 'saved' })).toEqual({ kind: 'cancelled' })
  })

  it('hands a host-key question to the sheet, on the step the outcome names', async () => {
    const prompt = {
      outcome: 'needs_host_key_approval',
      host: 'nas.local',
      port: 22,
      algorithm: 'ssh-ed25519',
      fingerprint: 'SHA256:x',
      kind: 'unknown',
    }
    ipc.mock('connect_saved_place', () => prompt)
    const openSignIn = vi.fn(() => Promise.resolve({ signedIn: true as const, volumeId: VOLUME_ID }))
    const result = await connectPlace({ volumeId: VOLUME_ID, connectionState: 'saved', openSignIn })

    expect(openSignIn).toHaveBeenCalledWith({ volumeId: VOLUME_ID, registered: false, firstOutcome: prompt })
    expect(result).toEqual({ kind: 'connected', volumeId: VOLUME_ID })
  })

  it('hands a refused credential to the sheet, and the sheet owns the rounds from there', async () => {
    ipc.mock('connect_saved_place', () => ({ outcome: 'authentication_rejected' }))
    const openSignIn = vi.fn(() => Promise.resolve({ signedIn: false as const }))
    const result = await connectPlace({ volumeId: VOLUME_ID, connectionState: 'saved', openSignIn })

    expect(openSignIn).toHaveBeenCalledTimes(1)
    // ❗ Exactly one dial from here. The sheet dials again itself, through the
    // attempt it was handed, and stays open across those rounds.
    expect(ipc.callCount('connect_saved_place')).toBe(1)
    expect(result).toEqual({ kind: 'cancelled' })
  })

  it('❌ never opens the sheet for a refusal no typing can fix', async () => {
    // The server challenged with a scheme Cmdr doesn't speak: the secret never
    // left, and a password box over it asks for something that cannot help.
    ipc.mock('connect_saved_place', () => ({ outcome: 'auth_method_unsupported' }))
    const openSignIn = vi.fn(() => Promise.resolve({ signedIn: false as const }))
    const result = await connectPlace({ volumeId: VOLUME_ID, connectionState: 'saved', openSignIn })

    expect(openSignIn).not.toHaveBeenCalled()
    expect(result).toEqual({ kind: 'refused', refusal: 'auth_method_unsupported' })
  })

  it('a typed refusal (the wrong arm for this volume) never reaches the user as itself', async () => {
    ipc.mock('connect_saved_place', () => {
      throw { reason: 'already_connected', volumeId: VOLUME_ID }
    })
    const result = await connectPlace({ volumeId: VOLUME_ID, connectionState: 'saved' })
    expect(result).toEqual({ kind: 'refused', refusal: 'unreachable' })
  })
})

describe('a place that is already serving', () => {
  it('is left alone', async () => {
    for (const state of ['direct', 'os_mount'] as const) {
      expect(await connectPlace({ volumeId: VOLUME_ID, connectionState: state })).toEqual({ kind: 'already_live' })
    }
    expect(ipc.callCount('connect_saved_place')).toBe(0)
  })
})

describe('cancelling', () => {
  it('calls the family cancel with the attempt id', async () => {
    ipc.mock('cancel_server_connect', () => true)
    await cancelPlaceConnect('server-connect-1')
    expect(ipc.lastCall('cancel_server_connect')?.payload).toMatchObject({ attemptId: 'server-connect-1' })
  })

  it('swallows a cancel that lands after the dial finished', async () => {
    ipc.mock('cancel_server_connect', () => {
      throw new Error('no such attempt')
    })
    await expect(cancelPlaceConnect('server-connect-1')).resolves.toBeUndefined()
  })
})
