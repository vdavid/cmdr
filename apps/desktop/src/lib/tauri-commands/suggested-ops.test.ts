/**
 * The Suggested ops command wrappers: thin pass-throughs over the typed `commands.*` bindings
 * that turn a typed error result into a throw, so no caller has to remember the check.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    suggestedOpsList: vi.fn(),
    suggestedOpsPage: vi.fn(),
    suggestedOpsReject: vi.fn(),
  },
}))

import { commands } from '$lib/ipc/bindings'
import { listSuggestedOps, pageSuggestedOps, rejectSuggestedGroup } from './suggested-ops'

beforeEach(() => {
  vi.clearAllMocks()
})

describe('listSuggestedOps', () => {
  it('returns the sweeps', async () => {
    const sweeps = [{ sweepId: 1, createdAt: 0, rationale: null, groups: [] }]
    vi.mocked(commands.suggestedOpsList).mockResolvedValueOnce({ status: 'ok', data: sweeps } as never)

    await expect(listSuggestedOps()).resolves.toBe(sweeps)
  })

  it('throws rather than answering an empty list when the read failed', async () => {
    vi.mocked(commands.suggestedOpsList).mockResolvedValueOnce({
      status: 'error',
      error: { message: 'no store' },
    } as never)

    await expect(listSuggestedOps()).rejects.toThrow()
  })
})

describe('pageSuggestedOps', () => {
  it('forwards the window it was asked for', async () => {
    const page = { ops: [], offset: 400, total: 60_000 }
    vi.mocked(commands.suggestedOpsPage).mockResolvedValueOnce({ status: 'ok', data: page } as never)

    await expect(pageSuggestedOps(7, 400, 200)).resolves.toBe(page)
    expect(commands.suggestedOpsPage).toHaveBeenCalledWith(7, 400, 200)
  })

  it('throws on a failed read', async () => {
    vi.mocked(commands.suggestedOpsPage).mockResolvedValueOnce({
      status: 'error',
      error: { message: 'gone' },
    } as never)

    await expect(pageSuggestedOps(7, 0, 200)).rejects.toThrow()
  })
})

describe('rejectSuggestedGroup', () => {
  it('answers what actually happened, so a group somebody already decided is not an error', async () => {
    vi.mocked(commands.suggestedOpsReject).mockResolvedValueOnce({
      status: 'ok',
      data: { kind: 'alreadyAnswered', found: 'approved' },
    } as never)

    await expect(rejectSuggestedGroup(7)).resolves.toEqual({ kind: 'alreadyAnswered', found: 'approved' })
  })

  it('throws when the call itself failed', async () => {
    vi.mocked(commands.suggestedOpsReject).mockResolvedValueOnce({
      status: 'error',
      error: { message: 'no store' },
    } as never)

    await expect(rejectSuggestedGroup(7)).rejects.toThrow()
  })
})
