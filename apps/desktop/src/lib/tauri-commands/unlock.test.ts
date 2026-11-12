/**
 * Tests for the unlock Tauri command wrappers.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: {
    unlockContainer: vi.fn(),
    forgetContainerPassword: vi.fn(),
    forgetSavedContainerPassword: vi.fn(),
    cancelContainerUnlock: vi.fn(),
    notifyArchivePasswordPrompt: vi.fn(),
    notifyArchivePasswordDismissed: vi.fn(),
  },
}))

import { commands } from '#lib/ipc/bindings.js'
import { failureOf } from '#lib/ipc/typed-failure.js'
import {
  setArchivePassword,
  unlockContainer,
  cancelContainerUnlock,
  forgetContainerPassword,
  forgetSavedContainerPassword,
  UnlockFailure,
  notifyArchivePasswordPrompt,
  notifyArchivePasswordDismissed,
} from './unlock'

beforeEach(() => {
  vi.clearAllMocks()
})

describe('setArchivePassword wrapper', () => {
  it('forwards the volume id, archive path, and password, remembering nothing', async () => {
    vi.mocked(commands.unlockContainer).mockResolvedValueOnce({ status: 'ok', data: 'stored' })
    await setArchivePassword('root', '/a/secret.zip/inner/x.pdf', 'hunter2')
    expect(commands.unlockContainer).toHaveBeenCalledWith(
      'root',
      '/a/secret.zip/inner/x.pdf',
      'hunter2',
      false,
      expect.any(String),
    )
  })

  it('throws the typed reason on an error result', async () => {
    vi.mocked(commands.unlockContainer).mockResolvedValueOnce({ status: 'error', error: { type: 'notLocked' } })
    await expect(setArchivePassword('root', '/a/plain.txt', 'x')).rejects.toThrow('notLocked')
  })
})

describe('unlockContainer wrapper', () => {
  const request = {
    parentVolumeId: 'root',
    path: '/b/test-restic/Browse snapshots',
    password: 'hunter2',
    remember: true,
    unlockId: 'unlock-1',
  }

  it('forwards every field in order and answers the typed outcome', async () => {
    vi.mocked(commands.unlockContainer).mockResolvedValueOnce({ status: 'ok', data: 'wrongPassword' })
    await expect(unlockContainer(request)).resolves.toBe('wrongPassword')
    expect(commands.unlockContainer).toHaveBeenCalledWith(
      'root',
      '/b/test-restic/Browse snapshots',
      'hunter2',
      true,
      'unlock-1',
    )
  })

  it('throws the refusal typed, so the dialog can tell a cancel from an unreachable repo', async () => {
    vi.mocked(commands.unlockContainer).mockResolvedValueOnce({ status: 'error', error: { type: 'timedOut' } })
    const thrown: unknown = await unlockContainer(request).catch((e: unknown) => e)
    expect(failureOf(UnlockFailure, thrown)).toEqual({ type: 'timedOut' })
  })
})

describe('cancelContainerUnlock wrapper', () => {
  it('names the unlock to drop', async () => {
    vi.mocked(commands.cancelContainerUnlock).mockResolvedValueOnce(undefined)
    await cancelContainerUnlock('unlock-1')
    expect(commands.cancelContainerUnlock).toHaveBeenCalledWith('unlock-1')
  })
})

describe('forgetContainerPassword wrapper', () => {
  it('forwards the volume id and container path', async () => {
    vi.mocked(commands.forgetContainerPassword).mockResolvedValueOnce({ status: 'ok', data: null })
    await forgetContainerPassword('root', '/a/secret.zip')
    expect(commands.forgetContainerPassword).toHaveBeenCalledWith('root', '/a/secret.zip')
  })

  it('throws the typed reason on an error result', async () => {
    vi.mocked(commands.forgetContainerPassword).mockResolvedValueOnce({
      status: 'error',
      error: { type: 'volumeNotFound', volumeId: 'root' },
    })
    const thrown: unknown = await forgetContainerPassword('root', '/a/secret.zip').catch((e: unknown) => e)
    expect(failureOf(UnlockFailure, thrown)).toEqual({ type: 'volumeNotFound', volumeId: 'root' })
  })
})

describe('forgetSavedContainerPassword wrapper', () => {
  it('forwards the volume id and path, and answers what the Keychain did', async () => {
    vi.mocked(commands.forgetSavedContainerPassword).mockResolvedValueOnce({ status: 'ok', data: 'storeKeptIt' })
    const answer = await forgetSavedContainerPassword('root', '/b/repo/Browse snapshots')
    expect(answer).toBe('storeKeptIt')
    expect(commands.forgetSavedContainerPassword).toHaveBeenCalledWith('root', '/b/repo/Browse snapshots')
  })

  it('throws the typed reason on an error result', async () => {
    vi.mocked(commands.forgetSavedContainerPassword).mockResolvedValueOnce({
      status: 'error',
      error: { type: 'notLocked' },
    })
    const thrown: unknown = await forgetSavedContainerPassword('root', '/b').catch((e: unknown) => e)
    expect(failureOf(UnlockFailure, thrown)).toEqual({ type: 'notLocked' })
  })
})

describe('password prompt mirror', () => {
  it('forwards what the prompt is asking, and nothing more', async () => {
    // The mirror is what lets `cmdr://state` name the container. ❌ There is no
    // password field on it, and there must never be one: the secret's only path
    // is the unlock call (or, over MCP, the backend's own store).
    const prompt = {
      archiveName: 'secret.zip',
      archivePath: '/a/secret.zip/inner/x.pdf',
      parentVolumeId: 'root',
      container: 'archive' as const,
      mode: 'transfer' as const,
      wrongAttempt: true,
      operationId: 'op-3',
    }
    await notifyArchivePasswordPrompt(prompt)
    expect(commands.notifyArchivePasswordPrompt).toHaveBeenCalledWith(prompt)
    expect(JSON.stringify(prompt)).not.toContain('password')
  })

  it('clears the mirror when the prompt goes', async () => {
    await notifyArchivePasswordDismissed()
    expect(commands.notifyArchivePasswordDismissed).toHaveBeenCalledOnce()
  })
})
