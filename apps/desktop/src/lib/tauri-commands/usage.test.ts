/**
 * The launch-day ledger wrapper. Its whole job is answering 0 rather than
 * throwing: a hint that stays quiet beats an error in someone's face.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    getLaunchDayCount: vi.fn(),
  },
}))

import { commands } from '$lib/ipc/bindings'
import { getLaunchDayCount } from './usage'

beforeEach(() => {
  vi.clearAllMocks()
})

describe('getLaunchDayCount', () => {
  it('passes the ledger count through', async () => {
    vi.mocked(commands.getLaunchDayCount).mockResolvedValueOnce(7)

    expect(await getLaunchDayCount()).toBe(7)
  })

  it('answers 0 when the ledger cannot be read, so a usage-gated hint stays silent', async () => {
    vi.mocked(commands.getLaunchDayCount).mockRejectedValueOnce(new Error('IPC down'))

    expect(await getLaunchDayCount()).toBe(0)
  })
})
