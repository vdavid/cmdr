/**
 * Tests for the send-feedback Tauri command wrapper.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    sendFeedback: vi.fn(),
  },
}))

import { commands } from '$lib/ipc/bindings'
import { sendFeedback } from './feedback'

describe('sendFeedback wrapper', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('forwards the text and email and returns the typed result', async () => {
    vi.mocked(commands.sendFeedback).mockResolvedValueOnce({ kind: 'sent' })
    const result = await sendFeedback('great app', 'tester@example.com')
    expect(commands.sendFeedback).toHaveBeenCalledWith('great app', 'tester@example.com')
    expect(result).toEqual({ kind: 'sent' })
  })

  it('coerces an omitted email to null (Rust Option::None)', async () => {
    vi.mocked(commands.sendFeedback).mockResolvedValueOnce({ kind: 'sent' })
    await sendFeedback('great app')
    expect(commands.sendFeedback).toHaveBeenCalledWith('great app', null)
  })

  it('passes through a soft failure result', async () => {
    vi.mocked(commands.sendFeedback).mockResolvedValueOnce({ kind: 'softFailure' })
    expect(await sendFeedback('great app')).toEqual({ kind: 'softFailure' })
  })

  it('degrades to softFailure when the command throws', async () => {
    vi.mocked(commands.sendFeedback).mockRejectedValueOnce(new Error('IPC down'))
    expect(await sendFeedback('great app')).toEqual({ kind: 'softFailure' })
  })
})
