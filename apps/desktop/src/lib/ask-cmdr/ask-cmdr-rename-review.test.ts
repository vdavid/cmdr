/**
 * The rename review's state slice: a `proposalReady` event opens it, the pane watcher and each
 * user decision revalidate it, an edited name is taken from the SERVER, and Apply sends opaque
 * row ids only. Split from `ask-cmdr-trigger.test.ts` (the streaming state machine) because
 * this is a guardrail surface with its own failure modes.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { AskCmdrSendOutcome, AskCmdrStreamEvent } from '$lib/tauri-commands'

const sendMock = vi.fn<(c: number | null, t: string, a: unknown[], d: string[]) => Promise<AskCmdrSendOutcome>>()
const preflightRenameMock = vi.fn<(...args: unknown[]) => Promise<unknown>>()
const reviseRenameMock = vi.fn<(...args: unknown[]) => Promise<unknown>>()
const applyRenameMock = vi.fn<(...args: unknown[]) => Promise<unknown>>()

vi.mock('$lib/tauri-commands', () => ({
  sendAskCmdrMessage: (c: number | null, t: string, a: unknown[], d: string[]) => sendMock(c, t, a, d),
  cancelAskCmdr: vi.fn(() => Promise.resolve()),
  listAskCmdrConversations: vi.fn(() => Promise.resolve([])),
  getAskCmdrConversation: vi.fn(() => Promise.resolve(null)),
  recordAskCmdrModelChange: vi.fn(() => Promise.resolve(null)),
  preflightBulkRename: (...args: unknown[]) => preflightRenameMock(...args),
  cancelBulkRenameProposal: vi.fn(() => Promise.resolve()),
  applyBulkRename: (...args: unknown[]) => applyRenameMock(...args),
  reviseBulkRenameRow: (...args: unknown[]) => reviseRenameMock(...args),
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
  newChat()
  resetStoppedTurns()
  askCmdrState.messages = []
  askCmdrState.conversationId = null
})

describe('carrying denials into the next batch', () => {
  /** The row shape a review carries, trimmed to what these tests drive. */
  function proposalRow(rowId: string, sourceName: string, destinationName: string) {
    return {
      rowId,
      sourceName,
      destinationName,
      sourcePath: `/shots/${sourceName}`,
      volumeId: 'root',
      evidence: { source: 'filename' as const, detail: sourceName },
      coverage: null,
    }
  }

  /** End the streaming turn, so the next `sendMessage` isn't a no-op (single-flight). */
  function finishTurn(): void {
    fire({ type: 'done', messageId: 1, seq: 1, stop: 'completed', usage: { promptTokens: 1, completionTokens: 1 } })
  }

  async function openTwoRowReview(): Promise<void> {
    sendMessage('rename these')
    fire({
      type: 'proposalReady',
      proposal: {
        proposalId: 'proposal-1',
        rows: [
          proposalRow('row-1', 'a.png', 'klarna-invoice.png'),
          proposalRow('row-2', 'b.png', 'klarna-receipt.png'),
        ],
      },
    })
    // The proposal kicks off a preflight; apply no-ops while one is in flight.
    await vi.waitFor(() => {
      expect(preflightRenameMock).toHaveBeenCalledTimes(1)
    })
  }

  it('sends the names the user denied with the next message, then forgets them', async () => {
    await openTwoRowReview()
    const review = askCmdrState.renameReview
    if (!review) throw new Error('the review should be open')
    // The user keeps the first name and turns down the second.
    review.rows[1].allowed = false
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
    fire({
      type: 'proposalReady',
      proposal: {
        proposalId: 'proposal-1',
        rows: [
          {
            rowId: 'row-1',
            sourceName: 'before.png',
            destinationName: 'after.png',
            sourcePath: '/shots/before.png',
            volumeId: 'root',
            evidence: { source: 'filename' as const, detail: 'before' },
            coverage: null,
          },
        ],
      },
    })
    await vi.waitFor(() => {
      expect(preflightRenameMock).toHaveBeenCalledTimes(1)
    })
    preflightRenameMock.mockClear()

    await renameReviewListingChanged([{ type: 'add', entry: { name: 'after.png' } }])

    await vi.waitFor(() => {
      expect(preflightRenameMock).toHaveBeenCalledWith('proposal-1', ['row-1'])
    })
    expect(askCmdrState.renameReview?.rows[0]).toMatchObject({
      allowed: false,
      blockedReason: 'targetExists',
    })

    preflightRenameMock.mockResolvedValue({
      status: 'ready',
      rows: [{ rowId: 'row-1', status: 'ready', reason: null, warnings: [] }],
    })
    await renameReviewListingChanged([{ type: 'remove', entry: { name: 'after.png' } }])

    await vi.waitFor(() => {
      expect(askCmdrState.renameReview?.rows[0]?.blockedReason).toBeNull()
    })
    expect(askCmdrState.renameReview?.rows[0]?.allowed).toBe(false)
  })

  it('ignores watcher changes unrelated to the reviewed names', async () => {
    sendMessage('rename it')
    fire({
      type: 'proposalReady',
      proposal: {
        proposalId: 'proposal-1',
        rows: [
          {
            rowId: 'row-1',
            sourceName: 'before.png',
            destinationName: 'after.png',
            sourcePath: '/shots/before.png',
            volumeId: 'root',
            evidence: { source: 'filename' as const, detail: 'before' },
            coverage: null,
          },
        ],
      },
    })
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
    fire({
      type: 'proposalReady',
      proposal: {
        proposalId: 'proposal-1',
        rows: [
          {
            rowId: 'row-1',
            sourceName: 'before.png',
            destinationName: 'after.png',
            sourcePath: '/shots/before.png',
            volumeId: 'root',
            evidence: { source: 'filename' as const, detail: 'before' },
            coverage: null,
          },
        ],
      },
    })

    await vi.waitFor(() => {
      expect(askCmdrState.renameReview?.rows[0]).toMatchObject({
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
      expect(askCmdrState.renameReview?.rows[0]?.blockedReason).toBeNull()
    })
    expect(preflightRenameMock).toHaveBeenCalledWith('proposal-1', ['row-1'])
    expect(askCmdrState.renameReview?.rows[0]?.allowed).toBe(false)
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
            rowId: 'row-1',
            sourceName: 'before.png',
            destinationName: 'Klarna invoice.png',
            sourcePath: '/shots/before.png',
            volumeId: 'root',
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

    await reviseRenameRow('row-1', 'Klarna payment confirmation 2026-07-24.png')

    expect(reviseRenameMock).toHaveBeenCalledWith('proposal-1', 'row-1', 'Klarna payment confirmation 2026-07-24.png')
    expect(askCmdrState.renameReview?.rows[0]).toMatchObject({
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

    await reviseRenameRow('row-1', 'folder/name.png')

    expect(askCmdrState.renameReview?.rows[0]).toMatchObject({
      destinationName: 'Klarna invoice.png',
      nameRejected: true,
    })
    expect(preflightRenameMock).not.toHaveBeenCalled()
  })

  /** A blur with nothing typed is not an edit: no IPC, and no evidence swapped for one. */
  it('sends nothing when the name did not change', async () => {
    await openOneRowReview()

    await reviseRenameRow('row-1', 'Klarna invoice.png')

    expect(reviseRenameMock).not.toHaveBeenCalled()
    expect(askCmdrState.renameReview?.rows[0]?.evidence.source).toBe('imageText')
  })
})
