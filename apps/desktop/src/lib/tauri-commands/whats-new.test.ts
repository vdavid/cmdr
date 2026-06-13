/**
 * Tests for the "What's new" Tauri command wrappers (thin pass-throughs over the typed
 * `commands.*` bindings).
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    getWhatsNew: vi.fn(),
    whatsNewDevOverride: vi.fn(),
  },
}))

import { commands } from '$lib/ipc/bindings'
import { getWhatsNew, whatsNewDevOverride } from './whats-new'

describe('getWhatsNew wrapper', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('forwards sinceVersion and max and returns the releases', async () => {
    const releases = [{ version: '0.26.0', date: '2026-06-11', lead: null, sections: [] }]
    vi.mocked(commands.getWhatsNew).mockResolvedValueOnce(releases)
    const result = await getWhatsNew('0.20.0', 5)
    expect(commands.getWhatsNew).toHaveBeenCalledWith('0.20.0', 5)
    expect(result).toBe(releases)
  })

  it('passes a null lower bound through unchanged', async () => {
    vi.mocked(commands.getWhatsNew).mockResolvedValueOnce([])
    await getWhatsNew(null, 1)
    expect(commands.getWhatsNew).toHaveBeenCalledWith(null, 1)
  })
})

describe('whatsNewDevOverride wrapper', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('returns the simulated version when the flag is set', async () => {
    vi.mocked(commands.whatsNewDevOverride).mockResolvedValueOnce('0.22.0')
    expect(await whatsNewDevOverride()).toBe('0.22.0')
  })

  it('returns null when the flag is unset', async () => {
    vi.mocked(commands.whatsNewDevOverride).mockResolvedValueOnce(null)
    expect(await whatsNewDevOverride()).toBeNull()
  })
})
