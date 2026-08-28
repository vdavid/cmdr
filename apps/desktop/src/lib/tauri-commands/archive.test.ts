/**
 * Tests for the archive-password Tauri command wrappers.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    setArchivePassword: vi.fn(),
    clearArchivePassword: vi.fn(),
    notifyArchivePasswordPrompt: vi.fn(),
    notifyArchivePasswordDismissed: vi.fn(),
  },
}))

import { commands } from '$lib/ipc/bindings'
import {
  setArchivePassword,
  clearArchivePassword,
  notifyArchivePasswordPrompt,
  notifyArchivePasswordDismissed,
} from './archive'

describe('setArchivePassword wrapper', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('forwards the volume id, archive path, and password', async () => {
    vi.mocked(commands.setArchivePassword).mockResolvedValueOnce({ status: 'ok', data: null })
    await setArchivePassword('root', '/a/secret.zip/inner/x.pdf', 'hunter2')
    expect(commands.setArchivePassword).toHaveBeenCalledWith('root', '/a/secret.zip/inner/x.pdf', 'hunter2')
  })

  it('throws the backend message on an error result', async () => {
    vi.mocked(commands.setArchivePassword).mockResolvedValueOnce({ status: 'error', error: 'not an archive' })
    await expect(setArchivePassword('root', '/a/plain.txt', 'x')).rejects.toThrow('not an archive')
  })
})

describe('clearArchivePassword wrapper', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('forwards the volume id and archive path', async () => {
    vi.mocked(commands.clearArchivePassword).mockResolvedValueOnce({ status: 'ok', data: null })
    await clearArchivePassword('root', '/a/secret.zip')
    expect(commands.clearArchivePassword).toHaveBeenCalledWith('root', '/a/secret.zip')
  })

  it('throws the backend message on an error result', async () => {
    vi.mocked(commands.clearArchivePassword).mockResolvedValueOnce({ status: 'error', error: 'gone' })
    await expect(clearArchivePassword('root', '/a/secret.zip')).rejects.toThrow('gone')
  })
})

describe('archive-password prompt mirror', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('forwards what the prompt is asking, and nothing more', async () => {
    // The mirror is what lets `cmdr://state` name the archive. ❌ There is no
    // password field on it, and there must never be one: the secret's only path
    // is `setArchivePassword` (or, over MCP, the backend's own store).
    const prompt = {
      archiveName: 'secret.zip',
      archivePath: '/a/secret.zip/inner/x.pdf',
      parentVolumeId: 'root',
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
