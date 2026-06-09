/**
 * Tests for the beta-signup Tauri command wrapper.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    betaSignup: vi.fn(),
  },
}))

import { commands } from '$lib/ipc/bindings'
import { betaSignup } from './beta-signup'

describe('betaSignup wrapper', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('forwards the email and returns the typed result', async () => {
    vi.mocked(commands.betaSignup).mockResolvedValueOnce({ kind: 'subscribed' })
    const result = await betaSignup('tester@example.com')
    expect(commands.betaSignup).toHaveBeenCalledWith('tester@example.com')
    expect(result).toEqual({ kind: 'subscribed' })
  })

  it('passes through a soft failure result', async () => {
    vi.mocked(commands.betaSignup).mockResolvedValueOnce({ kind: 'softFailure' })
    expect(await betaSignup('tester@example.com')).toEqual({ kind: 'softFailure' })
  })

  it('degrades to softFailure when the command throws', async () => {
    vi.mocked(commands.betaSignup).mockRejectedValueOnce(new Error('IPC down'))
    expect(await betaSignup('tester@example.com')).toEqual({ kind: 'softFailure' })
  })
})
