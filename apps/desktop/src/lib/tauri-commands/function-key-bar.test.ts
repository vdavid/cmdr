/**
 * Tests for the function-key-bar context-menu command wrapper and its
 * payload-free click event.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    showFunctionKeyBarContextMenu: vi.fn(),
  },
  events: {
    functionKeyBarHideRequested: { listen: vi.fn() },
  },
}))

import { commands, events } from '$lib/ipc/bindings'
import { showFunctionKeyBarContextMenu, onFunctionKeyBarHideRequested } from './function-key-bar'

beforeEach(() => {
  vi.clearAllMocks()
})

describe('showFunctionKeyBarContextMenu', () => {
  it('pops the menu and resolves on success', async () => {
    vi.mocked(commands.showFunctionKeyBarContextMenu).mockResolvedValueOnce({ status: 'ok', data: null })
    await expect(showFunctionKeyBarContextMenu()).resolves.toBeUndefined()
    expect(commands.showFunctionKeyBarContextMenu).toHaveBeenCalledOnce()
  })

  it('throws the backend error on failure', async () => {
    vi.mocked(commands.showFunctionKeyBarContextMenu).mockResolvedValueOnce({
      status: 'error',
      error: 'no window',
    })
    await expect(showFunctionKeyBarContextMenu()).rejects.toThrow('no window')
  })
})

describe('onFunctionKeyBarHideRequested', () => {
  it('fires on the payload-free event and hands back the unlisten', async () => {
    const unlisten = vi.fn()
    let deliver: (() => void) | undefined
    vi.mocked(events.functionKeyBarHideRequested.listen).mockImplementation((cb: unknown) => {
      deliver = cb as () => void
      return Promise.resolve(unlisten)
    })

    const handler = vi.fn()
    const stop = await onFunctionKeyBarHideRequested(handler)

    deliver?.()
    expect(handler).toHaveBeenCalledOnce()

    stop()
    expect(unlisten).toHaveBeenCalledOnce()
  })
})
