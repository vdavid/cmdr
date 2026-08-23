/**
 * The context gauge's state slice: a `contextUsage` event records the turn's figures, opening a
 * thread restores its last measured figures, and a fresh chat carries none of the previous
 * thread's fill.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AskCmdrSendOutcome, AskCmdrStreamEvent, ConversationDetailView } from '$lib/tauri-commands'

const sendMock = vi.fn<(c: number | null, t: string, a: unknown[], d: string[]) => Promise<AskCmdrSendOutcome>>()
const getConversationMock =
  vi.fn<(id: number, limit: number, offset: number) => Promise<ConversationDetailView | null>>()

vi.mock('$lib/tauri-commands', () => ({
  sendAskCmdrMessage: (c: number | null, t: string, a: unknown[], d: string[]) => sendMock(c, t, a, d),
  cancelAskCmdr: vi.fn(() => Promise.resolve()),
  listAskCmdrConversations: vi.fn(() => Promise.resolve([])),
  getAskCmdrConversation: (id: number, limit: number, offset: number) => getConversationMock(id, limit, offset),
  recordAskCmdrModelChange: vi.fn(() => Promise.resolve(null)),
  preflightBulkRename: vi.fn(() => Promise.resolve({ status: 'ready', rows: [] })),
  cancelBulkRenameProposal: vi.fn(() => Promise.resolve()),
  applyBulkRename: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  reviseBulkRenameRow: vi.fn(() => Promise.resolve()),
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

import { askCmdrState, newChat, sendMessage, switchToThread } from './ask-cmdr-trigger.svelte'
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
  getConversationMock.mockReset()
  newChat()
  resetStoppedTurns()
  askCmdrState.messages = []
  askCmdrState.conversationId = null
})

describe('the context gauge state', () => {
  it('records the turn figures a contextUsage event reports', () => {
    sendMessage('name these')
    fire({ type: 'contextUsage', estimatedTokens: 31_200, budgetTokens: 60_000, elidedResults: 0 })

    expect(askCmdrState.contextUsage).toEqual({
      estimatedTokens: 31_200,
      budgetTokens: 60_000,
      elidedResults: 0,
    })
  })

  it('keeps the set-aside count, so the gauge can show that state without a second event', () => {
    sendMessage('name these')
    fire({ type: 'contextUsage', estimatedTokens: 12_000, budgetTokens: 60_000, elidedResults: 3 })

    expect(askCmdrState.contextUsage?.elidedResults).toBe(3)
  })

  it('restores a reopened thread last measured figures instead of showing an empty gauge', async () => {
    getConversationMock.mockResolvedValue({
      conversation: { id: 7, title: 't', createdAt: 0, updatedAt: 0, archived: false, origin: null },
      messages: [],
      totalMessages: 0,
      lastContextUsage: { estimatedTokens: 44_000, budgetTokens: 60_000 },
    })

    await switchToThread(7)

    expect(askCmdrState.contextUsage).toEqual({
      estimatedTokens: 44_000,
      budgetTokens: 60_000,
      // Whether THAT turn set anything aside isn't persisted, so it never claims one.
      elidedResults: 0,
    })
  })

  it('shows no gauge for a thread that never finished a turn', async () => {
    getConversationMock.mockResolvedValue({
      conversation: { id: 8, title: 't', createdAt: 0, updatedAt: 0, archived: false, origin: null },
      messages: [],
      totalMessages: 0,
      lastContextUsage: null,
    })

    await switchToThread(8)

    expect(askCmdrState.contextUsage).toBeNull()
  })

  it('drops the previous thread figures when a fresh chat starts', () => {
    sendMessage('name these')
    fire({ type: 'contextUsage', estimatedTokens: 55_000, budgetTokens: 60_000, elidedResults: 0 })

    newChat()

    expect(askCmdrState.contextUsage).toBeNull()
  })
})
