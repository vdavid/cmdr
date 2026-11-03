/**
 * The pure history fold: a loaded conversation becomes rail items, each turn one assistant bubble
 * made of rounds, each tool line reading the facts the backend paired onto its call.
 */

import { describe, expect, it } from 'vitest'
import { buildRailMessages } from './ask-cmdr-history'
import type { ConversationDetailView, MessageBlock, MessageView } from '#lib/tauri-commands/index.js'
import type { TurnStats } from '#lib/ipc/bindings.js'
import type { AssistantItem } from './ask-cmdr-messages'

function detail(messages: MessageView[]): ConversationDetailView {
  return {
    conversation: { id: 1, title: 't', createdAt: 0, updatedAt: 0, archived: false, origin: null, wakePlace: null },
    messages,
    totalMessages: messages.length,
    lastContextUsage: null,
    turnInFlightSinceSeq: null,
    turnInFlightRun: null,
    spendPausedMicros: null,
  }
}

/** One row as the backend projects it; `id` doubles as `seq`. */
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
    turnId: null,
    duringTurn: false,
    turnStats: null,
    ...extra,
  }
}

const text = (t: string): MessageBlock => ({ type: 'text', text: t })

function toolCall(callId: string, overrides: Partial<Extract<MessageBlock, { type: 'toolCall' }>> = {}): MessageBlock {
  return {
    type: 'toolCall',
    callId,
    tool: 'list_dir',
    subject: { kind: 'folder', path: { text: '/shots', linkable: true } },
    outcome: { kind: 'answered' },
    repeatOf: null,
    ...overrides,
  }
}

const STATS: TurnStats = { toolCalls: 2, wallMs: 1_500, promptTokens: 30, completionTokens: 9 }

function assistantAt(items: ReturnType<typeof buildRailMessages>, index: number): AssistantItem {
  const item = items[index]
  if (item.kind !== 'assistant') throw new Error('expected an assistant item')
  return item
}

describe('buildRailMessages', () => {
  /** One bubble per turn, one round per assistant row, each round's text before its calls. The
   *  bubble's id is the turn's last row, its stats the ones the backend put on that row, and the
   *  `TurnFinished` row's empty view draws nothing. */
  it('folds a multi-round turn into one bubble of rounds', () => {
    const items = buildRailMessages(
      detail([
        view(1, 'user', [text('what am I looking at?')], { turnId: 1 }),
        view(2, 'assistant', [text('Checking.'), toolCall('c1')], { turnId: 1 }),
        view(3, 'tool', [], { turnId: 1 }),
        view(4, 'assistant', [toolCall('c1', { tool: 'app_state', subject: { kind: 'panes' } })], { turnId: 1 }),
        view(5, 'tool', [], { turnId: 1 }),
        view(6, 'assistant', [text('Your '), text('screenshots.')], { turnId: 1, turnStats: STATS }),
        view(7, 'event', [], { turnId: 1 }),
      ]),
    )

    expect(items.map((item) => item.kind)).toEqual(['user', 'assistant'])
    expect(assistantAt(items, 1)).toEqual({
      kind: 'assistant',
      id: 6,
      turnId: 1,
      rounds: [
        {
          messageId: 2,
          text: 'Checking.',
          calls: [
            {
              callId: 'c1',
              tool: 'list_dir',
              running: false,
              subject: { kind: 'folder', path: { text: '/shots', linkable: true } },
              outcome: { kind: 'answered' },
              repeatOf: null,
            },
          ],
        },
        {
          messageId: 4,
          text: '',
          calls: [
            {
              callId: 'c1',
              tool: 'app_state',
              running: false,
              subject: { kind: 'panes' },
              outcome: { kind: 'answered' },
              repeatOf: null,
            },
          ],
        },
        { messageId: 6, text: 'Your screenshots.', calls: [] },
      ],
      thinking: false,
      stalled: false,
      streaming: false,
      stats: STATS,
    })
  })

  /** A call with no result row (a stopped or crashed turn) is settled, not running forever. */
  it('settles a result-less call with no outcome', () => {
    const items = buildRailMessages(
      detail([
        view(1, 'user', [text('look')], { turnId: 1 }),
        view(2, 'assistant', [toolCall('c1', { outcome: null })], { turnId: 1 }),
      ]),
    )
    expect(assistantAt(items, 1).rounds[0].calls[0]).toMatchObject({ running: false, outcome: null })
  })

  /** The backend says since which row a turn has been running. Its result-less calls are still
   *  running, its bubble still streams (so the live events that follow write into it), and a
   *  result-less call of an earlier turn stays settled: that turn crashed, it will never answer. */
  it('folds the turn still running as running, and only that one', () => {
    const decision = { verb: 'trash' as const, what: '/a', ops: 1, outcome: { kind: 'rejected' as const } }
    const items = buildRailMessages({
      ...detail([
        view(1, 'user', [text('one')], { turnId: 1 }),
        view(2, 'assistant', [toolCall('c1', { outcome: null })], { turnId: 1 }),
        view(3, 'user', [text('two')], { turnId: 3 }),
        view(4, 'assistant', [toolCall('c1', { outcome: null }), toolCall('c2')], { turnId: 3 }),
        // Recorded after the running turn's newest row: still during that turn.
        view(5, 'event', [{ type: 'proposalDecisions', decisions: [decision] }], { turnId: 3 }),
      ]),
      turnInFlightSinceSeq: 2,
      turnInFlightRun: 1,
      spendPausedMicros: null,
    })

    expect(items.map((item) => item.kind)).toEqual(['user', 'assistant', 'user', 'proposalDecisions', 'assistant'])
    expect(assistantAt(items, 1)).toMatchObject({ streaming: false, rounds: [{ calls: [{ running: false }] }] })
    expect(assistantAt(items, 4)).toMatchObject({
      streaming: true,
      rounds: [
        {
          messageId: 4,
          calls: [
            { callId: 'c1', running: true },
            { callId: 'c2', running: false },
          ],
        },
      ],
    })
  })

  /** A reload before the running turn's first row landed: every bubble on screen is an ended
   *  turn's, so none is left open for the live events to write into. */
  it('leaves every bubble closed when the running turn has no row yet', () => {
    const items = buildRailMessages({
      ...detail([view(1, 'user', [text('one')], { turnId: 1 }), view(2, 'assistant', [text('First.')], { turnId: 1 })]),
      turnInFlightSinceSeq: 2,
      turnInFlightRun: 1,
      spendPausedMicros: null,
    })
    expect(assistantAt(items, 1).streaming).toBe(false)
  })

  /** A decision the user gave while the turn ran sits above that turn's bubble, where the live
   *  rail put it; one given after the turn stays after it. */
  it('hoists a mid-turn event above its bubble and leaves a post-turn one in place', () => {
    const decision = { verb: 'trash' as const, what: '/a', ops: 1, outcome: { kind: 'rejected' as const } }
    const items = buildRailMessages(
      detail([
        view(1, 'user', [text('clean up')], { turnId: 1 }),
        view(2, 'assistant', [toolCall('c1')], { turnId: 1 }),
        view(3, 'event', [{ type: 'proposalDecisions', decisions: [decision] }], { turnId: 1, duringTurn: true }),
        view(4, 'tool', [], { turnId: 1 }),
        view(5, 'assistant', [text('Done.')], { turnId: 1 }),
        view(6, 'event', [{ type: 'proposalDecisions', decisions: [decision] }], { turnId: 1 }),
      ]),
    )
    expect(
      items.map((item) => (item.kind === 'proposalDecisions' ? `decision ${String(item.id)}` : item.kind)),
    ).toEqual(['user', 'decision 3', 'assistant', 'decision 6'])
    expect(assistantAt(items, 2).rounds.map((round) => round.messageId)).toEqual([2, 5])
  })

  /** A model change is recorded between turns, before the next opener: it stays there. */
  it('keeps a between-turns line before the turn it changed, and each turn its own bubble', () => {
    const items = buildRailMessages(
      detail([
        view(1, 'user', [text('one')], { turnId: 1 }),
        view(2, 'assistant', [text('First.')], { turnId: 1 }),
        view(3, 'event', [{ type: 'modelChanged', model: 'model-two' }], { turnId: 1 }),
        view(4, 'user', [text('two')], { turnId: 4 }),
        view(5, 'assistant', [text('Second.')], { turnId: 4 }),
      ]),
    )
    expect(items.map((item) => item.kind)).toEqual(['user', 'assistant', 'modelChange', 'user', 'assistant'])
    expect(assistantAt(items, 4)).toMatchObject({ id: 5, turnId: 4 })
  })

  /** A wake's thread opens with a digest, which is its turn's opener like a user bubble. */
  it("folds a wake's turn under its digest", () => {
    const items = buildRailMessages(
      detail([
        view(7, 'user', [{ type: 'wakeDigest', folders: [], rollups: [] }], { turnId: 7 }),
        view(8, 'assistant', [toolCall('c1')], { turnId: 7 }),
        view(9, 'tool', [], { turnId: 7 }),
        view(10, 'assistant', [text('Nothing new.')], { turnId: 7 }),
      ]),
    )
    expect(items.map((item) => item.kind)).toEqual(['wakeDigest', 'assistant'])
    expect(assistantAt(items, 1)).toMatchObject({ id: 10, turnId: 7 })
  })

  /** Rows before a thread's first opener have no turn; they still fold, and an opener after them
   *  starts a bubble of its own. */
  it('folds rows with no turn apart from the turn after them', () => {
    const items = buildRailMessages(
      detail([
        view(1, 'assistant', [text('orphan one')]),
        view(2, 'assistant', [text('orphan two')]),
        view(3, 'user', [text('hi')], { turnId: 3 }),
        view(4, 'assistant', [text('hello')], { turnId: 3 }),
      ]),
    )
    expect(items.map((item) => item.kind)).toEqual(['assistant', 'user', 'assistant'])
    expect(assistantAt(items, 0).rounds.map((round) => round.text)).toEqual(['orphan one', 'orphan two'])
  })

  /** A model change persists as an `event` row, and renders as its own timeline line. */
  it('renders a persisted model change as a timeline line', () => {
    const items = buildRailMessages(detail([view(1, 'event', [{ type: 'modelChanged', model: 'claude-opus-5' }])]))
    expect(items).toEqual([{ kind: 'modelChange', model: 'claude-opus-5' }])
  })

  /** A chat-memory change persists the same way, so reopening a thread still explains why
   *  the replies after that point saw a different amount of the conversation. */
  it('renders a persisted chat-memory change as a timeline line', () => {
    const items = buildRailMessages(
      detail([view(1, 'event', [{ type: 'chatMemoryChanged', chatMemoryTokens: 16_000 }])]),
    )
    expect(items).toEqual([{ kind: 'chatMemoryChange', chatMemoryTokens: 16_000 }])
  })

  /** A wake opens its thread with a structured digest sitting in the user-role row. It has
   *  to fold into its own item, not into a text bubble: the digest carries no text at all,
   *  so treating it as one would render an empty bubble where the whole reason for the
   *  thread should be. */
  it('renders a wake digest as its own item rather than an empty user bubble', () => {
    const items = buildRailMessages(
      detail([
        view(7, 'user', [
          {
            type: 'wakeDigest',
            folders: [{ folder: '/Users/dana/Downloads', created: 4, modified: 0, removed: 0, renamed: 0 }],
            rollups: [{ ancestor: '/Users/dana/Projects', folders: 7, changes: 40 }],
          },
        ]),
      ]),
    )

    expect(items).toEqual([
      {
        kind: 'wakeDigest',
        id: 7,
        folders: [{ folder: '/Users/dana/Downloads', created: 4, modified: 0, removed: 0, renamed: 0 }],
        rollups: [{ ancestor: '/Users/dana/Projects', folders: 7, changes: 40 }],
      },
    ])
  })

  /** What the user answered arrives on two different rows, and both have to fold into the one
   *  item the rail renders: an `event` row the moment they answered, and the user-role opener
   *  of the follow-up turn a rejected sweep earns. The second carries no text either, so
   *  missing it would render an empty bubble where the reason for the turn should be. */
  it('renders a decision the same way whether it arrives as an event or as a follow-up opener', () => {
    const rejected = {
      verb: 'trash' as const,
      what: '/Users/dana/Downloads/*.dmg',
      ops: 12,
      outcome: { kind: 'rejected' as const },
    }
    const items = buildRailMessages(
      detail([
        view(3, 'event', [{ type: 'proposalDecisions', decisions: [rejected] }]),
        view(4, 'user', [{ type: 'proposalDecisions', decisions: [rejected] }]),
      ]),
    )

    expect(items).toEqual([
      { kind: 'proposalDecisions', id: 3, decisions: [rejected] },
      { kind: 'proposalDecisions', id: 4, decisions: [rejected] },
    ])
  })
})
