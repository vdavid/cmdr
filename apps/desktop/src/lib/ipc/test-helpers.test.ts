/**
 * Smoke tests for the IPC mock harness itself. Uses `commands.hasFontMetrics` (a
 * side-effect-free lookup with a two-word arg) so the test exercises the harness,
 * not the command.
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
    ipc.mock('has_font_metrics', () => true)

    const result = await commands.hasFontMetrics('system-400-12')

    expect(result).toBe(true)
    expect(ipc.calls).toHaveLength(1)
    expect(ipc.calls[0]).toEqual({ command: 'has_font_metrics', payload: { fontId: 'system-400-12' } })
  })

  it('throws a clear error when no responder is registered', async () => {
    installIpcMock()

    // The default `__TAURI_INVOKE` rejects the promise; `hasFontMetrics` is not
    // wrapped in `typedError`, so the throw bubbles up directly.
    await expect(commands.hasFontMetrics('system-400-12')).rejects.toThrow(/unmocked command 'has_font_metrics'/)
  })

  it('clears the recorded calls between tests', async () => {
    const ipc = installIpcMock()
    ipc.mock('has_font_metrics', () => true)
    await commands.hasFontMetrics('a')
    expect(ipc.calls).toHaveLength(1)

    clearIpcMocks()
    // After clearMocks() the global invoke is removed; installing fresh restores it.
    const ipc2 = installIpcMock()
    ipc2.mock('has_font_metrics', () => false)
    await commands.hasFontMetrics('b')
    expect(ipc2.calls).toEqual([{ command: 'has_font_metrics', payload: { fontId: 'b' } }])
  })
})
