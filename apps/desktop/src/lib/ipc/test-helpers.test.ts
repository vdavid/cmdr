/**
 * Smoke tests for the IPC mock harness itself. Uses `commands.greet` (a trivial
 * pass-through) so the test exercises the harness, not the command.
 */

import { afterEach, describe, expect, it } from 'vitest'

import { commands } from '$lib/ipc/bindings'
import { clearIpcMocks, installIpcMock } from '$lib/ipc/test-helpers'

afterEach(() => {
  clearIpcMocks()
})

describe('installIpcMock', () => {
  it('captures the snake_case command name and the camelCase payload from a typed binding', async () => {
    const ipc = installIpcMock()
    ipc.mock('greet', () => 'hello, world')

    const result = await commands.greet('world')

    expect(result).toBe('hello, world')
    expect(ipc.calls).toHaveLength(1)
    expect(ipc.calls[0]).toEqual({ command: 'greet', payload: { name: 'world' } })
  })

  it('throws a clear error when no responder is registered', async () => {
    installIpcMock()

    // The default `__TAURI_INVOKE` rejects the promise; `greet` is not wrapped in
    // `typedError`, so the throw bubbles up directly.
    await expect(commands.greet('world')).rejects.toThrow(/unmocked command 'greet'/)
  })

  it('clears the recorded calls between tests', async () => {
    const ipc = installIpcMock()
    ipc.mock('greet', () => 'first')
    await commands.greet('a')
    expect(ipc.calls).toHaveLength(1)

    clearIpcMocks()
    // After clearMocks() the global invoke is removed; installing fresh restores it.
    const ipc2 = installIpcMock()
    ipc2.mock('greet', () => 'second')
    await commands.greet('b')
    expect(ipc2.calls).toEqual([{ command: 'greet', payload: { name: 'b' } }])
  })
})
