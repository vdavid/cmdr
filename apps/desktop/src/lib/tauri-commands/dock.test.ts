/**
 * The Dock wrappers, which exist to make a backend that isn't answering look
 * like a typed refusal rather than a throw: the callers are a startup gate and a
 * toast button, and neither has anywhere to put an exception.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    getDockPinState: vi.fn(),
    addCmdrToDock: vi.fn(),
  },
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import { commands } from '$lib/ipc/bindings'
import { getDockPinState, addCmdrToDock } from './dock'

beforeEach(() => {
  vi.clearAllMocks()
})

describe('getDockPinState', () => {
  it('passes the backend answer straight through', async () => {
    vi.mocked(commands.getDockPinState).mockResolvedValueOnce({ kind: 'offerable' })

    expect(await getDockPinState()).toEqual({ kind: 'offerable' })
  })

  it('reads an unreachable backend as "we could not look", which keeps the nudge quiet', async () => {
    vi.mocked(commands.getDockPinState).mockRejectedValueOnce(new Error('IPC down'))

    expect(await getDockPinState()).toEqual({ kind: 'unavailable', reason: 'preferencesUnreadable' })
  })
})

describe('addCmdrToDock', () => {
  it('answers null when the tile lands', async () => {
    vi.mocked(commands.addCmdrToDock).mockResolvedValueOnce({ status: 'ok', data: null })

    expect(await addCmdrToDock()).toBeNull()
  })

  it('hands back the typed refusal', async () => {
    vi.mocked(commands.addCmdrToDock).mockResolvedValueOnce({
      status: 'error',
      error: { kind: 'blocked', reason: 'managedDock' },
    })

    expect(await addCmdrToDock()).toEqual({ kind: 'blocked', reason: 'managedDock' })
  })

  /** No answer at all means the same thing to the caller as the deadline does. */
  it('reads a throw as a timeout rather than letting it escape', async () => {
    vi.mocked(commands.addCmdrToDock).mockRejectedValueOnce(new Error('IPC down'))

    expect(await addCmdrToDock()).toEqual({ kind: 'timedOut' })
  })
})
