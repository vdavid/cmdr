/**
 * The line a suggestion's answer leaves in its thread (`proposalDecided`). Not part of any turn,
 * so it takes none of the turn rules in `ask-cmdr-stream.svelte.ts`: it never waits for a fresh
 * chat's id, and a stopped thread still shows it. See `DETAILS.md` § How an open thread stays
 * current.
 */

import type { RailMessage } from './ask-cmdr-messages'
import { askCmdrState, insertAboveBubble } from './ask-cmdr-state.svelte'
import type { ProposalDecision } from '#lib/tauri-commands/index.js'

/** One answer the user gave to a suggestion, and the thread whose timeline it was written to. */
export interface HeardDecision {
  conversationId: number
  /** The decision's timeline row, which is what makes a line heard twice one line. */
  messageId: number
  decision: ProposalDecision
}

/**
 * Decisions heard while a thread load was in flight, for {@link settleDecisionsHeardWhileLoading}
 * to put back. A load replaces the whole message list with what the backend read, and that read
 * can be a moment older than a decision the rail has already heard.
 */
let heardWhileLoading: HeardDecision[] = []

/**
 * The user answered a suggestion: show the line in the thread that made it, and only there.
 *
 * The event carries the decision itself, so the rail adds ONE line instead of re-reading the
 * thread. That is not only cheaper: a re-read replaces the message list, which would throw
 * away an answer that is mid-stream, and its reply could arrive after the user moved to
 * another thread. Nothing here waits on anything, so neither can happen.
 */
export function noteProposalDecided(heard: HeardDecision): void {
  if (askCmdrState.loadingHistory) heardWhileLoading.push(heard)
  if (heard.conversationId === askCmdrState.conversationId) showDecision(heard)
}

/**
 * Put back what a thread load may have missed. The trigger calls this when a load ends, however
 * it ended: whatever was heard for the thread now on screen is shown (once), and the rest was
 * about threads the rail isn't showing.
 */
export function settleDecisionsHeardWhileLoading(): void {
  const heard = heardWhileLoading
  heardWhileLoading = []
  for (const decision of heard) {
    if (decision.conversationId === askCmdrState.conversationId) showDecision(decision)
  }
}

/** Add a decision's line, or patch it where it stands when the thread already shows that row.
 * An approval's row is announced twice: "running" at the start, then rewritten with its outcome
 * when it settles. The two come off different threads, and a load can deliver the settled row
 * first, so a late "running" never replaces an outcome. A new line goes above a bubble that is
 * still streaming, which has to stay last for the next chunk to find. */
function showDecision({ messageId, decision }: HeardDecision): void {
  const line: RailMessage = { kind: 'proposalDecisions', id: messageId, decisions: [decision] }
  const at = askCmdrState.messages.findIndex((m) => m.kind === 'proposalDecisions' && m.id === messageId)
  if (at !== -1) {
    if (decision.outcome.kind !== 'running') askCmdrState.messages[at] = line
    return
  }
  insertAboveBubble(line)
}
