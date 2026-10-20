/**
 * Tests for the send-feedback Tauri command wrapper.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: {
    sendFeedback: vi.fn(),
  },
}))

import { commands } from '#lib/ipc/bindings.js'
import { sendFeedback } from './feedback'

describe('sendFeedback wrapper', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('forwards the text, email, and logs choice and returns the typed result', async () => {
    vi.mocked(commands.sendFeedback).mockResolvedValueOnce({ kind: 'sent' })
    const result = await sendFeedback('great app', 'tester@example.com', true)
    expect(commands.sendFeedback).toHaveBeenCalledWith('great app', 'tester@example.com', true)
    expect(result).toEqual({ kind: 'sent' })
  })

  it('coerces an omitted email to null (Rust Option::None)', async () => {
    vi.mocked(commands.sendFeedback).mockResolvedValueOnce({ kind: 'sent' })
    await sendFeedback('great app', undefined, false)
    expect(commands.sendFeedback).toHaveBeenCalledWith('great app', null, false)
  })

  it('passes through feedback that went without its logs', async () => {
    vi.mocked(commands.sendFeedback).mockResolvedValueOnce({ kind: 'sentWithoutLogs' })
    expect(await sendFeedback('it hung', undefined, true)).toEqual({ kind: 'sentWithoutLogs' })
  })

  it('passes through a soft failure result', async () => {
    vi.mocked(commands.sendFeedback).mockResolvedValueOnce({ kind: 'softFailure' })
    expect(await sendFeedback('great app', undefined, false)).toEqual({ kind: 'softFailure' })
  })

  it('degrades to softFailure when the command throws', async () => {
    vi.mocked(commands.sendFeedback).mockRejectedValueOnce(new Error('IPC down'))
    expect(await sendFeedback('great app', undefined, false)).toEqual({ kind: 'softFailure' })
  })
})
