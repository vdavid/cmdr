/**
 * The context gauge's state slice: a `contextUsage` event records the turn's figures, opening a
 * thread restores its last measured figures, and a fresh chat carries none of the previous
 * thread's fill.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AskCmdrStreamEvent, ConversationDetailView } from '$lib/tauri-commands'

const sendMock =
  vi.fn<
    (c: number | null, t: string, a: unknown[], d: string[], o: (e: AskCmdrStreamEvent) => void) => Promise<number>
  >()
const getConversationMock =
  vi.fn<(id: number, limit: number, offset: number) => Promise<ConversationDetailView | null>>()

vi.mock('$lib/tauri-commands', () => ({
  sendAskCmdrMessage: (c: number | null, t: string, a: unknown[], d: string[], o: (e: AskCmdrStreamEvent) => void) =>
    sendMock(c, t, a, d, o),
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

let lastOnEvent: ((e: AskCmdrStreamEvent) => void) | null = null

function fire(event: AskCmdrStreamEvent): void {
  if (!lastOnEvent) throw new Error('no active send to fire an event into')
  lastOnEvent(event)
}

beforeEach(() => {
  sendMock.mockReset()
  sendMock.mockImplementation((c, _t, _a, _d, o) => {
    lastOnEvent = o
    return Promise.resolve(c ?? 1)
  })
  getConversationMock.mockReset()
  newChat()
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
