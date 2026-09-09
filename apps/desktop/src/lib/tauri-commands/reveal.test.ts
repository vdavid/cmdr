/**
 * Tests for the "Reveal in Cmdr" command wrappers.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    drainPendingReveals: vi.fn(),
  },
}))

import { commands } from '$lib/ipc/bindings'
import { drainPendingReveals } from './reveal'

describe('drainPendingReveals wrapper', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('tells the backend this window is listening', async () => {
    vi.mocked(commands.drainPendingReveals).mockResolvedValueOnce(undefined)
    await drainPendingReveals()
    expect(commands.drainPendingReveals).toHaveBeenCalledOnce()
  })

  it('stays quiet where the command does not exist', async () => {
    // The whole mechanism is macOS-only, so the command is absent on Linux. Window
    // startup calls this unconditionally and must not care.
    vi.mocked(commands.drainPendingReveals).mockRejectedValueOnce(new Error('not allowed'))
    await expect(drainPendingReveals()).resolves.toBeUndefined()
  })
})
