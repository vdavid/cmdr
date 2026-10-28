/**
 * The live reducer folds a turn into rounds, and the bubble it builds is the one a reload folds
 * from the stored rows: narration before the calls it introduced, each line on its own
 * `(messageId, callId)`, a mid-turn decision above the bubble.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type {
  AskCmdrSendOutcome,
  AskCmdrStreamEvent,
  ConversationDetailView,
  MessageBlock,
  MessageView,
  ProposalDecision,
} from '#lib/tauri-commands/index.js'
import type { ToolOutcome, ToolSubject, TurnStats } from '#lib/ipc/bindings.js'

vi.mock('#lib/tauri-commands/index.js', () => ({
  sendAskCmdrMessage: vi.fn((c: number | null): Promise<AskCmdrSendOutcome> =>
    Promise.resolve({ accepted: true, conversationId: c ?? 1, run: 1 }),
  ),
  cancelAskCmdr: vi.fn(() => Promise.resolve({ tripped: 1, live: 1 })),
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

import { askCmdrState, newChat, sendMessage, type RailMessage } from './ask-cmdr-trigger.svelte'
import { handleTurnEvent, resetTurnBookkeeping } from './ask-cmdr-stream.svelte'
import { buildRailMessages } from './ask-cmdr-history'
import type { AssistantItem } from './ask-cmdr-messages'

const THREAD = 1
const DRIVES: ToolSubject = { kind: 'drives' }
const ITEMS: ToolOutcome = { kind: 'items', returned: 2, total: 2, lowerBound: false }
const STATS: TurnStats = { toolCalls: 2, wallMs: 2_400, promptTokens: 120, completionTokens: 30 }
const DECISION: ProposalDecision = { verb: 'trash', what: '/Users/dana/old.dmg', ops: 1, outcome: { kind: 'rejected' } }

function fire(event: AskCmdrStreamEvent): void {
  askCmdrState.conversationId = THREAD
  handleTurnEvent({ conversationId: THREAD, run: null, event })
}

/** One stored row as the backend projects it; `id` doubles as `seq`. */
function view(
  id: number,
  role: MessageView['role'],
  blocks: MessageBlock[],
  extra: Partial<MessageView> = {},
): MessageView {
  return {
    id,
    seq: id,
    role,
    blocks,
    promptTokens: null,
    completionTokens: null,
    createdAt: 0,
    turnId: 1,
    duringTurn: false,
    turnStats: null,
    ...extra,
  }
}

function thread(messages: MessageView[]): ConversationDetailView {
  return {
    conversation: { id: THREAD, title: 't', createdAt: 0, updatedAt: 0, archived: false, origin: null },
    messages,
    totalMessages: messages.length,
    lastContextUsage: null,
    turnInFlightSinceSeq: null,
  }
}

function assistantAt(index: number): AssistantItem {
  const message = askCmdrState.messages[index]
  if (message.kind !== 'assistant') throw new Error('expected an assistant message')
  return message
}

beforeEach(() => {
  newChat()
  resetTurnBookkeeping()
  askCmdrState.messages = []
  askCmdrState.conversationId = null
  askCmdrState.streaming = false
})

describe('the live reducer, round by round', () => {
  /** The wire may interleave a round's text with its starts; the stored row holds the text
   *  first, and so does the round, so the narration renders above the calls it introduced. */
  it("routes a round's text into that round, ahead of its calls", () => {
    sendMessage('look')
    fire({ type: 'assistantStarted' })
    fire({ type: 'textDelta', text: 'Let me ' })
    fire({ type: 'toolCallStarted', callId: 'c1', tool: 'list_dir' })
    fire({ type: 'textDelta', text: 'look.' })
    fire({ type: 'toolCallStarted', callId: 'c2', tool: 'app_state' })

    expect(assistantAt(1).rounds).toMatchObject([
      { messageId: null, text: 'Let me look.', calls: [{ callId: 'c1' }, { callId: 'c2' }] },
    ])
  })

  /** The batch is the round's truth: a call whose start was never heard joins with its real
   *  label, and a nameless call the backend discarded leaves. */
  it('adds a line the batch lists that never started, and drops one it does not list', () => {
    sendMessage('look')
    fire({ type: 'assistantStarted' })
    fire({ type: 'toolCallStarted', callId: 'c1', tool: 'list_dir' })
    fire({ type: 'toolCallStarted', callId: 'nameless', tool: '' })
    fire({
      type: 'toolCallsResolved',
      messageId: 2,
      calls: [
        {
          callId: 'c1',
          tool: 'list_dir',
          subject: { kind: 'folder', path: { text: '/a', linkable: true } },
          repeatOf: null,
        },
        { callId: 'c3', tool: 'list_volumes', subject: DRIVES, repeatOf: null },
      ],
    })

    expect(assistantAt(1).rounds).toEqual([
      {
        messageId: 2,
        text: '',
        calls: [
          {
            callId: 'c1',
            tool: 'list_dir',
            running: true,
            subject: { kind: 'folder', path: { text: '/a', linkable: true } },
            outcome: null,
            repeatOf: null,
          },
          { callId: 'c3', tool: 'list_volumes', running: true, subject: DRIVES, outcome: null, repeatOf: null },
        ],
      },
    ])
  })

  /** Rounds reuse call ids as the norm (the fake mints `fake-call-0` per round), so a finished
   *  call lands on its own round's line and no other. */
  it('settles each line by (messageId, callId) when rounds reuse a call id', () => {
    sendMessage('look')
    for (const messageId of [2, 4]) {
      fire({ type: 'assistantStarted' })
      fire({ type: 'toolCallStarted', callId: 'fake-call-0', tool: 'list_volumes' })
      fire({
        type: 'toolCallsResolved',
        messageId,
        calls: [{ callId: 'fake-call-0', tool: 'list_volumes', subject: DRIVES, repeatOf: null }],
      })
    }
    fire({ type: 'toolCallFinished', messageId: 2, callId: 'fake-call-0', outcome: ITEMS, subject: DRIVES })

    expect(assistantAt(1).rounds.map((round) => [round.messageId, round.calls[0].outcome])).toEqual([
      [2, ITEMS],
      [4, null],
    ])
  })

  /** A reload mid-turn folds every stored round sealed; the chunks still to come belong to a
   *  round of their own, never to a stored one. */
  it('opens a new round after a reload instead of writing into a stored one', () => {
    askCmdrState.messages = buildRailMessages({
      ...thread([
        view(1, 'user', [{ type: 'text', text: 'look' }]),
        view(2, 'assistant', [{ type: 'text', text: 'Checking.' }]),
      ]),
      // The turn has been running since before its opener.
      turnInFlightSinceSeq: 0,
    })

    fire({ type: 'textDelta', text: 'Found it.' })

    expect(assistantAt(1).rounds.map((round) => [round.messageId, round.text])).toEqual([
      [2, 'Checking.'],
      [null, 'Found it.'],
    ])
    expect(askCmdrState.streaming).toBe(true)
  })

  it('takes the turn identity from the persisted opener', () => {
    sendMessage('look')
    fire({ type: 'assistantStarted' })
    fire({ type: 'textDelta', text: 'Checking.' })
    fire({
      type: 'userPersisted',
      messageId: 11,
      seq: 11,
      opener: view(11, 'user', [{ type: 'text', text: 'look' }], { turnId: 11 }),
    })

    expect(askCmdrState.messages[0]).toMatchObject({ kind: 'user', id: 11 })
    expect(assistantAt(1).turnId).toBe(11)
  })

  /** The answer round is text-only, so nothing but `done` stamps it; without that, settling would
   *  drop the answer as never stored. */
  it('keeps the final answer, stamped by done', () => {
    sendMessage('hi')
    fire({ type: 'assistantStarted' })
    fire({ type: 'textDelta', text: 'Hello.' })
    fire({
      type: 'done',
      messageId: 2,
      seq: 2,
      stop: 'completed',
      usage: { promptTokens: 1, completionTokens: 1 },
      turn: null,
    })

    expect(assistantAt(1)).toMatchObject({ id: 2, rounds: [{ messageId: 2, text: 'Hello.', calls: [] }] })
  })
})

describe('live equals reload', () => {
  /** The same turn, streamed and then reopened, must look the same: this is the guard against
   *  one half drifting from the other. */
  it('builds the bubble a reload of the stored rows folds', () => {
    sendMessage('what drives do I have?')
    fire({ type: 'started' })
    fire({ type: 'assistantStarted' })
    fire({ type: 'textDelta', text: 'Let me ' })
    fire({ type: 'toolCallStarted', callId: 'fake-call-0', tool: 'list_volumes' })
    fire({ type: 'textDelta', text: 'look.' })
    fire({
      type: 'userPersisted',
      messageId: 1,
      seq: 1,
      opener: view(1, 'user', [{ type: 'text', text: 'what drives do I have?' }]),
    })
    fire({
      type: 'toolCallsResolved',
      messageId: 2,
      calls: [{ callId: 'fake-call-0', tool: 'list_volumes', subject: DRIVES, repeatOf: null }],
    })
    fire({ type: 'toolCallFinished', messageId: 2, callId: 'fake-call-0', outcome: ITEMS, subject: DRIVES })
    // The user answered an earlier suggestion while the turn ran.
    fire({ type: 'proposalDecided', messageId: 4, seq: 4, decision: DECISION })
    fire({ type: 'assistantStarted' })
    fire({ type: 'toolCallStarted', callId: 'fake-call-0', tool: 'list_volumes' })
    const repeatOf = { messageId: 2, callId: 'fake-call-0' }
    fire({
      type: 'toolCallsResolved',
      messageId: 5,
      calls: [{ callId: 'fake-call-0', tool: 'list_volumes', subject: DRIVES, repeatOf }],
    })
    fire({ type: 'toolCallFinished', messageId: 5, callId: 'fake-call-0', outcome: ITEMS, subject: DRIVES })
    fire({ type: 'assistantStarted' })
    fire({ type: 'reasoningTick' })
    fire({ type: 'textDelta', text: 'You have two drives.' })
    fire({ type: 'contextUsage', estimatedTokens: 900, budgetTokens: 8_000, elidedResults: 0 })
    fire({
      type: 'done',
      messageId: 7,
      seq: 7,
      stop: 'completed',
      usage: { promptTokens: 60, completionTokens: 20 },
      turn: STATS,
    })
    // And again once it had ended.
    fire({ type: 'proposalDecided', messageId: 9, seq: 9, decision: DECISION })

    const call = (extra: Partial<Extract<MessageBlock, { type: 'toolCall' }>> = {}): MessageBlock => ({
      type: 'toolCall',
      callId: 'fake-call-0',
      tool: 'list_volumes',
      subject: DRIVES,
      outcome: ITEMS,
      repeatOf: null,
      ...extra,
    })
    const reloaded: RailMessage[] = buildRailMessages(
      thread([
        view(1, 'user', [{ type: 'text', text: 'what drives do I have?' }]),
        view(2, 'assistant', [{ type: 'text', text: 'Let me look.' }, call()]),
        view(3, 'tool', []),
        view(4, 'event', [{ type: 'proposalDecisions', decisions: [DECISION] }], { duringTurn: true }),
        view(5, 'assistant', [call({ repeatOf })]),
        view(6, 'tool', []),
        view(7, 'assistant', [{ type: 'text', text: 'You have two drives.' }], { turnStats: STATS }),
        view(8, 'event', []),
        view(9, 'event', [{ type: 'proposalDecisions', decisions: [DECISION] }]),
      ]),
    )

    expect(reloaded.map((item) => item.kind)).toEqual(['user', 'proposalDecisions', 'assistant', 'proposalDecisions'])
    expect(askCmdrState.messages).toEqual(reloaded)
  })
})
