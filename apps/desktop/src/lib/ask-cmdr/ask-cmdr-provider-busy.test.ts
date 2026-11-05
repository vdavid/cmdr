/**
 * A busy provider in the rail: while the backend waits to try an overloaded call again, the
 * bubble says the provider is busy and which try comes next; the first sign of a reply, or the
 * turn's end, takes that line away; and a provider busy through every try ends on its own line.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { AskCmdrSendOutcome, AskCmdrStreamEvent } from '#lib/tauri-commands/index.js'
import type { AssistantItem } from './ask-cmdr-messages'

const sendMock = vi.fn<(c: number | null, t: string, a: unknown[], d: string[]) => Promise<AskCmdrSendOutcome>>()

vi.mock('#lib/tauri-commands/index.js', () => ({
  sendAskCmdrMessage: (c: number | null, t: string, a: unknown[], d: string[]) => sendMock(c, t, a, d),
  cancelAskCmdr: vi.fn(() => Promise.resolve({ tripped: 1, live: 1 })),
  keepGoingAskCmdr: vi.fn(() => Promise.resolve(true)),
  listAskCmdrConversations: vi.fn(() => Promise.resolve([])),
  getAskCmdrConversation: vi.fn(() => Promise.resolve(null)),
  recordAskCmdrSlotChange: vi.fn(() => Promise.resolve([])),
  preflightBulkRename: vi.fn(() => Promise.resolve({ status: 'ready', rows: [] })),
  cancelBulkRenameProposal: vi.fn(() => Promise.resolve()),
  applyBulkRename: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  reviseBulkRenameRow: vi.fn(() => Promise.resolve()),
}))
vi.mock('#lib/app-status-store.js', () => ({ saveAppStatus: vi.fn() }))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))
vi.mock('#lib/file-explorer/pane/explorer-state.svelte.js', () => ({
  explorerState: { setRailFocused: vi.fn() },
}))
vi.mock('./rail-window', () => ({
  growMainWindowForRail: vi.fn(() => Promise.resolve()),
  shrinkMainWindowForRail: vi.fn(() => Promise.resolve()),
}))
vi.mock('./ask-cmdr-gate.svelte', () => ({
  refreshRailGate: vi.fn(() => Promise.resolve('chat')),
}))

import { askCmdrState, newChat, sendMessage } from './ask-cmdr-trigger.svelte'
import { handleTurnEvent, resetTurnBookkeeping } from './ask-cmdr-stream.svelte'
import { errorMessage, providerBusyMessage } from './ask-cmdr-labels'

const THREAD_ID = 1

function fire(event: AskCmdrStreamEvent): void {
  askCmdrState.conversationId = THREAD_ID
  handleTurnEvent({ conversationId: THREAD_ID, run: null, event })
}

function bubble(): AssistantItem | undefined {
  return askCmdrState.messages.find((m): m is AssistantItem => m.kind === 'assistant')
}

/** A send whose first call the provider turned away, so the backend waits for try two. */
function busyTurn(): void {
  sendMessage('what changed in Downloads?')
  fire({ type: 'started' })
  fire({ type: 'assistantStarted' })
  fire({ type: 'providerBusyRetrying', attempt: 2, attempts: 4 })
}

beforeEach(() => {
  sendMock.mockReset()
  sendMock.mockImplementation((c) => Promise.resolve({ accepted: true, conversationId: c ?? THREAD_ID, run: 1 }))
  newChat()
  resetTurnBookkeeping()
  askCmdrState.messages = []
  askCmdrState.conversationId = null
})

describe('a busy provider', () => {
  it('says so in the bubble, with the try that comes next', () => {
    busyTurn()
    expect(bubble()?.busy).toMatchObject({ attempt: 2, attempts: 4 })
    expect(bubble()?.thinking).toBe(false)
    fire({ type: 'providerBusyRetrying', attempt: 3, attempts: 4 })
    expect(bubble()?.busy).toMatchObject({ attempt: 3, attempts: 4 })
  })

  it('stops saying so once the reply starts', () => {
    busyTurn()
    fire({ type: 'textDelta', text: 'Three new files.' })
    expect(bubble()?.busy).toBeUndefined()
  })

  it('stops saying so once the model starts thinking or calls a tool', () => {
    busyTurn()
    fire({ type: 'reasoningTick' })
    expect(bubble()?.busy).toBeUndefined()

    fire({ type: 'providerBusyRetrying', attempt: 3, attempts: 4 })
    fire({ type: 'toolCallStarted', callId: 'c1', tool: 'list_dir' })
    expect(bubble()?.busy).toBeUndefined()
  })

  /** A busy wait can run past the stall window, and it is progress the backend reported: the
   *  watchdog must not call it a timeout. */
  it('counts each retry as progress for the watchdog', async () => {
    vi.useFakeTimers()
    try {
      busyTurn()
      await vi.advanceTimersByTimeAsync(100_000)
      fire({ type: 'providerBusyRetrying', attempt: 3, attempts: 4 })
      await vi.advanceTimersByTimeAsync(100_000)
      expect(askCmdrState.streaming).toBe(true)
      expect(askCmdrState.messages.some((m) => m.kind === 'error')).toBe(false)
    } finally {
      vi.useRealTimers()
    }
  })

  it('ends on its own busy line when every try was turned away', () => {
    busyTurn()
    fire({ type: 'failed', kind: 'providerBusy', detail: 'HTTP 503: high demand', managed: null, turn: null })
    expect(askCmdrState.streaming).toBe(false)
    expect(askCmdrState.messages.some((m) => m.kind === 'error' && m.errorKind === 'providerBusy')).toBe(true)
  })
})

describe('the busy copy', () => {
  it('names the provider when it has a name, and says which try comes next', () => {
    const named = providerBusyMessage({ attempt: 2, attempts: 4, provider: 'Google Gemini' })
    expect(named).toContain('Google Gemini')
    expect(named).toContain('2')
    expect(named).toContain('4')
    const unnamed = providerBusyMessage({ attempt: 3, attempts: 4 })
    expect(unnamed).toContain('3')
    expect(unnamed).not.toBe(named)
  })

  it('ends a busy turn on a line of its own, not the quota one', () => {
    const named = errorMessage('providerBusy', null, 'Google Gemini')
    expect(named).toContain('Google Gemini')
    expect(named).not.toBe(errorMessage('rateLimited'))
    const unnamed = errorMessage('providerBusy')
    expect(unnamed).not.toBe('')
    expect(unnamed).not.toBe(named)
    expect(unnamed).not.toBe(errorMessage('rateLimited'))
  })
})
