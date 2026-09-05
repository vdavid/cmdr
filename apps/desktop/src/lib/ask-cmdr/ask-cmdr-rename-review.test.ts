/**
 * The rename review's state slice: a turn's `proposalReady` events stage plans, the end of the
 * turn opens ONE review over all of them, the pane watcher and each user decision revalidate it,
 * an edited name is taken from the SERVER, and Apply sends opaque row ids only. Split from
 * `ask-cmdr-trigger.test.ts` (the streaming state machine) because this is a guardrail surface
 * with its own failure modes.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { AskCmdrSendOutcome, AskCmdrStreamEvent } from '$lib/tauri-commands'

const sendMock = vi.fn<(c: number | null, t: string, a: unknown[], d: string[]) => Promise<AskCmdrSendOutcome>>()
const preflightRenameMock = vi.fn<(...args: unknown[]) => Promise<unknown>>()
const reviseRenameMock = vi.fn<(...args: unknown[]) => Promise<unknown>>()
const applyRenameMock = vi.fn<(...args: unknown[]) => Promise<unknown>>()
const cancelProposalMock = vi.fn<(proposalId: string) => Promise<void>>()
const undoOperationsMock = vi.fn<(operationIds: string[]) => Promise<unknown>>()

vi.mock('$lib/tauri-commands', () => ({
  sendAskCmdrMessage: (c: number | null, t: string, a: unknown[], d: string[]) => sendMock(c, t, a, d),
  cancelAskCmdr: vi.fn(() => Promise.resolve()),
  listAskCmdrConversations: vi.fn(() => Promise.resolve([])),
  getAskCmdrConversation: vi.fn(() => Promise.resolve(null)),
  recordAskCmdrModelChange: vi.fn(() => Promise.resolve(null)),
  preflightBulkRename: (...args: unknown[]) => preflightRenameMock(...args),
  cancelBulkRenameProposal: (proposalId: string) => cancelProposalMock(proposalId),
  applyBulkRename: (...args: unknown[]) => applyRenameMock(...args),
  reviseBulkRenameRow: (...args: unknown[]) => reviseRenameMock(...args),
  undoOperations: (operationIds: string[]) => undoOperationsMock(operationIds),
}))
vi.mock('$lib/app-status-store', () => ({ saveAppStatus: vi.fn() }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))
vi.mock('$lib/file-explorer/pane/explorer-state.svelte', () => ({
  explorerState: { setRailFocused: vi.fn() },
}))
vi.mock('./rail-window', () => ({
  growMainWindowForRail: vi.fn(() => Promise.resolve()),
  shrinkMainWindowForRail: vi.fn(() => Promise.resolve()),
}))
vi.mock('./ask-cmdr-consent.svelte', () => ({
  consentState: { accepted: true, acceptedAt: null },
  refreshConsent: vi.fn(() => Promise.resolve()),
}))

import {
  applyRenameReview,
  askCmdrState,
  cancelRenameReview,
  newChat,
  renameReviewListingChanged,
  reviseRenameRow,
  sendMessage,
  setRenameRowAllowed,
} from './ask-cmdr-trigger.svelte'
import { handleTurnEvent, resetStoppedTurns } from './ask-cmdr-stream.svelte'

/** The thread a fired event belongs to when the test doesn't name one. */
const THREAD_ID = 1

/** Feed one stream event in as the backend emits it, for the thread the rail is on. A rail with
 * no thread yet is put on the named one first: production gets there through `started`, which
 * `adopts a new thread's id from its own started event` covers on its own. */
function fire(event: AskCmdrStreamEvent, conversationId: number = askCmdrState.conversationId ?? THREAD_ID): void {
  askCmdrState.conversationId = conversationId
  handleTurnEvent({ conversationId, event })
}

beforeEach(() => {
  sendMock.mockReset()
  sendMock.mockImplementation((c) => Promise.resolve({ accepted: true, conversationId: c ?? THREAD_ID }))
  preflightRenameMock.mockReset()
  preflightRenameMock.mockResolvedValue({ status: 'ready', rows: [] })
  reviseRenameMock.mockReset()
  applyRenameMock.mockReset()
  applyRenameMock.mockResolvedValue({ operationId: 'op-1' })
  cancelProposalMock.mockReset()
  cancelProposalMock.mockResolvedValue()
  undoOperationsMock.mockReset()
  newChat()
  resetStoppedTurns()
  askCmdrState.messages = []
  askCmdrState.conversationId = null
})

/** The row shape a review carries, trimmed to what these tests drive. */
function proposalRow(rowId: string, sourceName: string, destinationName: string, folder = '/shots') {
  return {
    rowId,
    sourceName,
    destinationName,
    sourcePath: `${folder}/${sourceName}`,
    volumeId: 'root',
    evidence: { source: 'filename' as const, detail: sourceName },
    coverage: null,
  }
}

/** Stage one batch the way the backend does: the plan is staged, the dialog waits. */
function stageBatch(proposalId: string, rows: ReturnType<typeof proposalRow>[]): void {
  fire({ type: 'proposalReady', proposal: { proposalId, rows } })
}

/** End the streaming turn, which is what opens the review over everything staged. */
function finishTurn(): void {
  fire({ type: 'done', messageId: 1, seq: 1, stop: 'completed', usage: { promptTokens: 1, completionTokens: 1 } })
}

/** The first row on show, whichever batch staged it. */
function firstReviewRow() {
  return askCmdrState.renameReview?.proposals[0]?.rows[0]
}

/** Wait until every batch on show has a settled preflight: Apply no-ops while one is in flight. */
async function reviewSettles(batches: number): Promise<void> {
  await vi.waitFor(() => {
    expect(preflightRenameMock).toHaveBeenCalledTimes(batches)
    expect(askCmdrState.renameReview?.proposals.every((proposal) => !proposal.preflighting)).toBe(true)
  })
}

/** Every row on show, in dialog order, whichever batch staged it. */
function reviewRowIds(): string[] {
  return askCmdrState.renameReview?.proposals.flatMap((proposal) => proposal.rows.map((row) => row.rowId)) ?? []
}

/**
 * One review, however many batches the model needed.
 *
 * The model can only emit about 101 plan rows per reply, so a big job arrives as a run of
 * `propose_rename_plan` calls inside one turn. Each used to replace the review under the
 * user's cursor — cancelling the plan they were reading — so they were asked to make one
 * decision several times, on the operation where careful review matters most.
 */
describe('one review for one job', () => {
  it('adds a second batch to the review instead of destroying the first', async () => {
    sendMessage('rename all of them')

    stageBatch('proposal-1', [proposalRow('row-1', 'a.png', 'klarna-invoice.png')])
    stageBatch('proposal-2', [proposalRow('row-2', 'b.png', 'klarna-receipt.png')])

    // Nothing to answer yet: the model is still working through the job.
    expect(askCmdrState.renameReview).toBeNull()

    finishTurn()

    await reviewSettles(2)
    expect(askCmdrState.renameReview?.proposals.map((proposal) => proposal.proposalId)).toEqual([
      'proposal-1',
      'proposal-2',
    ])
    expect(reviewRowIds()).toEqual(['row-1', 'row-2'])
    // The first batch is still staged on the spine: nothing cancelled it to make room.
    expect(cancelProposalMock).not.toHaveBeenCalled()
    // Each batch is preflighted as itself; the backend knows nothing about the grouping.
    expect(preflightRenameMock).toHaveBeenCalledWith('proposal-1', ['row-1'])
    expect(preflightRenameMock).toHaveBeenCalledWith('proposal-2', ['row-2'])
  })

  it('starts one operation per batch, in the order they were staged', async () => {
    applyRenameMock.mockResolvedValueOnce({ operationId: 'op-1' }).mockResolvedValueOnce({ operationId: 'op-2' })
    sendMessage('rename all of them')
    stageBatch('proposal-1', [proposalRow('row-1', 'a.png', 'invoice-a.png')])
    stageBatch('proposal-2', [proposalRow('row-2', 'b.png', 'invoice-b.png', '/receipts')])
    finishTurn()
    await reviewSettles(2)

    await applyRenameReview()

    expect(applyRenameMock.mock.calls).toEqual([
      ['proposal-1', ['row-1']],
      ['proposal-2', ['row-2']],
    ])
    // Every started operation reaches the thread, and the review is over.
    const applied = askCmdrState.messages.filter((message) => message.kind === 'renameApplied')
    expect(applied.map((line) => line.operationId)).toEqual(['op-1', 'op-2'])
    expect(askCmdrState.renameReview).toBeNull()
  })

  it('never sends a row the user turned down, whichever batch staged it', async () => {
    sendMessage('rename all of them')
    stageBatch('proposal-1', [
      proposalRow('row-1', 'a.png', 'invoice-a.png'),
      proposalRow('row-2', 'b.png', 'invoice-b.png'),
    ])
    stageBatch('proposal-2', [
      proposalRow('row-3', 'c.png', 'invoice-c.png'),
      proposalRow('row-4', 'd.png', 'invoice-d.png'),
    ])
    finishTurn()
    await reviewSettles(2)

    // One turned down in each batch, so neither a first-batch nor a last-batch denial can ride.
    setRenameRowAllowed('proposal-1', 'row-2', false)
    setRenameRowAllowed('proposal-2', 'row-3', false)
    await reviewSettles(4)
    await applyRenameReview()

    expect(applyRenameMock.mock.calls).toEqual([
      ['proposal-1', ['row-1']],
      ['proposal-2', ['row-4']],
    ])
    // And both denied names ride the next send as feedback, not just the last batch's.
    finishTurn()
    sendMessage('try again')
    expect(sendMock).toHaveBeenLastCalledWith(expect.anything(), 'try again', [], ['invoice-b.png', 'invoice-c.png'])
  })
})

describe('carrying denials into the next message', () => {
  async function openTwoRowReview(): Promise<void> {
    sendMessage('rename these')
    stageBatch('proposal-1', [
      proposalRow('row-1', 'a.png', 'klarna-invoice.png'),
      proposalRow('row-2', 'b.png', 'klarna-receipt.png'),
    ])
    finishTurn()
    await reviewSettles(1)
  }

  it('sends the names the user denied with the next message, then forgets them', async () => {
    await openTwoRowReview()
    // The user keeps the first name and turns down the second.
    setRenameRowAllowed('proposal-1', 'row-2', false)
    await reviewSettles(2)
    await applyRenameReview()
    finishTurn()

    sendMessage('try again')

    // The rejected name rides the next send; the accepted one does not.
    expect(sendMock).toHaveBeenLastCalledWith(expect.anything(), 'try again', [], ['klarna-receipt.png'])

    // And they are feedback on one decision, not a permanent denylist.
    finishTurn()
    sendMessage('and again')
    expect(sendMock).toHaveBeenLastCalledWith(expect.anything(), 'and again', [], [])
  })

  it('treats cancelling the whole review as turning every name down', async () => {
    await openTwoRowReview()
    cancelRenameReview()
    finishTurn()

    sendMessage('different style please')

    expect(sendMock).toHaveBeenLastCalledWith(
      expect.anything(),
      'different style please',
      [],
      ['klarna-invoice.png', 'klarna-receipt.png'],
    )
  })

  it('carries no denials when the user accepted every row', async () => {
    await openTwoRowReview()
    await applyRenameReview()
    finishTurn()

    sendMessage('now the rest')

    expect(sendMock).toHaveBeenLastCalledWith(expect.anything(), 'now the rest', [], [])
  })
})

describe('rename review listing updates', () => {
  it('rechecks a proposed target when the pane watcher reports it appeared', async () => {
    preflightRenameMock.mockResolvedValue({
      status: 'blocked',
      rows: [{ rowId: 'row-1', status: 'blocked', reason: 'targetExists' }],
    })
    sendMessage('rename it')
    stageBatch('proposal-1', [proposalRow('row-1', 'before.png', 'after.png')])
    finishTurn()
    await vi.waitFor(() => {
      expect(preflightRenameMock).toHaveBeenCalledTimes(1)
    })
    preflightRenameMock.mockClear()

    await renameReviewListingChanged([{ type: 'add', entry: { name: 'after.png' } }])

    await vi.waitFor(() => {
      expect(preflightRenameMock).toHaveBeenCalledWith('proposal-1', ['row-1'])
    })
    expect(firstReviewRow()).toMatchObject({
      allowed: false,
      blockedReason: 'targetExists',
    })

    preflightRenameMock.mockResolvedValue({
      status: 'ready',
      rows: [{ rowId: 'row-1', status: 'ready', reason: null, warnings: [] }],
    })
    await renameReviewListingChanged([{ type: 'remove', entry: { name: 'after.png' } }])

    await vi.waitFor(() => {
      expect(firstReviewRow()?.blockedReason).toBeNull()
    })
    expect(firstReviewRow()?.allowed).toBe(false)
  })

  it('ignores watcher changes unrelated to the reviewed names', async () => {
    sendMessage('rename it')
    stageBatch('proposal-1', [proposalRow('row-1', 'before.png', 'after.png')])
    finishTurn()
    await vi.waitFor(() => {
      expect(preflightRenameMock).toHaveBeenCalledTimes(1)
    })
    preflightRenameMock.mockClear()

    await renameReviewListingChanged([{ type: 'modify', entry: { name: 'other.png' } }])

    await Promise.resolve()
    expect(preflightRenameMock).not.toHaveBeenCalled()
  })

  it('deselects a missing source and rechecks it when the pane watcher reports its return', async () => {
    preflightRenameMock.mockResolvedValue({
      status: 'blocked',
      rows: [{ rowId: 'row-1', status: 'blocked', reason: 'sourceMissing', warnings: [] }],
    })
    sendMessage('rename it')
    stageBatch('proposal-1', [proposalRow('row-1', 'before.png', 'after.png')])
    finishTurn()

    await vi.waitFor(() => {
      expect(firstReviewRow()).toMatchObject({
        allowed: false,
        blockedReason: 'sourceMissing',
      })
    })
    preflightRenameMock.mockResolvedValue({
      status: 'ready',
      rows: [{ rowId: 'row-1', status: 'ready', reason: null, warnings: [] }],
    })
    preflightRenameMock.mockClear()

    await renameReviewListingChanged([{ type: 'add', entry: { name: 'before.png' } }])

    await vi.waitFor(() => {
      expect(firstReviewRow()?.blockedReason).toBeNull()
    })
    expect(preflightRenameMock).toHaveBeenCalledWith('proposal-1', ['row-1'])
    expect(firstReviewRow()?.allowed).toBe(false)
  })
})

describe('revising a proposed name', () => {
  /** Stage a one-row review the way a real `proposalReady` event does. */
  async function openOneRowReview(): Promise<void> {
    sendMessage('rename it')
    fire({
      type: 'proposalReady',
      proposal: {
        proposalId: 'proposal-1',
        rows: [
          {
            ...proposalRow('row-1', 'before.png', 'Klarna invoice.png'),
            evidence: { source: 'imageText' as const, detail: 'payment confirmation' },
            coverage: {
              matchOffset: 21,
              matchedChars: 20,
              deliveredChars: 61,
              contextBefore: 'Klarna ',
              matchedText: 'payment confirmation',
              contextAfter: ' 1,299 SEK',
              trimmedBefore: false,
              trimmedAfter: false,
            },
          },
        ],
      },
    })
    finishTurn()
    await vi.waitFor(() => {
      expect(preflightRenameMock).toHaveBeenCalledTimes(1)
    })
    preflightRenameMock.mockClear()
  }

  /**
   * The round trip M2 exists for: the user retypes a wrong name, the row takes the SERVER's
   * answer (the name, and evidence that no longer credits the model), a fresh preflight runs
   * because the edit invalidated the accepted one, and Apply then sends opaque row ids only.
   */
  it('takes the server’s revised row, re-preflights, and applies', async () => {
    await openOneRowReview()
    reviseRenameMock.mockResolvedValue({
      rowId: 'row-1',
      sourceName: 'before.png',
      destinationName: 'Klarna payment confirmation 2026-07-24.png',
      sourcePath: '/shots/before.png',
      volumeId: 'root',
      evidence: { source: 'userEdited', detail: '' },
      coverage: null,
    })

    await reviseRenameRow('proposal-1', 'row-1', 'Klarna payment confirmation 2026-07-24.png')

    expect(reviseRenameMock).toHaveBeenCalledWith('proposal-1', 'row-1', 'Klarna payment confirmation 2026-07-24.png')
    expect(firstReviewRow()).toMatchObject({
      destinationName: 'Klarna payment confirmation 2026-07-24.png',
      evidence: { source: 'userEdited', detail: '' },
      coverage: null,
      nameRejected: false,
    })
    // The edit cleared the backend's accepted preflight, so the review has to earn a new one.
    expect(preflightRenameMock).toHaveBeenCalledWith('proposal-1', ['row-1'])

    await applyRenameReview()

    expect(applyRenameMock).toHaveBeenCalledWith('proposal-1', ['row-1'])
    expect(askCmdrState.renameReview).toBeNull()
  })

  /** A name the server won't take leaves the row on the name it had, and says so on the row. */
  it('keeps the row’s name when the server refuses the typed one', async () => {
    await openOneRowReview()
    reviseRenameMock.mockRejectedValue(new Error('Each destinationName must be one filename, not a path.'))

    await reviseRenameRow('proposal-1', 'row-1', 'folder/name.png')

    expect(firstReviewRow()).toMatchObject({
      destinationName: 'Klarna invoice.png',
      nameRejected: true,
    })
    expect(preflightRenameMock).not.toHaveBeenCalled()
  })

  /** A blur with nothing typed is not an edit: no IPC, and no evidence swapped for one. */
  it('sends nothing when the name did not change', async () => {
    await openOneRowReview()

    await reviseRenameRow('proposal-1', 'row-1', 'Klarna invoice.png')

    expect(reviseRenameMock).not.toHaveBeenCalled()
    expect(firstReviewRow()?.evidence.source).toBe('imageText')
  })
})
