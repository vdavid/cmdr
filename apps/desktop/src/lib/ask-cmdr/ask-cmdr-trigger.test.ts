/**
 * The rail's streaming state machine: sending appends a user message and streams the
 * answer into the last assistant bubble; width persists; the soft-cap nudge trips past the
 * constant. Stop and a reload mid-turn: `ask-cmdr-stop-and-reload.test.ts`.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { AskCmdrSendOutcome, AskCmdrStreamEvent, MessageView } from '#lib/tauri-commands/index.js'
import type { CancelOutcomeView, ResolvedCall, ToolOutcome, ToolSubject, TurnStats } from '#lib/ipc/bindings.js'

const sendMock = vi.fn<(c: number | null, t: string, a: unknown[], d: string[]) => Promise<AskCmdrSendOutcome>>()
const cancelMock = vi.fn<(id: number) => Promise<CancelOutcomeView>>()
const listMock = vi.fn<(...a: unknown[]) => Promise<unknown>>()
const getMock = vi.fn<(...a: unknown[]) => Promise<unknown>>()
const recordMock = vi.fn<(id: number) => Promise<unknown>>()
const slotStatusMock = vi.fn<() => Promise<unknown>>(() => Promise.resolve(null))
const saveMock = vi.fn()
const growWindowMock = vi.fn<(w: number) => Promise<void>>()
const shrinkWindowMock = vi.fn<(w: number) => Promise<void>>()
const preflightRenameMock = vi.fn<(...args: unknown[]) => Promise<unknown>>()
const reviseRenameMock = vi.fn<(...args: unknown[]) => Promise<unknown>>()
const applyRenameMock = vi.fn<(...args: unknown[]) => Promise<unknown>>()

vi.mock('#lib/tauri-commands/index.js', () => ({
  sendAskCmdrMessage: (c: number | null, t: string, a: unknown[], d: string[]) => sendMock(c, t, a, d),
  cancelAskCmdr: (id: number) => cancelMock(id),
  listAskCmdrConversations: (...a: unknown[]) => listMock(...a),
  getAskCmdrConversation: (...a: unknown[]) => getMock(...a),
  recordAskCmdrSlotChange: (id: number) => recordMock(id),
  askCmdrSlotStatus: () => slotStatusMock(),
  preflightBulkRename: (...args: unknown[]) => preflightRenameMock(...args),
  cancelBulkRenameProposal: vi.fn(() => Promise.resolve()),
  applyBulkRename: (...args: unknown[]) => applyRenameMock(...args),
  reviseBulkRenameRow: (...args: unknown[]) => reviseRenameMock(...args),
}))
vi.mock('#lib/app-status-store.js', () => ({
  saveAppStatus: (s: unknown) => {
    saveMock(s)
  },
}))
const logMocks = vi.hoisted(() => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => logMocks,
}))
vi.mock('#lib/file-explorer/pane/explorer-state.svelte.js', () => ({
  explorerState: { setRailFocused: vi.fn() },
}))
vi.mock('./rail-window', () => ({
  growMainWindowForRail: (w: number) => growWindowMock(w),
  shrinkMainWindowForRail: (w: number) => shrinkWindowMock(w),
}))
// The rail's gate reads "chat" unless a test moves it, so `openRail` proceeds to bootstrap.
const gateMock = vi.hoisted(() => ({ gate: 'chat' }))
vi.mock('./ask-cmdr-gate.svelte', () => ({
  refreshRailGate: vi.fn(() => Promise.resolve(gateMock.gate)),
}))

import {
  addAttachments,
  askCmdrState,
  hasOlderMessages,
  hydrateRail,
  isOverSoftCap,
  loadOlderMessages,
  MESSAGE_PAGE,
  closeRail,
  newChat,
  newChatShortcutText,
  noteSlotSettingChanged,
  openNewChat,
  openRail,
  ensureThreadLoaded,
  RAIL_MAX_WIDTH,
  RAIL_MIN_WIDTH,
  removeAttachment,
  sendMessage,
  setRailWidth,
  stopStreaming,
  switchToThread,
  THREAD_SOFT_CAP_MESSAGES,
  type RailMessage,
  type RailToolCall,
} from './ask-cmdr-trigger.svelte'
import { handleTurnEvent, resetTurnBookkeeping } from './ask-cmdr-stream.svelte'
import { toDisplayShortcut } from '#lib/shortcuts/index.js'
import { toPlatformShortcut } from '#lib/shortcuts/key-capture.js'

/** The thread a fired event belongs to when the test doesn't name one. */
const THREAD_ID = 1

/** What most tool-line tests need off `toolCallFinished`: a plain answer, and a refusal. */
const ANSWERED: ToolOutcome = { kind: 'answered' }
const REFUSED: ToolOutcome = { kind: 'refused', reason: { kind: 'problem' } }
const UNPARSED: ToolSubject = { kind: 'unparsed' }
const PANES: ToolSubject = { kind: 'panes' }
const STATS: TurnStats = { toolCalls: 2, wallMs: 1200, promptTokens: 10, completionTokens: 2 }

/** One call of a `toolCallsResolved` batch. */
function resolved(callId: string, tool: string): ResolvedCall {
  return { callId, tool, subject: UNPARSED, repeatOf: null }
}

/** Feed one stream event in as the backend emits it, for the thread the rail is on. A rail with
 * no thread yet is put on the named one first: production gets there through its send's answer,
 * which `ask-cmdr-send-before-id.test.ts` covers on its own. */
function fire(event: AskCmdrStreamEvent, conversationId: number = askCmdrState.conversationId ?? THREAD_ID): void {
  askCmdrState.conversationId = conversationId
  handleTurnEvent({ conversationId, event })
}

beforeEach(() => {
  sendMock.mockReset()
  cancelMock.mockReset()
  cancelMock.mockResolvedValue({ tripped: 1, live: 1 })
  saveMock.mockReset()
  listMock.mockReset()
  getMock.mockReset()
  recordMock.mockReset()
  growWindowMock.mockReset()
  shrinkWindowMock.mockReset()
  preflightRenameMock.mockReset()
  preflightRenameMock.mockResolvedValue({ status: 'ready', rows: [] })
  reviseRenameMock.mockReset()
  applyRenameMock.mockReset()
  applyRenameMock.mockResolvedValue({ operationId: 'op-1' })
  growWindowMock.mockResolvedValue()
  shrinkWindowMock.mockResolvedValue()
  listMock.mockResolvedValue([])
  sendMock.mockImplementation((c) => Promise.resolve({ accepted: true, conversationId: c ?? THREAD_ID }))
  newChat()
  resetTurnBookkeeping()
  askCmdrState.messages = []
  askCmdrState.conversationId = null
  askCmdrState.open = false
})

function conversationRow(id: number) {
  return { id, title: 't', createdAt: 0, updatedAt: 0, archived: false, origin: null }
}

function assistantAt(index: number): Extract<RailMessage, { kind: 'assistant' }> {
  const message = askCmdrState.messages[index]
  if (message.kind !== 'assistant') throw new Error('expected an assistant message')
  return message
}

/** A bubble's narration across its rounds. */
function textOf(item: Extract<RailMessage, { kind: 'assistant' }>): string {
  return item.rounds.map((round) => round.text).join('')
}

/** A bubble's tool lines across its rounds. */
function linesOf(item: Extract<RailMessage, { kind: 'assistant' }>): RailToolCall[] {
  return item.rounds.flatMap((round) => round.calls)
}

describe('sendMessage + streaming', () => {
  it('appends a user message and streams the answer into an assistant bubble', () => {
    sendMessage('hello')
    expect(askCmdrState.messages[0]).toEqual({ kind: 'user', id: null, text: 'hello', attachments: [] })
    expect(askCmdrState.streaming).toBe(true)
    expect(sendMock).toHaveBeenCalledWith(null, 'hello', [], [])

    fire({ type: 'started' }, 7)
    fire({ type: 'assistantStarted' })
    fire({ type: 'textDelta', text: 'Hi ' })
    fire({ type: 'textDelta', text: 'there' })
    expect(textOf(assistantAt(1))).toBe('Hi there')
    expect(assistantAt(1).streaming).toBe(true)

    fire({
      type: 'done',
      messageId: 42,
      seq: 2,
      stop: 'completed',
      usage: { promptTokens: 1, completionTokens: 2 },
      turn: null,
    })
    expect(assistantAt(1).streaming).toBe(false)
    expect(assistantAt(1).id).toBe(42)
    expect(askCmdrState.conversationId).toBe(7)
    expect(askCmdrState.streaming).toBe(false)
  })

  it('renders tool call lines and their finished status', () => {
    sendMessage('what am I looking at?')
    fire({ type: 'assistantStarted' })
    fire({ type: 'toolCallStarted', callId: 'c1', tool: 'app_state' })
    expect(linesOf(assistantAt(1))[0]).toMatchObject({ callId: 'c1', tool: 'app_state', running: true })
    fire({ type: 'toolCallsResolved', messageId: 2, calls: [resolved('c1', 'app_state')] })
    fire({ type: 'toolCallFinished', messageId: 2, callId: 'c1', outcome: ANSWERED, subject: PANES })
    expect(linesOf(assistantAt(1))[0]).toMatchObject({ running: false, outcome: ANSWERED, subject: PANES })
  })

  it('a refused outcome settles its line with that outcome', () => {
    sendMessage('look')
    fire({ type: 'assistantStarted' })
    fire({ type: 'toolCallStarted', callId: 'c1', tool: 'delete' })
    fire({ type: 'toolCallsResolved', messageId: 2, calls: [resolved('c1', 'delete')] })
    fire({ type: 'toolCallFinished', messageId: 2, callId: 'c1', outcome: REFUSED, subject: UNPARSED })
    expect(linesOf(assistantAt(1))[0]).toMatchObject({ running: false, outcome: REFUSED })
  })

  it('a repeated start for a call already on screen adds no second tool line', () => {
    // Regression anchor for ERR-FRMAR: two lines with one call id crashed the window on
    // Svelte's `each_key_duplicate`. The backend announces each call once, and the rail
    // holds the same line on its own side, whatever sends the repeat.
    sendMessage('rename these')
    fire({ type: 'assistantStarted' })
    fire({ type: 'toolCallStarted', callId: 'c1', tool: 'list_dir' })
    fire({ type: 'toolCallStarted', callId: 'c1', tool: 'list_dir' })
    fire({ type: 'toolCallStarted', callId: 'c2', tool: 'app_state' })
    expect(linesOf(assistantAt(1)).map((tool) => tool.callId)).toEqual(['c1', 'c2'])
  })

  it("a turn's end settles a call that never reported back, keeping it with no outcome", () => {
    sendMessage('inspect this folder')
    fire({ type: 'assistantStarted' })
    fire({ type: 'toolCallStarted', callId: 'finished', tool: 'list_dir' })
    fire({ type: 'toolCallStarted', callId: 'unfinished', tool: 'list_volumes' })
    fire({
      type: 'toolCallsResolved',
      messageId: 2,
      calls: [resolved('finished', 'list_dir'), resolved('unfinished', 'list_volumes')],
    })
    fire({ type: 'toolCallFinished', messageId: 2, callId: 'finished', outcome: ANSWERED, subject: UNPARSED })
    fire({ type: 'assistantStarted' })
    fire({ type: 'reasoningTick' })
    fire({ type: 'textDelta', text: 'Here it is.' })

    fire({
      type: 'done',
      messageId: 6,
      seq: 6,
      stop: 'completed',
      usage: { promptTokens: 1, completionTokens: 2 },
      turn: null,
    })

    expect(linesOf(assistantAt(1)).map((tool) => [tool.callId, tool.running, tool.outcome])).toEqual([
      ['finished', false, ANSWERED],
      ['unfinished', false, null],
    ])
    expect(assistantAt(1)).toMatchObject({ id: 6, thinking: false, stalled: false, streaming: false })
    expect(textOf(assistantAt(1))).toBe('Here it is.')
  })

  it('a typed failure ends streaming and shows an honest notice', () => {
    sendMessage('hi')
    fire({ type: 'assistantStarted' })
    fire({ type: 'failed', kind: 'rateLimited', detail: null, managed: null, turn: null })
    expect(askCmdrState.streaming).toBe(false)
    // The empty assistant bubble is dropped; an error item takes its place.
    const last = askCmdrState.messages.at(-1)
    expect(last).toEqual({ kind: 'error', errorKind: 'rateLimited', detail: undefined })
  })

  /** What wasn't stored goes, as a reload wouldn't show it: the round that never reached `End`.
   *  The rounds that did stay, with what the turn spent. */
  it('a typed failure keeps what persisted and drops the round that never did', () => {
    sendMessage('inspect this folder')
    fire({ type: 'assistantStarted' })
    fire({ type: 'toolCallStarted', callId: 'c1', tool: 'list_dir' })
    fire({ type: 'toolCallsResolved', messageId: 2, calls: [resolved('c1', 'list_dir')] })
    fire({ type: 'toolCallFinished', messageId: 2, callId: 'c1', outcome: ANSWERED, subject: UNPARSED })
    fire({ type: 'assistantStarted' })
    fire({ type: 'textDelta', text: 'I found part of it.' })

    fire({ type: 'failed', kind: 'budgetExhausted', detail: null, managed: null, turn: STATS })

    expect(assistantAt(1).rounds.map((round) => round.messageId)).toEqual([2])
    expect(assistantAt(1)).toMatchObject({ id: 2, thinking: false, streaming: false, stats: STATS })
    expect(askCmdrState.messages.at(-1)).toEqual({
      kind: 'error',
      errorKind: 'budgetExhausted',
      detail: undefined,
    })
  })

  /** A turn that reached a per-answer ceiling says so once, in its own bubble from its stats
   *  (the way a reload shows it), so no vaguer error line joins it. */
  it('a turn that reached a limit adds no error line beside its own notice', () => {
    const limited = { ...STATS, limit: { kind: 'toolRounds' as const, max: 25, answered: false } }
    sendMessage('sort my downloads')
    fire({ type: 'assistantStarted' })
    fire({ type: 'toolCallStarted', callId: 'c1', tool: 'list_dir' })
    fire({ type: 'toolCallsResolved', messageId: 2, calls: [resolved('c1', 'list_dir')] })
    fire({ type: 'toolCallFinished', messageId: 2, callId: 'c1', outcome: ANSWERED, subject: UNPARSED })

    fire({ type: 'failed', kind: 'budgetExhausted', detail: null, managed: null, turn: limited })

    expect(askCmdrState.messages.map((m) => m.kind)).toEqual(['user', 'assistant'])
    expect(assistantAt(1)).toMatchObject({ stats: limited })
  })

  it('a typed failure before anything persisted leaves no bubble', () => {
    sendMessage('inspect this folder')
    fire({ type: 'assistantStarted' })
    fire({ type: 'toolCallStarted', callId: 'c1', tool: 'list_dir' })
    fire({ type: 'textDelta', text: 'I found part of it.' })

    fire({ type: 'failed', kind: 'budgetExhausted', detail: null, managed: null, turn: null })

    expect(askCmdrState.messages.map((m) => m.kind)).toEqual(['user', 'error'])
  })

  it('a rejected send settles the turn without blaming the provider or showing raw IPC text', async () => {
    // `sendAskCmdrMessage` turns every Rust refusal into an outcome, so a rejection means the
    // invoke itself broke: a Cmdr bug, logged at error, and nothing the provider did.
    logMocks.error.mockClear()
    sendMock.mockRejectedValueOnce(new Error('command ask_cmdr_send_message not found'))
    sendMessage('hello')

    await vi.waitFor(() => {
      expect(askCmdrState.streaming).toBe(false)
    })

    expect(askCmdrState.messages.at(-1)).toEqual({
      kind: 'error',
      errorKind: 'unfinishedReply',
      detail: undefined,
    })
    expect(logMocks.error).toHaveBeenCalledOnce()
  })

  it('a send refused because cloud AI is off re-reads the gate, so the rail can show it', async () => {
    const { refreshRailGate } = await import('./ask-cmdr-gate.svelte')
    vi.mocked(refreshRailGate).mockClear()
    sendMock.mockResolvedValueOnce({ accepted: false, kind: 'noCloudConsent', detail: null, managed: null })
    sendMessage('hello')

    await vi.waitFor(() => {
      expect(askCmdrState.streaming).toBe(false)
    })
    expect(refreshRailGate).toHaveBeenCalledOnce()
  })

  it("a failure with provider detail keeps the provider's wording for display", () => {
    sendMessage('hi')
    fire({ type: 'assistantStarted' })
    fire({
      type: 'failed',
      kind: 'provider',
      detail: 'HTTP 404: This model is unavailable for free.',
      managed: null,
      turn: null,
    })
    const last = askCmdrState.messages.at(-1)
    expect(last).toEqual({
      kind: 'error',
      errorKind: 'provider',
      detail: 'HTTP 404: This model is unavailable for free.',
    })
  })

  it('ignores a second send while one is streaming (single-flight)', () => {
    sendMessage('first')
    sendMessage('second')
    expect(sendMock).toHaveBeenCalledTimes(1)
  })

  it('a modelChanged stream event inserts a timeline line before the current user bubble', () => {
    sendMessage('hi')
    fire({ type: 'assistantStarted' })
    fire({ type: 'modelChanged', messageId: 9, seq: 0, model: 'model-two' })
    expect(askCmdrState.messages.map((m) => m.kind)).toEqual(['modelChange', 'user', 'assistant'])
    expect(askCmdrState.messages[0]).toEqual({ kind: 'modelChange', model: 'model-two' })
  })

  it('a chatMemoryChanged stream event inserts a timeline line before the current user bubble', () => {
    // The change happened between the turns, so the line belongs above the message that
    // will be answered with the new amount of chat in view.
    sendMessage('hi')
    fire({ type: 'assistantStarted' })
    fire({ type: 'chatMemoryChanged', messageId: 9, seq: 0, chatMemoryTokens: 16_000 })
    expect(askCmdrState.messages.map((m) => m.kind)).toEqual(['chatMemoryChange', 'user', 'assistant'])
    expect(askCmdrState.messages[0]).toEqual({ kind: 'chatMemoryChange', chatMemoryTokens: 16_000 })
  })

  it('a contextTrimmed event tells the user the reply saw less than the whole chat', () => {
    // The backend drops older tool results when the prompt budget is tight. That must be
    // visible: an unannounced drop is what let a reply written without the evidence read
    // like a normal one.
    sendMessage('now the other 12')
    fire({ type: 'assistantStarted' })
    fire({ type: 'contextTrimmed', elidedResults: 2, approxTokens: 5_400 })
    expect(askCmdrState.messages.map((m) => m.kind)).toEqual(['user', 'contextTrimmed', 'assistant'])
    expect(askCmdrState.messages[1]).toEqual({ kind: 'contextTrimmed', count: 2 })
  })
})

describe('slot settings changes', () => {
  /** One `event`-role row as the backend hands it back. */
  function eventRow(id: number, blocks: unknown[]): unknown {
    return { id, seq: 4, role: 'event', blocks, promptTokens: null, completionTokens: null, createdAt: 0 }
  }

  it('records an event per changed facet for the active thread and appends the lines (debounced)', async () => {
    vi.useFakeTimers()
    try {
      askCmdrState.conversationId = 7
      // A model with a smaller window moves both facets, so the thread gets both lines.
      recordMock.mockResolvedValue([
        eventRow(9, [{ type: 'modelChanged', model: 'model-two' }]),
        eventRow(10, [{ type: 'chatMemoryChanged', chatMemoryTokens: 16_000 }]),
      ])
      noteSlotSettingChanged()
      noteSlotSettingChanged() // rapid keystrokes collapse to one backend call
      await vi.advanceTimersByTimeAsync(1500)
      expect(recordMock).toHaveBeenCalledTimes(1)
      expect(recordMock).toHaveBeenCalledWith(7)
      expect(askCmdrState.messages).toEqual([
        { kind: 'modelChange', model: 'model-two' },
        { kind: 'chatMemoryChange', chatMemoryTokens: 16_000 },
      ])
    } finally {
      vi.useRealTimers()
    }
  })

  it('re-reads the slot status once the debounce outlasts the settings flush, even with no thread', async () => {
    vi.useFakeTimers()
    try {
      askCmdrState.conversationId = null
      slotStatusMock.mockClear()
      noteSlotSettingChanged()
      await vi.advanceTimersByTimeAsync(500)
      // Not yet: the backend reads `settings.json`, which may not hold the change yet.
      expect(slotStatusMock).not.toHaveBeenCalled()
      await vi.advanceTimersByTimeAsync(1000)
      expect(slotStatusMock).toHaveBeenCalledTimes(1)
    } finally {
      vi.useRealTimers()
    }
  })

  it('appends only the chat-memory line when that is all that moved', async () => {
    vi.useFakeTimers()
    try {
      askCmdrState.conversationId = 7
      recordMock.mockResolvedValue([eventRow(9, [{ type: 'chatMemoryChanged', chatMemoryTokens: 200_000 }])])
      noteSlotSettingChanged()
      await vi.advanceTimersByTimeAsync(1500)
      expect(askCmdrState.messages).toEqual([{ kind: 'chatMemoryChange', chatMemoryTokens: 200_000 }])
    } finally {
      vi.useRealTimers()
    }
  })

  it('does nothing without an active thread, or when the backend reports no change', async () => {
    vi.useFakeTimers()
    try {
      noteSlotSettingChanged()
      await vi.advanceTimersByTimeAsync(1500)
      expect(recordMock).not.toHaveBeenCalled()

      askCmdrState.conversationId = 7
      recordMock.mockResolvedValue([]) // same effective slot (e.g. masked by the override)
      noteSlotSettingChanged()
      await vi.advanceTimersByTimeAsync(1500)
      expect(recordMock).toHaveBeenCalledTimes(1)
      expect(askCmdrState.messages).toEqual([])
    } finally {
      vi.useRealTimers()
    }
  })

  it('drops a recorded event that resolves after the user switched threads', async () => {
    vi.useFakeTimers()
    try {
      askCmdrState.conversationId = 7
      let resolveRecord: (v: unknown) => void = () => {}
      recordMock.mockImplementation(
        () =>
          new Promise((resolve) => {
            resolveRecord = resolve
          }),
      )
      noteSlotSettingChanged()
      await vi.advanceTimersByTimeAsync(1500)
      askCmdrState.conversationId = 8 // switched threads while the backend waited
      resolveRecord([eventRow(9, [{ type: 'modelChanged', model: 'model-two' }])])
      await vi.advanceTimersByTimeAsync(0)
      expect(askCmdrState.messages).toEqual([])
    } finally {
      vi.useRealTimers()
    }
  })
})

describe('openRail bootstrap + newChat + hydrate', () => {
  it('opens, bootstraps the most recent thread, and folds tool results into their lines', async () => {
    listMock.mockResolvedValue([conversationRow(9)])
    getMock.mockResolvedValue({
      conversation: conversationRow(9),
      totalMessages: 3,
      turnInFlightSinceSeq: null,
      messages: [
        {
          id: 1,
          seq: 0,
          role: 'user',
          blocks: [{ type: 'text', text: 'hi' }],
          promptTokens: null,
          completionTokens: null,
          createdAt: 0,
          turnId: null,
          duringTurn: false,
          turnStats: null,
        },
        {
          id: 2,
          seq: 1,
          role: 'assistant',
          blocks: [
            { type: 'text', text: 'hello' },
            {
              type: 'toolCall',
              callId: 'c1',
              tool: 'list_dir',
              subject: { kind: 'folder', path: { text: '/x', linkable: true } },
              outcome: { kind: 'items', returned: 1, total: 1, lowerBound: false },
              repeatOf: null,
            },
          ],
          promptTokens: null,
          completionTokens: null,
          createdAt: 0,
          turnId: null,
          duringTurn: false,
          turnStats: null,
        },
        {
          id: 3,
          seq: 2,
          role: 'tool',
          blocks: [],
          promptTokens: null,
          completionTokens: null,
          createdAt: 0,
          turnId: null,
          duringTurn: false,
          turnStats: null,
        },
      ],
    })
    await openRail()
    expect(askCmdrState.open).toBe(true)
    expect(askCmdrState.conversationId).toBe(9)
    expect(saveMock).toHaveBeenCalledWith({ askCmdrRailOpen: true })
    // The user + assistant items; the tool row draws nothing, its result rides the call's line.
    expect(askCmdrState.messages).toHaveLength(2)
    const assistant = assistantAt(1)
    expect(textOf(assistant)).toBe('hello')
    expect(linesOf(assistant)[0]).toMatchObject({
      callId: 'c1',
      tool: 'list_dir',
      subject: { kind: 'folder', path: { text: '/x', linkable: true } },
      outcome: { kind: 'items', returned: 1, total: 1, lowerBound: false },
    })
  })

  it('folds persisted event rows into model-change timeline lines', async () => {
    listMock.mockResolvedValue([conversationRow(9)])
    getMock.mockResolvedValue({
      conversation: conversationRow(9),
      totalMessages: 2,
      turnInFlightSinceSeq: null,
      messages: [
        {
          id: 1,
          seq: 0,
          role: 'user',
          blocks: [{ type: 'text', text: 'hi' }],
          promptTokens: null,
          completionTokens: null,
          createdAt: 0,
          turnId: null,
          duringTurn: false,
          turnStats: null,
        },
        {
          id: 2,
          seq: 1,
          role: 'event',
          blocks: [{ type: 'modelChanged', model: 'model-two' }],
          promptTokens: null,
          completionTokens: null,
          createdAt: 0,
          turnId: null,
          duringTurn: false,
          turnStats: null,
        },
      ],
    })
    await openRail()
    expect(askCmdrState.messages).toEqual([
      { kind: 'user', id: 1, text: 'hi', attachments: [] },
      { kind: 'modelChange', model: 'model-two' },
    ])
  })

  it('opens cleanly when there is no prior thread', async () => {
    listMock.mockResolvedValue([])
    await openRail()
    expect(askCmdrState.open).toBe(true)
    expect(askCmdrState.conversationId).toBeNull()
    expect(askCmdrState.messages).toHaveLength(0)
  })

  it('opens behind a closed gate without loading any history, and loads it once the gate opens', async () => {
    // Ask Cmdr off, or Cloud without consent: there's no chat to show, so nothing is fetched.
    gateMock.gate = 'cloudOff'
    listMock.mockResolvedValue([conversationRow(9)])
    getMock.mockResolvedValue({
      conversation: conversationRow(9),
      totalMessages: 0,
      messages: [],
      turnInFlightSinceSeq: null,
    })
    try {
      await openRail()
      expect(askCmdrState.open).toBe(true)
      expect(listMock).not.toHaveBeenCalled()

      // The rail calls this when its gate turns into the chat (the user allowed cloud AI).
      await ensureThreadLoaded()
      expect(askCmdrState.conversationId).toBe(9)
    } finally {
      gateMock.gate = 'chat'
    }
  })

  it('newChat clears the active thread', () => {
    askCmdrState.conversationId = 5
    askCmdrState.messages.push({ kind: 'user', id: null, text: 'x', attachments: [] })
    newChat()
    expect(askCmdrState.conversationId).toBeNull()
    expect(askCmdrState.messages).toHaveLength(0)
  })

  it('openNewChat on a closed rail opens it on a fresh chat, not the last thread, and asks for the composer', async () => {
    askCmdrState.conversationId = 5
    askCmdrState.messages.push({ kind: 'user', id: null, text: 'x', attachments: [] })
    listMock.mockResolvedValue([conversationRow(9)])
    const focusBefore = askCmdrState.composerFocusRequest
    openNewChat()
    await vi.waitFor(() => {
      expect(askCmdrState.open).toBe(true)
    })
    await Promise.resolve()
    expect(askCmdrState.conversationId).toBeNull()
    expect(askCmdrState.messages).toHaveLength(0)
    // The most recent thread is NOT bootstrapped over the fresh chat.
    expect(listMock).not.toHaveBeenCalled()
    expect(askCmdrState.composerFocusRequest).toBe(focusBefore + 1)
  })

  /** ⚠️ On the very first open, cloud consent is still loading, so `openRail` stops at the gate
   *  and the rail loads the last thread when the gate turns into the chat. That load used to
   *  replace the new chat the user had just asked for. */
  it('keeps a new chat opened while consent loads when the gate then opens', async () => {
    gateMock.gate = 'loading'
    listMock.mockResolvedValue([conversationRow(9)])
    try {
      openNewChat()
      await vi.waitFor(() => {
        expect(askCmdrState.open).toBe(true)
      })

      // What the rail does when its gate turns into the chat.
      await ensureThreadLoaded()

      expect(listMock).not.toHaveBeenCalled()
      expect(askCmdrState.conversationId).toBeNull()
    } finally {
      gateMock.gate = 'chat'
    }
  })

  it('a thread load ends the new chat', async () => {
    openNewChat()
    getMock.mockResolvedValue({
      conversation: conversationRow(9),
      totalMessages: 0,
      messages: [],
      turnInFlightSinceSeq: null,
    })
    await switchToThread(9)
    expect(askCmdrState.freshChat).toBe(false)
  })

  it('openNewChat on an open rail clears the thread and asks for the composer again', async () => {
    await openRail()
    askCmdrState.conversationId = 5
    growWindowMock.mockClear()
    const focusBefore = askCmdrState.composerFocusRequest
    openNewChat()
    expect(askCmdrState.conversationId).toBeNull()
    expect(askCmdrState.composerFocusRequest).toBe(focusBefore + 1)
    expect(growWindowMock).not.toHaveBeenCalled()
  })

  it('names the New chat shortcut for its tooltip, in the platform display form', () => {
    // ⌘N on macOS; this suite's navigator isn't a Mac, so the platform form is Ctrl+N.
    expect(newChatShortcutText()).toBe(toDisplayShortcut(toPlatformShortcut('⌘N')))
  })

  it('hydrateRail applies a persisted width and opens when the flag is set', () => {
    hydrateRail(true, 420)
    expect(askCmdrState.width).toBe(420)
    expect(askCmdrState.open).toBe(true)
  })
})

describe('window growth wiring', () => {
  it('grows the main window by the current rail width on a real open', async () => {
    askCmdrState.width = 360
    await openRail()
    expect(growWindowMock).toHaveBeenCalledWith(360)
  })

  it('does not grow the window when the rail is already open', async () => {
    askCmdrState.open = true
    await openRail()
    expect(growWindowMock).not.toHaveBeenCalled()
  })

  it('does not grow the window on startup hydration (the window is already rail-inclusive)', () => {
    hydrateRail(true, 420)
    expect(growWindowMock).not.toHaveBeenCalled()
  })

  it('shrinks the window back when the rail closes', () => {
    askCmdrState.open = true
    askCmdrState.width = 300
    closeRail()
    expect(shrinkWindowMock).toHaveBeenCalledWith(300)
  })
})

describe('setRailWidth', () => {
  it('persists the width and clamps to the bounds', () => {
    setRailWidth(400)
    expect(askCmdrState.width).toBe(400)
    expect(saveMock).toHaveBeenCalledWith({ askCmdrRailWidth: 400 })
    setRailWidth(99999)
    expect(askCmdrState.width).toBe(RAIL_MAX_WIDTH)
    setRailWidth(10)
    expect(askCmdrState.width).toBe(RAIL_MIN_WIDTH)
  })
})

describe('isOverSoftCap', () => {
  it('trips only once the thread crosses the soft cap', () => {
    for (let i = 0; i <= THREAD_SOFT_CAP_MESSAGES; i++) {
      askCmdrState.messages.push({ kind: 'user', id: null, text: `m${String(i)}`, attachments: [] })
    }
    expect(askCmdrState.messages.length).toBe(THREAD_SOFT_CAP_MESSAGES + 1)
    expect(isOverSoftCap()).toBe(true)
  })

  /** A turn is one bubble, but each of its rounds counts, as each assistant row did when it was
   *  its own bubble: a long tool-heavy thread still gets the nudge on time. */
  it("counts a bubble's rounds, not the bubble", () => {
    const rounds = Array.from({ length: THREAD_SOFT_CAP_MESSAGES }, (_, i) => ({
      messageId: i + 2,
      text: 'x',
      calls: [],
    }))
    askCmdrState.messages = [
      { kind: 'user', id: 1, text: 'hi', attachments: [] },
      {
        kind: 'assistant',
        id: null,
        turnId: 1,
        rounds,
        thinking: false,
        stalled: false,
        streaming: false,
        stats: null,
      },
    ]
    expect(isOverSoftCap()).toBe(true)
  })
})

/** A minimal user-role MessageView for paging fixtures. */
function userMessage(id: number, seq: number) {
  return {
    id,
    seq,
    role: 'user' as const,
    blocks: [{ type: 'text' as const, text: `m${String(seq)}` }],
    promptTokens: null,
    completionTokens: null,
    createdAt: 0,
    turnId: null,
    duringTurn: false,
    turnStats: null,
  }
}

/** A minimal text-only assistant-role MessageView. */
function assistantRow(id: number, seq: number, text: string) {
  return { ...userMessage(id, seq), role: 'assistant' as const, blocks: [{ type: 'text' as const, text }] }
}

/** A detail page covering seqs [offset, offset+limit) out of `total`. */
function detailPage(total: number, limit: number, offset: number) {
  const upper = Math.min(offset + limit, total)
  const messages = []
  for (let seq = offset; seq < upper; seq++) messages.push(userMessage(seq + 1, seq))
  return { conversation: conversationRow(9), totalMessages: total, messages, turnInFlightSinceSeq: null }
}

describe('message paging (tail-first, load older)', () => {
  it('loads the most recent page for a long thread and offers older', async () => {
    const total = MESSAGE_PAGE + 20
    getMock.mockImplementation((...args: unknown[]) => {
      const [, limit, offset] = args as [number, number, number]
      return Promise.resolve(detailPage(total, limit, offset))
    })
    await switchToThread(9)
    // A long thread refetches its tail: the last page, not the oldest.
    expect(getMock).toHaveBeenLastCalledWith(9, MESSAGE_PAGE, total - MESSAGE_PAGE)
    expect(askCmdrState.messages).toHaveLength(MESSAGE_PAGE)
    expect(askCmdrState.historyCount).toBe(MESSAGE_PAGE)
    expect(hasOlderMessages()).toBe(true)
    // The newest message shown is the last one (seq total-1).
    const last = askCmdrState.messages.at(-1)
    expect(last?.kind === 'user' && last.text).toBe(`m${String(total - 1)}`)
  })

  it('load-older prepends the previous page with a non-overlapping offset', async () => {
    const total = MESSAGE_PAGE + 20
    getMock.mockImplementation((...args: unknown[]) => {
      const [, limit, offset] = args as [number, number, number]
      return Promise.resolve(detailPage(total, limit, offset))
    })
    await switchToThread(9)
    await loadOlderMessages()
    // The older page starts at 0 and fills the gap up to the tail (20 messages).
    expect(getMock).toHaveBeenLastCalledWith(9, 20, 0)
    expect(askCmdrState.messages).toHaveLength(total)
    expect(askCmdrState.historyCount).toBe(total)
    expect(hasOlderMessages()).toBe(false)
    // Ordering holds: oldest at the top, newest at the bottom, no gaps or repeats.
    const seqs = askCmdrState.messages.map((m) => (m.kind === 'user' ? m.text : ''))
    expect(seqs).toEqual(Array.from({ length: total }, (_, i) => `m${String(i)}`))
  })

  /** "Load earlier" is a read like any other, so the user can leave the thread before it
   *  answers. Its page belongs to the thread it was asked for, never the one now on screen. */
  it('drops an earlier page that answers after the user switched threads', async () => {
    const total = MESSAGE_PAGE + 20
    getMock.mockImplementation((...args: unknown[]) => {
      const [, limit, offset] = args as [number, number, number]
      return Promise.resolve(detailPage(total, limit, offset))
    })
    await switchToThread(9)
    let answerOlder: (detail: unknown) => void = () => {}
    getMock.mockReturnValueOnce(new Promise((resolve) => (answerOlder = resolve)))
    const older = loadOlderMessages()

    getMock.mockResolvedValue(detailPage(2, MESSAGE_PAGE, 0))
    await switchToThread(2)
    answerOlder(detailPage(total, 20, 0))
    await older

    expect(askCmdrState.messages).toHaveLength(2)
    expect(askCmdrState.historyCount).toBe(2)
    expect(askCmdrState.loadingOlder).toBe(false)
  })

  /** A tool-heavy turn is many rows, so page boundaries cut turns routinely. Loading the older
   *  half joins it back into the one bubble a whole-thread load would show. */
  it('load-older folds a turn the page boundary split back into one bubble', async () => {
    const total = MESSAGE_PAGE + 2
    const rows: MessageView[] = Array.from({ length: total }, (_, seq) => userMessage(seq + 1, seq))
    // Rows 0–2 are one turn: its opener, then two rounds; the tail page starts at its second round.
    rows[0] = { ...rows[0], turnId: 1 }
    rows[1] = { ...assistantRow(2, 1, 'first half'), turnId: 1 }
    rows[2] = { ...assistantRow(3, 2, 'second half'), turnId: 1 }
    getMock.mockImplementation((...args: unknown[]) => {
      const [, limit, offset] = args as [number, number, number]
      return Promise.resolve({
        conversation: conversationRow(9),
        totalMessages: total,
        turnInFlightSinceSeq: null,
        messages: rows.slice(offset, offset + limit),
      })
    })
    await switchToThread(9)
    expect(askCmdrState.messages[0]).toMatchObject({ kind: 'assistant', id: 3 })

    await loadOlderMessages()

    expect(askCmdrState.messages.slice(0, 3).map((m) => m.kind)).toEqual(['user', 'assistant', 'user'])
    expect(assistantAt(1)).toMatchObject({ id: 3, turnId: 1 })
    expect(assistantAt(1).rounds.map((round) => round.text)).toEqual(['first half', 'second half'])
  })

  /** ⚠️ The fold builds the joined bubble anew, so a stop still waiting on the newer half held a
   *  bubble no longer on screen, and its terminal settled that one instead. */
  it("settles a stopped turn's bubble on its terminal after load-older joined it", async () => {
    const total = MESSAGE_PAGE + 2
    // One running turn the whole thread long: its opener, then a round per row.
    const rows: MessageView[] = Array.from({ length: total }, (_, seq) => ({
      ...assistantRow(seq + 1, seq, `round ${String(seq)}`),
      turnId: 1,
    }))
    rows[0] = { ...userMessage(1, 0), turnId: 1 }
    getMock.mockImplementation((...args: unknown[]) => {
      const [, limit, offset] = args as [number, number, number]
      return Promise.resolve({
        conversation: conversationRow(9),
        totalMessages: total,
        turnInFlightSinceSeq: 0,
        messages: rows.slice(offset, offset + limit),
      })
    })
    await switchToThread(9)
    stopStreaming()

    await loadOlderMessages()
    fire({ type: 'stopped', turn: STATS })

    expect(assistantAt(1)).toMatchObject({ turnId: 1, streaming: false, stats: STATS })
  })

  it('a short thread loads in one page with nothing older', async () => {
    getMock.mockImplementation((...args: unknown[]) => {
      const [, limit, offset] = args as [number, number, number]
      return Promise.resolve(detailPage(3, limit, offset))
    })
    await switchToThread(9)
    expect(askCmdrState.messages).toHaveLength(3)
    expect(hasOlderMessages()).toBe(false)
  })
})

describe('attachments', () => {
  it('adds and de-duplicates by path, and removes by path', () => {
    addAttachments([
      { path: '/a', kind: 'file' },
      { path: '/b', kind: 'folder' },
    ])
    addAttachments([{ path: '/a', kind: 'file' }]) // duplicate ignored
    expect(askCmdrState.attachments.map((a) => a.path)).toEqual(['/a', '/b'])
    removeAttachment('/a')
    expect(askCmdrState.attachments.map((a) => a.path)).toEqual(['/b'])
  })

  it('send passes staged attachments, echoes them on the user bubble, then clears them', () => {
    addAttachments([{ path: '/a', kind: 'file' }])
    sendMessage('about this')
    expect(sendMock).toHaveBeenCalledWith(null, 'about this', [{ path: '/a', kind: 'file' }], [])
    const first = askCmdrState.messages[0]
    expect(first.kind === 'user' && first.attachments).toEqual([{ path: '/a', kind: 'file' }])
    // Staged attachments are cleared after the send.
    expect(askCmdrState.attachments).toEqual([])
  })
})

describe('switchToThread', () => {
  it('loads the chosen thread into the rail', async () => {
    getMock.mockResolvedValue(detailPage(2, MESSAGE_PAGE, 0))
    await switchToThread(9)
    expect(askCmdrState.conversationId).toBe(9)
    expect(askCmdrState.messages).toHaveLength(2)
  })

  /** Thread reads that answer when the test says so, keyed by thread id. */
  function heldReads(): { answer: (read: { thread: number; messages: number }) => void } {
    const resolvers: Record<number, (detail: unknown) => void> = {}
    getMock.mockImplementation((...args: unknown[]) => {
      const [id] = args as [number]
      return new Promise((resolve) => (resolvers[id] = resolve))
    })
    return {
      answer: ({ thread, messages }) => {
        resolvers[thread](detailPage(messages, MESSAGE_PAGE, 0))
      },
    }
  }

  /** Two reads are two independent commands, so the first one asked can be the last one
   *  answered. The thread the user picked LAST is the one that stays on screen. */
  it('keeps the thread the user switched to when an earlier one answers late', async () => {
    const reads = heldReads()
    const first = switchToThread(1)
    const second = switchToThread(2)

    reads.answer({ thread: 2, messages: 2 })
    await second
    reads.answer({ thread: 1, messages: 5 })
    await first

    expect(askCmdrState.conversationId).toBe(2)
    expect(askCmdrState.messages).toHaveLength(2)
  })

  it('stays loading until the latest switch has answered, not the first one to finish', async () => {
    const reads = heldReads()
    const first = switchToThread(1)
    const second = switchToThread(2)

    reads.answer({ thread: 1, messages: 5 })
    await first
    expect(askCmdrState.loadingHistory).toBe(true)
    expect(askCmdrState.messages).toEqual([])

    reads.answer({ thread: 2, messages: 2 })
    await second
    expect(askCmdrState.loadingHistory).toBe(false)
  })

  it('leaves a new chat empty when the thread it replaced answers late', async () => {
    const reads = heldReads()
    const loading = switchToThread(1)

    newChat()
    reads.answer({ thread: 1, messages: 5 })
    await loading

    expect(askCmdrState.conversationId).toBeNull()
    expect(askCmdrState.messages).toEqual([])
    expect(askCmdrState.loadingHistory).toBe(false)
  })
})
