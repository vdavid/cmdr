/**
 * Undo after a rename batch lands: the state slice.
 *
 * This is the safety net that fires AFTER the names are real, so its failure modes are
 * about honesty. Three things it must never do: claim a clean undo when files stayed
 * behind, send the batch ids in an order the backend would reverse oldest-first, or
 * leave the job-wide "undo everything" on more than one line at a time.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { UndoReport } from '$lib/tauri-commands'

const applyRenameMock = vi.fn<(...args: unknown[]) => Promise<unknown>>()
const undoOperationsMock = vi.fn<(ids: string[]) => Promise<UndoReport>>()

vi.mock('$lib/tauri-commands', () => ({
  sendAskCmdrMessage: vi.fn(() => Promise.resolve({ accepted: true, conversationId: 1 })),
  cancelAskCmdr: vi.fn(() => Promise.resolve()),
  listAskCmdrConversations: vi.fn(() => Promise.resolve([])),
  getAskCmdrConversation: vi.fn(() => Promise.resolve(null)),
  recordAskCmdrModelChange: vi.fn(() => Promise.resolve(null)),
  preflightBulkRename: vi.fn(() => Promise.resolve({ status: 'ready', rows: [] })),
  cancelBulkRenameProposal: vi.fn(() => Promise.resolve()),
  applyBulkRename: (...args: unknown[]) => applyRenameMock(...args),
  reviseBulkRenameRow: vi.fn(() => Promise.resolve(null)),
  undoOperations: (ids: string[]) => undoOperationsMock(ids),
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

import { askCmdrState, newChat, noteRenameApplied, undoRename, type RailMessage } from './ask-cmdr-trigger.svelte'

type RenameLine = Extract<RailMessage, { kind: 'renameApplied' }>

function lines(): RenameLine[] {
  return askCmdrState.messages.filter((m): m is RenameLine => m.kind === 'renameApplied')
}

function report(overrides: Partial<UndoReport> = {}): UndoReport {
  return { operations: [], restored: 0, skipped: 0, ...overrides }
}

function operation(operationId: string, restored: number, skipped = 0): UndoReport['operations'][number] {
  return {
    operationId,
    restored,
    skipped,
    // The engine records a reason for every skip, so a skipping operation carries one.
    skips: skipped > 0 ? [{ reason: 'drift', count: skipped, exampleName: 'invoice-2026.pdf' }] : [],
    finalState: skipped > 0 ? 'partiallyRolledBack' : 'rolledBack',
    refusal: null,
  }
}

beforeEach(() => {
  vi.clearAllMocks()
  newChat()
  askCmdrState.messages = []
})

describe('a finished batch', () => {
  it('leaves an undoable line in the thread reporting what was renamed', () => {
    noteRenameApplied('op-1', 23)

    expect(lines()).toEqual([
      {
        kind: 'renameApplied',
        operationId: 'op-1',
        fileCount: 23,
        jobOperationIds: [],
        jobFileCount: 0,
        undo: { status: 'undoable' },
      },
    ])
  })

  it('offers no job-wide undo for a single batch', () => {
    noteRenameApplied('op-1', 23)

    expect(lines()[0].jobOperationIds).toEqual([])
  })
})

describe('a multi-batch run', () => {
  it('collects the whole run on the NEWEST line only, so "undo everything" appears once', () => {
    noteRenameApplied('op-1', 10)
    noteRenameApplied('op-2', 5)
    noteRenameApplied('op-3', 8)

    const [first, second, third] = lines()
    expect(first.jobOperationIds).toEqual([])
    expect(second.jobOperationIds).toEqual([])
    // Apply order, oldest first: that's what the backend needs to reverse newest-first.
    expect(third.jobOperationIds).toEqual(['op-1', 'op-2', 'op-3'])
    expect(third.jobFileCount).toBe(23)
  })

  it('sends the batch ids in APPLY order, so the backend can reverse them newest first', async () => {
    undoOperationsMock.mockResolvedValue(
      report({ restored: 23, operations: [operation('op-3', 8), operation('op-2', 5), operation('op-1', 10)] }),
    )
    noteRenameApplied('op-1', 10)
    noteRenameApplied('op-2', 5)
    noteRenameApplied('op-3', 8)

    await undoRename(lines()[2], 'job')

    // Oldest first over IPC. Reversing them is the backend's job (`undo_order`), and
    // this order is what breaks a same-second tie there — a reversed list here would
    // silently undo oldest-first and skip any batch whose old name was reused.
    expect(undoOperationsMock).toHaveBeenCalledWith(['op-1', 'op-2', 'op-3'])
  })

  it('undoes only the clicked batch when the scope is that batch', async () => {
    undoOperationsMock.mockResolvedValue(report({ restored: 5, operations: [operation('op-2', 5)] }))
    noteRenameApplied('op-1', 10)
    noteRenameApplied('op-2', 5)

    await undoRename(lines()[1])

    expect(undoOperationsMock).toHaveBeenCalledWith(['op-2'])
    // The untouched batch keeps its own Undo.
    expect(lines()[0].undo).toEqual({ status: 'undoable' })
  })

  it('retires every line a job undo covered, so no line offers an undo that would be refused', async () => {
    undoOperationsMock.mockResolvedValue(
      report({ restored: 15, operations: [operation('op-2', 5), operation('op-1', 10)] }),
    )
    noteRenameApplied('op-1', 10)
    noteRenameApplied('op-2', 5)

    await undoRename(lines()[1], 'job')

    expect(lines()[0].undo).toEqual({ status: 'unavailable' })
    expect(lines()[1].undo).toEqual({ status: 'undone', restored: 15 })
    expect(lines()[1].jobOperationIds).toEqual([])
  })
})

describe('reporting the result', () => {
  it('shows the undo running while it waits, then the outcome', async () => {
    let resolve: (value: UndoReport) => void = () => {}
    undoOperationsMock.mockReturnValue(new Promise<UndoReport>((r) => (resolve = r)))
    noteRenameApplied('op-1', 23)
    const line = lines()[0]

    const pending = undoRename(line)
    expect(line.undo).toEqual({ status: 'undoing' })

    resolve(report({ restored: 23, operations: [operation('op-1', 23)] }))
    await pending
    expect(line.undo).toEqual({ status: 'undone', restored: 23 })
  })

  it('reports a partial undo as partial, never as a clean success', async () => {
    undoOperationsMock.mockResolvedValue(report({ restored: 19, skipped: 4, operations: [operation('op-1', 19, 4)] }))
    noteRenameApplied('op-1', 23)

    await undoRename(lines()[0])

    expect(lines()[0].undo).toEqual({
      status: 'partial',
      restored: 19,
      skipped: 4,
      refusedBatches: 0,
      skips: [{ reason: 'drift', count: 4, exampleName: 'invoice-2026.pdf' }],
    })
  })

  it('hands the Undo back when the call itself did not go through', async () => {
    undoOperationsMock.mockRejectedValue(new Error('journal is locked'))
    noteRenameApplied('op-1', 23)

    await undoRename(lines()[0])

    // Nothing is known to have moved, so the user must still be able to try.
    expect(lines()[0].undo).toEqual({ status: 'undoable' })
  })

  it('ignores a second click while an undo is already running', async () => {
    undoOperationsMock.mockReturnValue(new Promise<UndoReport>(() => {}))
    noteRenameApplied('op-1', 23)
    const line = lines()[0]

    void undoRename(line)
    await undoRename(line)

    expect(undoOperationsMock).toHaveBeenCalledTimes(1)
  })
})

describe('applying a review', () => {
  it('records the started batch, so the result carries an undo', async () => {
    applyRenameMock.mockResolvedValue({ operationId: 'op-42', operationType: 'rename' })
    askCmdrState.renameReview = {
      proposalId: 'p-1',
      rows: [
        { rowId: 'r-1', allowed: true, blockedReason: null },
        { rowId: 'r-2', allowed: true, blockedReason: null },
        { rowId: 'r-3', allowed: false, blockedReason: null },
      ],
      preflighting: false,
      expired: false,
      requestVersion: 0,
    } as unknown as typeof askCmdrState.renameReview
    const { applyRenameReview } = await import('./ask-cmdr-trigger.svelte')

    await applyRenameReview()

    // The count is the rows actually submitted, not every row in the review.
    expect(lines()).toHaveLength(1)
    expect(lines()[0].operationId).toBe('op-42')
    expect(lines()[0].fileCount).toBe(2)
  })
})

describe('a fresh chat', () => {
  it('starts with no rename lines, so a new thread offers no stale undo', () => {
    noteRenameApplied('op-1', 23)
    newChat()

    expect(lines()).toEqual([])
  })
})
