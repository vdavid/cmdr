/**
 * The servers wrappers. Two risks live here and nowhere else in the family:
 * the argument order every command shares (`volumeId, attemptId, secret` reads
 * the same for a dial and a cancel), and `connectSavedPlace`'s typed refusal,
 * which is a `Result` on the wire and has to reach the caller as a throw rather
 * than as a `{ status: 'error' }` object nobody switches on.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    listSavedServers: vi.fn(),
    connectSavedPlace: vi.fn(),
    connectServer: vi.fn(),
    cancelServerConnect: vi.fn(),
    disconnectPlace: vi.fn(),
    setPlacePinned: vi.fn(),
    forgetServer: vi.fn(),
    hasServerSecret: vi.fn(),
    forgetServerSecret: vi.fn(),
    updateSavedServer: vi.fn(),
  },
}))

import { commands } from '$lib/ipc/bindings'
import {
  cancelServerConnect,
  connectSavedPlace,
  connectServer,
  disconnectPlace,
  forgetServer,
  forgetServerSecret,
  hasServerSecret,
  listSavedServers,
  newServerAttemptId,
  setPlacePinned,
  updateSavedServer,
  type ServerTarget,
} from './servers'

const target: ServerTarget = {
  protocol: 'sftp',
  displayName: 'Naspolya',
  host: 'nas.local',
  port: 22,
  username: 'ada',
  remoteRoot: '/srv/data',
  keyFile: null,
  useAgent: true,
  autoReconnect: true,
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('dialing', () => {
  it('hands a saved place its id, the caller’s attempt id, and the offer, in that order', async () => {
    vi.mocked(commands.connectSavedPlace).mockResolvedValueOnce({
      status: 'ok',
      data: { outcome: 'connected', volumeId: 'sftp-nas-local-22-ada' },
    })
    const offer = { secret: 'hunter2', remember: false }
    const outcome = await connectSavedPlace('sftp-nas-local-22-ada', 'attempt-1', offer)
    expect(commands.connectSavedPlace).toHaveBeenCalledWith('sftp-nas-local-22-ada', 'attempt-1', offer)
    expect(outcome).toEqual({ outcome: 'connected', volumeId: 'sftp-nas-local-22-ada' })
  })

  it('offers no secret by default: the dial reads the store', async () => {
    vi.mocked(commands.connectSavedPlace).mockResolvedValueOnce({ status: 'ok', data: { outcome: 'timed_out' } })
    await connectSavedPlace('sftp-nas-local-22-ada', 'attempt-2')
    expect(commands.connectSavedPlace).toHaveBeenCalledWith('sftp-nas-local-22-ada', 'attempt-2', null)
  })

  it('throws a saved-place refusal instead of handing back a result nobody switches on', async () => {
    // ❗ Neither refusal is something a person did: both mean the connect flow
    // picked the wrong move for the volume's standing, which is a bug to see in
    // a log, not a message to word for a user.
    vi.mocked(commands.connectSavedPlace).mockResolvedValueOnce({
      status: 'error',
      error: { reason: 'already_connected', volumeId: 'sftp-nas-local-22-ada' },
    })
    await expect(connectSavedPlace('sftp-nas-local-22-ada', 'attempt-3')).rejects.toThrow()
  })

  it('passes an add-mode target through whole', async () => {
    vi.mocked(commands.connectServer).mockResolvedValueOnce({ outcome: 'needs_credentials' })
    await connectServer(target, 'attempt-4')
    expect(commands.connectServer).toHaveBeenCalledWith(target, 'attempt-4', null)
  })

  it('mints a fresh attempt id every time', () => {
    expect(newServerAttemptId()).not.toBe(newServerAttemptId())
    expect(newServerAttemptId().startsWith('server-connect-')).toBe(true)
  })

  it('cancels by the same id the dial was given', async () => {
    vi.mocked(commands.cancelServerConnect).mockResolvedValueOnce(true)
    expect(await cancelServerConnect('attempt-1')).toBe(true)
    expect(commands.cancelServerConnect).toHaveBeenCalledWith('attempt-1')
  })
})

describe('the saved list', () => {
  it('reads it whole', async () => {
    vi.mocked(commands.listSavedServers).mockResolvedValueOnce([])
    expect(await listSavedServers()).toEqual([])
  })

  it('keeps the pin and the two forgets apart', async () => {
    vi.mocked(commands.setPlacePinned).mockResolvedValueOnce(true)
    vi.mocked(commands.forgetServer).mockResolvedValueOnce(true)
    vi.mocked(commands.forgetServerSecret).mockResolvedValueOnce(true)
    vi.mocked(commands.hasServerSecret).mockResolvedValueOnce(true)
    vi.mocked(commands.disconnectPlace).mockResolvedValueOnce(false)

    await setPlacePinned('sftp-nas-local-22-ada', false)
    await forgetServer('sftp-nas-local-22-ada')
    await forgetServerSecret('sftp-nas-local-22-ada')
    expect(await hasServerSecret('sftp-nas-local-22-ada')).toBe(true)
    // `false` is a disconnect racing a dropped session, not a problem.
    expect(await disconnectPlace('sftp-nas-local-22-ada')).toBe(false)

    expect(commands.setPlacePinned).toHaveBeenCalledWith('sftp-nas-local-22-ada', false)
    expect(commands.forgetServer).toHaveBeenCalledWith('sftp-nas-local-22-ada')
    expect(commands.forgetServerSecret).toHaveBeenCalledWith('sftp-nas-local-22-ada')
  })

  it('edits a server through the same shape the add sheet collects', async () => {
    vi.mocked(commands.updateSavedServer).mockResolvedValueOnce(undefined)
    await updateSavedServer(target)
    expect(commands.updateSavedServer).toHaveBeenCalledWith(target)
  })
})
