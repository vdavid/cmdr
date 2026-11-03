/**
 * Pure readings of a turn's rounds, shared by the live reducer, the history fold, and the bubble
 * that renders them. Nothing here touches `askCmdrState`.
 */

import type { AssistantItem, RailMessage, RailToolCall, Round } from './ask-cmdr-messages'

/** One tool line as the bubble renders it, with the key its `each` block uses. */
export interface DisplayLine {
  key: string
  /** The line's round's row, `null` while the round still streams: with `call.callId`, the
   * line's identity. */
  messageId: number | null
  /** Which of its round's calls with this id it is, 0 for the first: a provider can name one id
   * twice in a round, and each line fetches its own result. */
  occurrence: number
  call: RailToolCall
}

/** What a bubble shows, in order: a round's narration, or a run of tool lines. */
export type DisplaySegment =
  | {
      kind: 'text'
      key: string
      text: string
      /** The turn's last round, the one whose prose is still being written. */
      lastRound: boolean
    }
  | { kind: 'tools'; key: string; lines: DisplayLine[] }

/**
 * A bubble's rounds as segments: each round's text, then its calls. Two runs of calls with no text
 * between them are one segment, so a round that said nothing doesn't split a group.
 */
export function displaySegments(rounds: Round[]): DisplaySegment[] {
  const segments: DisplaySegment[] = []
  rounds.forEach((round, roundIndex) => {
    if (round.text.length > 0) {
      segments.push({
        kind: 'text',
        key: `text:${String(roundIndex)}`,
        text: round.text,
        lastRound: roundIndex === rounds.length - 1,
      })
    }
    if (round.calls.length === 0) return
    const lines = lineKeys(round, roundIndex)
    const last = segments.at(-1)
    if (last?.kind === 'tools') last.lines.push(...lines)
    else segments.push({ kind: 'tools', key: `tools:${lines[0].key}`, lines })
  })
  return segments
}

/** A line's key is its `(messageId, callId)`, or the round's index while it has no id. A provider
 * that repeats an id within one round gets an occurrence suffix, since a duplicate key crashes the
 * window (Svelte's `each_key_duplicate`, ERR-FRMAR). */
function lineKeys(round: Round, roundIndex: number): DisplayLine[] {
  const roundKey = round.messageId === null ? `r${String(roundIndex)}` : String(round.messageId)
  const seen = new Map<string, number>()
  return round.calls.map((call) => {
    const occurrence = seen.get(call.callId) ?? 0
    seen.set(call.callId, occurrence + 1)
    const suffix = occurrence === 0 ? '' : `#${String(occurrence)}`
    return { key: `${roundKey}:${call.callId}${suffix}`, messageId: round.messageId, occurrence, call }
  })
}

/**
 * What a run of tool lines' open state is remembered by: its first line's key, once that line's
 * round has a row. Stable for the rest of the turn, since calls only append. `null` while the
 * first round still streams: such a group can't be toggled yet, so it needs no key.
 */
export function groupKey(lines: DisplayLine[]): string | null {
  const first = lines.at(0)
  return first === undefined || first.messageId === null ? null : first.key
}

/**
 * A turn's rounds as the stored rows hold them once it ended, which is what a reload would fold:
 * a round that never reached `End` was never written, a call still running has no result row (it
 * settles with no outcome), and a round with nothing in it shows nothing.
 */
export function settleRounds(rounds: Round[]): Round[] {
  return rounds
    .filter((round) => round.messageId !== null)
    .map((round) => ({
      ...round,
      calls: round.calls.map((call) => (call.running ? { ...call, running: false, outcome: null } : call)),
    }))
    .filter((round) => round.text.length > 0 || round.calls.length > 0)
}

/**
 * Prepend an older history page to what's shown, folding back a turn the page boundary cut.
 *
 * The older page's last bubble and the newer list's first bubble are one turn exactly when their
 * `turnId`s match. Anything the newer list holds before that bubble is an event the fold hoisted
 * there, and stays before the joined bubble. The joined bubble takes the newer half's id, flags,
 * and stats (the stats ride a turn's last round, so they are on the newer half when anywhere).
 */
export function mergeAtSeam(older: RailMessage[], newer: RailMessage[]): RailMessage[] {
  const tail = older.at(-1)
  const at = newer.findIndex((item) => item.kind === 'assistant')
  const head = at === -1 ? undefined : newer[at]
  if (tail?.kind !== 'assistant' || head?.kind !== 'assistant' || tail.turnId === null || tail.turnId !== head.turnId) {
    return [...older, ...newer]
  }
  const merged: AssistantItem = {
    ...head,
    rounds: joinRounds(tail.rounds, head.rounds),
    stats: head.stats ?? tail.stats,
  }
  return [...older.slice(0, -1), ...newer.slice(0, at), merged, ...newer.slice(at + 1)]
}

/** Two halves of a turn's rounds, joining the round both halves hold, if any. */
function joinRounds(older: Round[], newer: Round[]): Round[] {
  const last = older.at(-1)
  const first = newer.at(0)
  if (last === undefined || first === undefined || last.messageId === null || last.messageId !== first.messageId) {
    return [...older, ...newer]
  }
  const joined: Round = {
    messageId: last.messageId,
    text: last.text + first.text,
    calls: [...last.calls, ...first.calls],
  }
  return [...older.slice(0, -1), joined, ...newer.slice(1)]
}

/** Everything about the thread's tail the rail's auto-scroll follows: a new item, and the last
 * bubble growing a round, a line, prose, or its stats. */
export function contentSignature(messages: RailMessage[]): string {
  const last = messages.at(-1)
  if (last?.kind !== 'assistant') return String(messages.length)
  const calls = last.rounds.reduce((sum, round) => sum + round.calls.length, 0)
  const textLength = last.rounds.at(-1)?.text.length ?? 0
  return [messages.length, last.rounds.length, calls, textLength, last.stats ? 1 : 0].join(':')
}
