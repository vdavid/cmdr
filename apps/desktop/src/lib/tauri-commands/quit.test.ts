/**
 * Tests for the quit-gate command wrappers and the `quit-requested` subscription.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { QuitRequested } from '$lib/ipc/bindings'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    quitConfirm: vi.fn(),
    quitCancel: vi.fn(),
  },
  events: {
    quitRequested: { listen: vi.fn() },
  },
}))

import { commands, events } from '$lib/ipc/bindings'
import { quitConfirm, quitCancel, onQuitRequested } from './quit'

beforeEach(() => {
  vi.clearAllMocks()
})

describe('quit-gate wrappers', () => {
  it('quitConfirm asks the backend to go ahead', async () => {
    await quitConfirm()
    expect(commands.quitConfirm).toHaveBeenCalledOnce()
    expect(commands.quitCancel).not.toHaveBeenCalled()
  })

  it('quitCancel calls the quit off', async () => {
    await quitCancel()
    expect(commands.quitCancel).toHaveBeenCalledOnce()
    expect(commands.quitConfirm).not.toHaveBeenCalled()
  })
})

describe('onQuitRequested', () => {
  it('unwraps the event payload and hands back the unlisten', async () => {
    const unlisten = vi.fn()
    let deliver: ((event: { payload: QuitRequested }) => void) | undefined
    vi.mocked(events.quitRequested.listen).mockImplementation((cb: unknown) => {
      deliver = cb as (event: { payload: QuitRequested }) => void
      return Promise.resolve(unlisten)
    })

    const seen: QuitRequested[] = []
    const stop = await onQuitRequested((event) => seen.push(event))

    const payload: QuitRequested = { operations: [], countdownMs: 15_000 }
    deliver?.({ payload })
    expect(seen).toEqual([payload])

    stop()
    expect(unlisten).toHaveBeenCalledOnce()
  })
})
