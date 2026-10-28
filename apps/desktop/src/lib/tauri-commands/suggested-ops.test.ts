/**
 * The Suggested ops command wrappers: thin pass-throughs over the typed `commands.*` bindings
 * that turn a typed error result into a throw, so no caller has to remember the check.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: {
    suggestedOpsList: vi.fn(),
    suggestedOpsPage: vi.fn(),
    suggestedOpsReject: vi.fn(),
    suggestedOpsApprove: vi.fn(),
    suggestedOpsApproveServer: vi.fn(),
    suggestedOpsLiveFacts: vi.fn(),
  },
  events: {
    suggestionsChanged: { listen: vi.fn() },
  },
}))

import { commands, events } from '#lib/ipc/bindings.js'
import {
  approveSuggestedGroup,
  approveSuggestedServer,
  listSuggestedOps,
  liveSuggestedOpFacts,
  onSuggestionsChanged,
  pageSuggestedOps,
  rejectSuggestedGroup,
} from './suggested-ops'

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

describe('a refusal the backend typed', () => {
  it("keeps the variant's detail, so the log line names the cause", async () => {
    vi.mocked(commands.suggestedOpsList).mockResolvedValueOnce({
      status: 'error',
      error: { type: 'store', detail: 'database is locked' },
    } as never)

    const error = await listSuggestedOps().catch((e: unknown) => e)

    expect(String(error)).toContain('store')
    expect(String(error)).toContain('database is locked')
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

describe('approveSuggestedGroup', () => {
  it('sends the selection and the fallback the review showed as given, and answers what happened', async () => {
    vi.mocked(commands.suggestedOpsApprove).mockResolvedValueOnce({
      status: 'ok',
      data: { kind: 'started', operationId: 'op-9' },
    } as never)

    await expect(approveSuggestedGroup(7, 3, { mode: 'only', opIds: [4, 9] }, 'permanentDelete')).resolves.toEqual({
      kind: 'started',
      operationId: 'op-9',
    })
    expect(commands.suggestedOpsApprove).toHaveBeenCalledWith(7, 3, { mode: 'only', opIds: [4, 9] }, 'permanentDelete')
  })

  it('passes a refusal through rather than throwing, since each kind means something different', async () => {
    vi.mocked(commands.suggestedOpsApprove).mockResolvedValueOnce({
      status: 'ok',
      data: { kind: 'listChanged' },
    } as never)

    await expect(approveSuggestedGroup(7, 3, { mode: 'allExcept', opIds: [] }, null)).resolves.toEqual({
      kind: 'listChanged',
    })
  })

  it('throws when the call itself failed', async () => {
    vi.mocked(commands.suggestedOpsApprove).mockResolvedValueOnce({
      status: 'error',
      error: { message: 'no store' },
    } as never)

    await expect(approveSuggestedGroup(7, 3, { mode: 'allExcept', opIds: [] }, null)).rejects.toThrow()
  })
})

describe('approveSuggestedServer', () => {
  it('sends the revision the user read and answers what happened, a refusal included', async () => {
    vi.mocked(commands.suggestedOpsApproveServer).mockResolvedValueOnce({
      status: 'ok',
      data: { kind: 'alreadySaved', serverId: 'sftp-1' },
    } as never)

    await expect(approveSuggestedServer(9, 2)).resolves.toEqual({ kind: 'alreadySaved', serverId: 'sftp-1' })
    expect(commands.suggestedOpsApproveServer).toHaveBeenCalledWith(9, 2)
  })

  it('throws when the save never answered, so the card is re-read rather than offered again', async () => {
    vi.mocked(commands.suggestedOpsApproveServer).mockResolvedValueOnce({
      status: 'error',
      error: { type: 'approvalDidntFinish' },
    } as never)

    await expect(approveSuggestedServer(9, 2)).rejects.toThrow()
  })
})

describe('onSuggestionsChanged', () => {
  it('hands the listener the payload, not the event envelope', async () => {
    const seen: unknown[] = []
    vi.mocked(events.suggestionsChanged.listen).mockImplementation(((cb: (e: unknown) => void) => {
      cb({
        event: 'suggestions-changed',
        id: 1,
        payload: { pendingGroupCount: 2, pendingOpCount: 8, groupId: null, reason: 'proposed' },
      })
      return Promise.resolve(() => undefined)
    }) as never)

    await onSuggestionsChanged((payload) => seen.push(payload))

    expect(seen).toEqual([{ pendingGroupCount: 2, pendingOpCount: 8, groupId: null, reason: 'proposed' }])
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

describe('liveSuggestedOpFacts', () => {
  it('answers the facts for the window it was asked for', async () => {
    const facts = [{ opId: 4, state: { kind: 'missing' } }]
    vi.mocked(commands.suggestedOpsLiveFacts).mockResolvedValueOnce({ status: 'ok', data: facts } as never)

    await expect(liveSuggestedOpFacts(7, 50, 50)).resolves.toEqual({ kind: 'facts', facts })
    expect(commands.suggestedOpsLiveFacts).toHaveBeenCalledWith(7, 50, 50)
  })

  it('passes a refusal through as a value, since the dialog words it once per group', async () => {
    vi.mocked(commands.suggestedOpsLiveFacts).mockResolvedValueOnce({
      status: 'error',
      error: { type: 'sourceNotConnected' },
    } as never)

    await expect(liveSuggestedOpFacts(7, 0, 50)).resolves.toEqual({
      kind: 'refused',
      reason: { type: 'sourceNotConnected' },
    })
  })
})
