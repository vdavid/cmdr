/**
 * A Stop and a reload mid-turn both end in the view a later reload shows. A stop ends the turn on
 * screen at once and the backend's terminal settles its bubble; a reload folds the running turn
 * as running, adopts it, and lets the live events finish it.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { AskCmdrSendOutcome, AskCmdrStreamEvent, MessageBlock, MessageView } from '#lib/tauri-commands/index.js'
import type { CancelOutcomeView, ResolvedCall, ToolOutcome, ToolSubject, TurnStats } from '#lib/ipc/bindings.js'

const sendMock = vi.fn<(c: number | null, t: string, a: unknown[], d: string[]) => Promise<AskCmdrSendOutcome>>()
const cancelMock = vi.fn<(id: number) => Promise<CancelOutcomeView>>()
const getMock = vi.fn<(...a: unknown[]) => Promise<unknown>>()

vi.mock('#lib/tauri-commands/index.js', () => ({
  sendAskCmdrMessage: (c: number | null, t: string, a: unknown[], d: string[]) => sendMock(c, t, a, d),
  cancelAskCmdr: (id: number) => cancelMock(id),
  listAskCmdrConversations: vi.fn(() => Promise.resolve([])),
  getAskCmdrConversation: (...a: unknown[]) => getMock(...a),
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

import {
  askCmdrState,
  newChat,
  sendMessage,
  stopStreaming,
  switchToThread,
  type RailMessage,
  type RailToolCall,
} from './ask-cmdr-trigger.svelte'
import { handleTurnEvent, resetTurnBookkeeping } from './ask-cmdr-stream.svelte'

/** The thread a fired event belongs to when the test doesn't name one. */
const THREAD_ID = 1

const ANSWERED: ToolOutcome = { kind: 'answered' }
const REFUSED: ToolOutcome = { kind: 'refused', reason: { kind: 'problem' } }
const UNPARSED: ToolSubject = { kind: 'unparsed' }
const PANES: ToolSubject = { kind: 'panes' }
const STATS: TurnStats = { toolCalls: 2, wallMs: 1200, promptTokens: 10, completionTokens: 2 }

/** One call of a `toolCallsResolved` batch. */
function resolved(callId: string, tool: string): ResolvedCall {
  return { callId, tool, subject: UNPARSED, repeatOf: null }
}

/** Feed one stream event in as the backend emits it, for the thread the rail is on (or the named
 * one, which the rail is put on first), from the named turn (`run`) when the test cares which. */
function fire(
  event: AskCmdrStreamEvent,
  conversationId: number = askCmdrState.conversationId ?? THREAD_ID,
  run: number | null = null,
): void {
  askCmdrState.conversationId = conversationId
  handleTurnEvent({ conversationId, run, event })
}

beforeEach(() => {
  sendMock.mockReset()
  cancelMock.mockReset()
  getMock.mockReset()
  sendMock.mockImplementation((c) => Promise.resolve({ accepted: true, conversationId: c ?? THREAD_ID, run: 1 }))
  // The usual Stop: it reaches the one turn running, whose terminal is still to come.
  cancelMock.mockResolvedValue({ tripped: 1, live: 1 })
  newChat()
  resetTurnBookkeeping()
  askCmdrState.messages = []
  askCmdrState.conversationId = null
  askCmdrState.renameReview = null
})

/** Let a resolved cancel's answer land. */
async function flushMicrotasks(): Promise<void> {
  for (let i = 0; i < 3; i++) await Promise.resolve()
}

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

describe('stopStreaming', () => {
  /** A round of `partial` narration whose row persisted, then a second round still streaming. */
  function streamPastOneRound(): void {
    sendMessage('long one')
    fire({ type: 'started' }, 3)
    fire({ type: 'assistantStarted' })
    fire({ type: 'textDelta', text: 'partial' })
    fire({ type: 'toolCallStarted', callId: 'c1', tool: 'list_dir' })
    fire({ type: 'toolCallsResolved', messageId: 5, calls: [resolved('c1', 'list_dir')] })
    fire({ type: 'assistantStarted' })
    fire({ type: 'textDelta', text: 'never stored' })
  }

  const answer = (messageId: number): AskCmdrStreamEvent => ({
    type: 'done',
    messageId,
    seq: messageId,
    stop: 'completed',
    usage: { promptTokens: 1, completionTokens: 1 },
    turn: STATS,
  })

  /** The click ends the turn on screen, but dispatch isn't cancellable: the round's calls still
   *  run and store their results. So the bubble keeps what may yet be stored until the backend
   *  says how the turn ended. */
  it('ends the turn on screen at once and keeps what the backend may still store', () => {
    streamPastOneRound()
    stopStreaming()
    expect(cancelMock).toHaveBeenCalledWith(3)
    expect(askCmdrState.streaming).toBe(false)
    expect(assistantAt(1)).toMatchObject({ streaming: false, thinking: false, stalled: false })
    expect(assistantAt(1).rounds.map((round) => round.text)).toEqual(['partial', 'never stored'])
    expect(linesOf(assistantAt(1))).toMatchObject([{ callId: 'c1', running: true }])
  })

  /** The stopped turn's terminal settles its bubble the way a reload reads the stored rows, and
   *  the footer shows what the turn spent: it matters most when the turn didn't deliver. */
  it("settles the bubble on the stopped turn's terminal, with its stats", () => {
    streamPastOneRound()
    stopStreaming()

    fire({ type: 'stopped', turn: STATS })
    expect(assistantAt(1)).toMatchObject({ id: 5, streaming: false, stats: STATS })
    expect(textOf(assistantAt(1))).toBe('partial')
    expect(linesOf(assistantAt(1))).toMatchObject([{ callId: 'c1', running: false, outcome: null }])
    expect(askCmdrState.streaming).toBe(false)

    fire({ type: 'textDelta', text: 'a later turn' })
    expect(askCmdrState.streaming).toBe(true)
    expect(askCmdrState.messages.map((m) => m.kind)).toEqual(['user', 'assistant', 'assistant'])
  })

  /** After the click, the calls the round still runs report back. Their lines settle where they
   *  are, and none of it puts the rail back into a working state. */
  it("lets the stopped turn's calls settle their lines without re-entering the turn", () => {
    streamPastOneRound()
    fire({ type: 'toolCallStarted', callId: 'c1', tool: 'app_state' })
    stopStreaming()

    fire({ type: 'toolCallFinished', messageId: 5, callId: 'c1', outcome: ANSWERED, subject: PANES })
    fire({ type: 'toolCallsResolved', messageId: 7, calls: [resolved('c1', 'app_state')] })
    fire({ type: 'textDelta', text: 'dropped' })
    fire({ type: 'toolCallFinished', messageId: 7, callId: 'c1', outcome: REFUSED, subject: PANES })

    expect(askCmdrState.streaming).toBe(false)
    expect(assistantAt(1).rounds).toMatchObject([
      { messageId: 5, calls: [{ running: false, outcome: ANSWERED }] },
      { messageId: 7, text: 'never stored', calls: [{ tool: 'app_state', running: false, outcome: REFUSED }] },
    ])
  })

  it('keeps the answer when the stopped turn answered anyway', () => {
    streamPastOneRound()
    stopStreaming()

    fire(answer(9))

    expect(assistantAt(1)).toMatchObject({ id: 9, stats: STATS })
    expect(assistantAt(1).rounds.map((round) => [round.messageId, round.text])).toEqual([
      [5, 'partial'],
      [9, 'never stored'],
    ])
    expect(askCmdrState.streaming).toBe(false)
  })

  /** The user stopped it, so a failure after the click is not news: the bubble settles, with no
   *  notice under it. */
  it('settles quietly when the stopped turn failed', () => {
    streamPastOneRound()
    stopStreaming()

    fire({ type: 'failed', kind: 'provider', detail: null, managed: null, turn: STATS })

    expect(askCmdrState.messages.map((m) => m.kind)).toEqual(['user', 'assistant'])
    expect(assistantAt(1)).toMatchObject({ id: 5, stats: STATS })
    expect(textOf(assistantAt(1))).toBe('partial')
  })

  /** The next send queues behind the stopped turn, so until that turn's terminal, every event
   *  is still the stopped turn's: its stats land on its own bubble, never on the new one. */
  it("puts the stopped turn's stats on its own bubble even after a new send", () => {
    streamPastOneRound()
    stopStreaming()
    sendMessage('next')

    fire({ type: 'textDelta', text: 'still the stopped turn' })
    fire({ type: 'stopped', turn: STATS })
    expect(assistantAt(1)).toMatchObject({ stats: STATS, streaming: false })
    expect(textOf(assistantAt(1))).toBe('partial')
    expect(askCmdrState.streaming).toBe(true)

    fire({ type: 'assistantStarted' })
    fire({ type: 'textDelta', text: 'the new turn' })
    expect(askCmdrState.messages.map((m) => m.kind)).toEqual(['user', 'assistant', 'user', 'assistant'])
    expect(textOf(assistantAt(3))).toBe('the new turn')
  })

  /** Each stop waits for its own turn's terminal, oldest first. */
  it('settles two stopped turns in order', () => {
    streamPastOneRound()
    stopStreaming()
    sendMessage('next')
    stopStreaming()

    fire({ type: 'stopped', turn: STATS })
    fire({ type: 'stopped', turn: null })
    expect(assistantAt(1)).toMatchObject({ stats: STATS, streaming: false })
    expect(linesOf(assistantAt(1))).toMatchObject([{ running: false }])

    fire({ type: 'textDelta', text: 'a later turn' })
    expect(askCmdrState.streaming).toBe(true)
  })

  /** ⚠️ A Stop that reached no live turn (it had ended, or died without a terminal) has no
   *  terminal coming. Waiting for one anyway took the thread's NEXT turn for it: its events were
   *  dropped, and its answer landed on the stopped bubble. */
  it('settles a stop that reached no live turn at once, so the next turn is its own', async () => {
    cancelMock.mockResolvedValue({ tripped: 0, live: 0 })
    streamPastOneRound()
    stopStreaming()
    await flushMicrotasks()

    expect(textOf(assistantAt(1))).toBe('partial')
    expect(linesOf(assistantAt(1))).toMatchObject([{ callId: 'c1', running: false, outcome: null }])

    sendMessage('next')
    fire({ type: 'assistantStarted' })
    fire({ type: 'textDelta', text: 'the new turn' })
    expect(askCmdrState.streaming).toBe(true)
    fire(answer(9))

    expect(askCmdrState.messages.map((m) => m.kind)).toEqual(['user', 'assistant', 'user', 'assistant'])
    expect(assistantAt(3)).toMatchObject({ id: 9, streaming: false, stats: STATS })
    expect(textOf(assistantAt(3))).toBe('the new turn')
    expect(askCmdrState.streaming).toBe(false)
  })

  /** ⚠️ A terminal the backend emitted just before the Stop can reach the rail AFTER the Stop's
   *  `live: 0` answer already settled the turn. It belongs to the stopped turn, and it used to end
   *  whatever turn the user had sent since. Each event names its turn's run, so it can't. */
  describe("a stopped turn's late terminal", () => {
    async function stopFirstTurnThenSendAgain(): Promise<void> {
      cancelMock.mockResolvedValue({ tripped: 0, live: 0 })
      sendMock.mockResolvedValueOnce({ accepted: true, conversationId: 3, run: 1 })
      askCmdrState.conversationId = 3
      sendMessage('first')
      await flushMicrotasks()
      fire({ type: 'assistantStarted' }, 3, 1)
      fire({ type: 'textDelta', text: 'partial' }, 3, 1)
      stopStreaming()
      await flushMicrotasks()
      sendMock.mockResolvedValueOnce({ accepted: true, conversationId: 3, run: 2 })
      sendMessage('next')
    }

    it('ends nothing once the new send knows its run', async () => {
      await stopFirstTurnThenSendAgain()
      await flushMicrotasks()

      fire({ type: 'stopped', turn: null }, 3, 1)

      expect(askCmdrState.streaming).toBe(true)
      fire({ type: 'assistantStarted' }, 3, 2)
      fire({ type: 'textDelta', text: 'the new turn' }, 3, 2)
      expect(textOf(assistantAt(askCmdrState.messages.length - 1))).toBe('the new turn')
    })

    it("ends nothing even before the new send's answer arrives", async () => {
      await stopFirstTurnThenSendAgain()

      fire({ type: 'stopped', turn: null }, 3, 1)

      expect(askCmdrState.streaming).toBe(true)
    })
  })

  /** A Stop of a send queued behind a turn the rail wasn't tracking (a wake) trips both, so two
   *  terminals come. The second must not end a turn the user sent since. */
  it('waits for every terminal its Stop owes, so a later send keeps streaming', async () => {
    cancelMock.mockResolvedValue({ tripped: 2, live: 2 })
    askCmdrState.conversationId = 3
    sendMessage('queued behind a wake')
    fire({ type: 'queued' })
    stopStreaming()
    await flushMicrotasks()

    fire({ type: 'stopped', turn: STATS })
    sendMessage('third')
    fire({ type: 'stopped', turn: null })

    expect(askCmdrState.streaming).toBe(true)
    fire({ type: 'assistantStarted' })
    fire({ type: 'textDelta', text: 'the third turn' })
    expect(textOf(assistantAt(askCmdrState.messages.length - 1))).toBe('the third turn')
  })

  /** A send the backend refused never ran a turn, so no terminal will come for its stop. */
  it('withdraws the stop of a send that was refused before its turn ran', async () => {
    let answerSend: (outcome: AskCmdrSendOutcome) => void = () => {}
    sendMock.mockReturnValueOnce(new Promise((resolve) => (answerSend = resolve)))
    askCmdrState.conversationId = 3
    sendMessage('refused')
    stopStreaming()
    answerSend({ accepted: false, kind: 'provider', detail: null, managed: null })
    await Promise.resolve()

    fire({ type: 'textDelta', text: 'a later turn' })
    expect(askCmdrState.streaming).toBe(true)
  })

  /** The stopped turn can still stage a plan (its calls run on). The turn is over on screen, so
   *  the plan opens at once rather than waiting for a turn end the user already had. */
  it('opens a plan the stopped turn staged after the click', () => {
    streamPastOneRound()
    stopStreaming()

    fire({ type: 'proposalReady', proposal: { proposalId: 'p1', revision: 1, rows: [] } })

    expect(askCmdrState.renameReview?.proposals.map((p) => p.proposalId)).toEqual(['p1'])
  })

  it('drops a bubble nothing of which persisted, once its terminal says so', () => {
    sendMessage('long one')
    fire({ type: 'started' }, 3)
    fire({ type: 'assistantStarted' })
    fire({ type: 'textDelta', text: 'partial' })
    fire({ type: 'toolCallStarted', callId: 'c1', tool: 'list_dir' })

    stopStreaming()
    fire({ type: 'stopped', turn: null })

    expect(askCmdrState.messages.map((m) => m.kind)).toEqual(['user'])
  })

  it('stops the turn when the progress watchdog times out, and settles it on its terminal', async () => {
    vi.useFakeTimers()
    try {
      streamPastOneRound()

      await vi.advanceTimersByTimeAsync(90_000)

      expect(assistantAt(1)).toMatchObject({ thinking: false, stalled: false, streaming: false })
      expect(askCmdrState.messages.at(-1)).toEqual({ kind: 'error', errorKind: 'timeout' })

      fire({ type: 'stopped', turn: STATS })
      expect(linesOf(assistantAt(1))).toMatchObject([{ callId: 'c1', running: false, outcome: null }])
    } finally {
      vi.useRealTimers()
    }
  })

  /** The status corner stops a wake without the rail's Stop, so nothing here is waiting for the
   *  terminal: it ends the turn like an answer would, review included, and the watchdog never
   *  fires a "took too long" over a turn that ended. */
  it('ends a turn stopped from elsewhere like an answer', async () => {
    vi.useFakeTimers()
    try {
      askCmdrState.conversationId = 3
      fire({ type: 'assistantStarted' })
      fire({ type: 'textDelta', text: 'I noticed' })
      fire({ type: 'toolCallsResolved', messageId: 5, calls: [resolved('c1', 'list_dir')] })
      fire({ type: 'proposalReady', proposal: { proposalId: 'p1', revision: 1, rows: [] } })
      expect(askCmdrState.renameReview).toBeNull()

      fire({ type: 'stopped', turn: STATS })

      expect(askCmdrState.streaming).toBe(false)
      expect(askCmdrState.messages).toHaveLength(1)
      expect(assistantAt(0)).toMatchObject({ id: 5, streaming: false, stats: STATS })
      expect(askCmdrState.renameReview?.proposals.map((p) => p.proposalId)).toEqual(['p1'])
      await vi.advanceTimersByTimeAsync(90_000)
      expect(askCmdrState.messages.map((m) => m.kind)).toEqual(['assistant'])
    } finally {
      vi.useRealTimers()
    }
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

describe('a reload mid-turn', () => {
  /** A thread page whose turn has been running since row `since`. */
  function runningThread(rows: MessageView[], since: number) {
    return {
      conversation: conversationRow(9),
      totalMessages: rows.length,
      messages: rows,
      lastContextUsage: null,
      turnInFlightSinceSeq: since,
    }
  }

  /** The backend says a turn runs here, so the rail is in it at once, before any live event:
   *  Stop shows, and the line still waiting for its result keeps waiting. */
  it('adopts the running turn on load and keeps writing into its bubble', async () => {
    const call: MessageBlock = {
      type: 'toolCall',
      callId: 'c1',
      tool: 'app_state',
      subject: PANES,
      outcome: null,
      repeatOf: null,
    }
    getMock.mockResolvedValue(
      runningThread(
        [
          { ...userMessage(1, 1), turnId: 1 },
          { ...assistantRow(2, 2, 'Checking.'), blocks: [call], turnId: 1 },
        ],
        0,
      ),
    )
    await switchToThread(9)
    expect(askCmdrState.streaming).toBe(true)
    expect(linesOf(assistantAt(1))).toMatchObject([{ callId: 'c1', running: true }])

    fire({ type: 'toolCallFinished', messageId: 2, callId: 'c1', outcome: ANSWERED, subject: PANES })
    fire({ type: 'assistantStarted' })
    fire({ type: 'textDelta', text: 'Here.' })
    fire({
      type: 'done',
      messageId: 4,
      seq: 4,
      stop: 'completed',
      usage: { promptTokens: 1, completionTokens: 1 },
      turn: STATS,
    })

    expect(askCmdrState.messages.map((m) => m.kind)).toEqual(['user', 'assistant'])
    expect(assistantAt(1)).toMatchObject({ id: 4, streaming: false, stats: STATS })
    expect(assistantAt(1).rounds.map((round) => round.messageId)).toEqual([2, 4])
    expect(linesOf(assistantAt(1))).toMatchObject([{ running: false, outcome: ANSWERED }])
  })

  /** Reloaded before the running turn's first row landed: the bubble on screen is the previous
   *  turn's, so the live events open one of their own, and its opener, which the load never saw,
   *  goes in above it once it persists. */
  it('opens a new bubble for a turn with no row yet, and puts its opener above it', async () => {
    getMock.mockResolvedValue(
      runningThread(
        [
          { ...userMessage(1, 1), turnId: 1 },
          { ...assistantRow(2, 2, 'First.'), turnId: 1 },
        ],
        2,
      ),
    )
    await switchToThread(9)
    expect(askCmdrState.streaming).toBe(true)

    fire({ type: 'assistantStarted' })
    fire({ type: 'textDelta', text: 'Second.' })
    fire({
      type: 'userPersisted',
      messageId: 3,
      seq: 3,
      opener: { ...userMessage(3, 3), blocks: [{ type: 'text', text: 'and now?' }], turnId: 3 },
    })

    expect(askCmdrState.messages.map((m) => m.kind)).toEqual(['user', 'assistant', 'user', 'assistant'])
    expect(askCmdrState.messages[0]).toMatchObject({ id: 1, text: 'm1' })
    expect(askCmdrState.messages[2]).toEqual({ kind: 'user', id: 3, text: 'and now?', attachments: [] })
    expect(assistantAt(1).rounds.map((round) => round.text)).toEqual(['First.'])
    expect(assistantAt(3)).toMatchObject({ turnId: 3, streaming: true, rounds: [{ text: 'Second.' }] })
  })

  /** A wake (or a rejected sweep's follow-up) opens its turn with no user bubble typed here. Its
   *  opener goes in as its own item; the user bubble of an EARLIER turn keeps its own id. */
  it("inserts the opener of a turn nobody typed here, leaving an earlier bubble's id alone", () => {
    askCmdrState.messages = [
      { kind: 'user', id: 1, text: 'hi', attachments: [] },
      {
        kind: 'assistant',
        id: 2,
        turnId: 1,
        rounds: [{ messageId: 2, text: 'Hello.', calls: [] }],
        thinking: false,
        stalled: false,
        streaming: false,
        stats: null,
      },
    ]

    fire({ type: 'assistantStarted' })
    fire({ type: 'textDelta', text: 'I noticed something.' })
    fire({
      type: 'userPersisted',
      messageId: 5,
      seq: 5,
      opener: { ...userMessage(5, 5), blocks: [{ type: 'wakeDigest', folders: [], rollups: [] }], turnId: 5 },
    })

    expect(askCmdrState.messages.map((m) => m.kind)).toEqual(['user', 'assistant', 'wakeDigest', 'assistant'])
    expect(askCmdrState.messages[0]).toMatchObject({ kind: 'user', id: 1 })
    expect(assistantAt(3).turnId).toBe(5)
  })
})
