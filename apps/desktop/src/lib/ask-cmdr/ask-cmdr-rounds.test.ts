/**
 * The pure readings of a turn's rounds: how they display, how a turn split across history pages
 * folds back into one bubble, how a terminal event settles them, and what the soft cap counts.
 */

import { describe, expect, it } from 'vitest'
import type { AssistantItem, RailMessage, RailToolCall, Round } from './ask-cmdr-messages'
import { contentSignature, displaySegments, mergeAtSeam, settleRounds } from './ask-cmdr-rounds'

function call(callId: string, overrides: Partial<RailToolCall> = {}): RailToolCall {
  return {
    callId,
    tool: 'list_dir',
    running: false,
    subject: null,
    outcome: { kind: 'answered' },
    repeatOf: null,
    ...overrides,
  }
}

function round(messageId: number | null, text: string, calls: RailToolCall[] = []): Round {
  return { messageId, text, calls }
}

function bubble(overrides: Partial<AssistantItem> = {}): AssistantItem {
  return {
    kind: 'assistant',
    id: null,
    turnId: 1,
    rounds: [],
    thinking: false,
    stalled: false,
    streaming: false,
    stats: null,
    ...overrides,
  }
}

describe('displaySegments', () => {
  it("puts each round's text before its calls", () => {
    const segments = displaySegments([round(2, 'Checking.', [call('c1')]), round(4, 'Done.')])
    expect(segments.map((s) => s.kind)).toEqual(['text', 'tools', 'text'])
  })

  /** Consecutive calls read as one group, so a round that said nothing doesn't split them. */
  it('merges the tool groups of rounds with no text between them', () => {
    const segments = displaySegments([
      round(2, 'Looking.', [call('c1')]),
      round(4, '', [call('c1'), call('c2')]),
      round(6, 'One more.', [call('c1')]),
    ])
    expect(segments.map((s) => (s.kind === 'tools' ? s.lines.length : s.kind))).toEqual(['text', 3, 'text', 1])
  })

  /** A line is `(messageId, callId)`: rounds reuse call ids, so the key must carry the round. */
  it('keys lines by their round and call, never the call id alone', () => {
    const segments = displaySegments([round(2, '', [call('fake-call-0')]), round(4, '', [call('fake-call-0')])])
    const tools = segments[0]
    if (tools.kind !== 'tools') throw new Error('expected a tools segment')
    expect(tools.lines.map((line) => line.key)).toEqual(['2:fake-call-0', '4:fake-call-0'])
  })

  it('keys an unresolved round by its index, and a repeated id within a round by occurrence', () => {
    const segments = displaySegments([round(null, '', [call('c1'), call('c1')])])
    const tools = segments[0]
    if (tools.kind !== 'tools') throw new Error('expected a tools segment')
    expect(tools.lines.map((line) => line.key)).toEqual(['r0:c1', 'r0:c1#1'])
    expect(tools.lines.map((line) => line.occurrence)).toEqual([0, 1])
  })

  it('marks only the last round as live prose', () => {
    const segments = displaySegments([round(2, 'First.', [call('c1')]), round(4, 'Second.')])
    const lastRound = segments.flatMap((s) => (s.kind === 'text' ? [s.lastRound] : []))
    expect(lastRound).toEqual([false, true])
  })
})

describe('settleRounds', () => {
  /** What a terminal event leaves is what the stored rows hold: a round that never reached
   *  `End` was never written, and a call with no result row says so with no outcome. */
  it('drops unresolved and empty rounds and settles running calls with no outcome', () => {
    const settled = settleRounds([
      round(2, 'Looking.', [call('c1', { running: true, outcome: null })]),
      round(3, ''),
      round(null, 'half an answer', [call('c2', { running: true, outcome: null })]),
    ])
    expect(settled).toEqual([round(2, 'Looking.', [call('c1', { running: false, outcome: null })])])
  })
})

describe('mergeAtSeam', () => {
  it("joins a turn the page boundary split, keeping the newer half's id and the stats that exist", () => {
    const stats = { toolCalls: 2, wallMs: 900, promptTokens: 10, completionTokens: 4 }
    const older: RailMessage[] = [
      { kind: 'user', id: 1, text: 'hi', attachments: [] },
      bubble({ id: 2, rounds: [round(2, 'Looking.', [call('c1')])] }),
    ]
    const newer: RailMessage[] = [
      { kind: 'modelChange', model: 'hoisted' },
      bubble({ id: 6, rounds: [round(4, '', [call('c1')]), round(6, 'Done.')], stats }),
      { kind: 'user', id: 7, text: 'next', attachments: [] },
    ]
    expect(mergeAtSeam(older, newer)).toEqual([
      { kind: 'user', id: 1, text: 'hi', attachments: [] },
      { kind: 'modelChange', model: 'hoisted' },
      bubble({
        id: 6,
        rounds: [round(2, 'Looking.', [call('c1')]), round(4, '', [call('c1')]), round(6, 'Done.')],
        stats,
      }),
      { kind: 'user', id: 7, text: 'next', attachments: [] },
    ])
  })

  it('joins the two halves of one round a page split', () => {
    const older: RailMessage[] = [bubble({ id: 2, rounds: [round(2, 'Look', [call('c1')])] })]
    const newer: RailMessage[] = [bubble({ id: 2, rounds: [round(2, 'ing.', [call('c2')])] })]
    expect(mergeAtSeam(older, newer)).toEqual([
      bubble({ id: 2, rounds: [round(2, 'Looking.', [call('c1'), call('c2')])] }),
    ])
  })

  it('leaves two different turns apart', () => {
    const older: RailMessage[] = [bubble({ id: 2, turnId: 1, rounds: [round(2, 'One.')] })]
    const newer: RailMessage[] = [bubble({ id: 4, turnId: 3, rounds: [round(4, 'Two.')] })]
    expect(mergeAtSeam(older, newer)).toEqual([...older, ...newer])
  })
})

describe('contentSignature', () => {
  it('changes when the last bubble grows in any way the rail should follow', () => {
    const base = bubble({ rounds: [round(null, 'a')] })
    const signatures = [
      contentSignature([base]),
      contentSignature([bubble({ rounds: [round(null, 'ab')] })]),
      contentSignature([bubble({ rounds: [round(null, 'a', [call('c1')])] })]),
      contentSignature([bubble({ rounds: [round(2, 'a'), round(null, '')] })]),
      contentSignature([
        bubble({
          rounds: [round(null, 'a')],
          stats: { toolCalls: 1, wallMs: 1, promptTokens: 1, completionTokens: 1 },
        }),
      ]),
      contentSignature([base, { kind: 'modelChange', model: 'm' }]),
    ]
    expect(new Set(signatures).size).toBe(signatures.length)
  })
})
