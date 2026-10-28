/**
 * The rail's shared state: the `$state` object every Ask Cmdr module reads and writes, its
 * types, its size limits, and the few accessors that only look at the message list.
 *
 * It lives on its own so the slices that mutate it — `ask-cmdr-stream.svelte.ts` (the live
 * turn), `ask-cmdr-rename-review.svelte.ts` (the review), `ask-cmdr-trigger.svelte.ts` (the
 * rail itself and its threads) — can all depend on it without depending on each other.
 * Everything here is state and shape; no IPC, no side effects.
 *
 * The thread is a flat list of {@link RailMessage}s. History (loaded via
 * `getAskCmdrConversation`) and the live stream both write into the same list; streaming
 * events mutate the last assistant message in place (Svelte 5 deep-proxies the array and
 * its objects, so field mutation is reactive).
 */

import type { ContextUsage } from './ask-cmdr-context-usage'
import type { AssistantItem, RailMessage } from './ask-cmdr-messages'
import { settleRounds, softCapCount } from './ask-cmdr-rounds'
import type { AttachmentRef, RenameEvidence, RenameEvidenceCoverage } from '#lib/tauri-commands/index.js'

/** Past this many thread messages the rail nudges "start a fresh one?" (mirrors the Rust
 * `THREAD_SOFT_CAP_MESSAGES`; no hard cut). */
export const THREAD_SOFT_CAP_MESSAGES = 40

/** How many messages a thread page holds. Threads are small (soft cap ~40), so the first
 * page is usually the whole thread; paging is the insurance for a long one. Loading is
 * tail-first (newest page), with "load earlier" prepending older pages. */
export const MESSAGE_PAGE = 50
export interface BulkRenameReviewRow {
  rowId: string
  sourceName: string
  destinationName: string
  /** The file this row renames, for its thumbnail and the full viewer. Display only: apply
   * sends opaque row ids, and the backend resolves paths from its own stored proposal. */
  sourcePath: string
  volumeId: string
  /** What the backend verified this name is based on. Display only; `detail` is
   * model-authored text, so it renders as plain text and is never branched on. */
  evidence: RenameEvidence
  /** How much of the text Cmdr read in the image the quote covers (`imageText` rows only),
   * so a thin match looks thin. Backend-derived, never model-authored. */
  coverage: RenameEvidenceCoverage | null
  allowed: boolean
  blockedReason: string | null
  warnings: Array<'extensionChanged' | 'cycle'>
  /** True when the last name the user typed here wasn't a usable filename, so the row kept the
   * name it had. Display only; the backend is the authority on what a name may be. */
  nameRejected: boolean
}

/**
 * One staged plan inside a review: the unit the BACKEND knows about. Preflight, apply, and
 * cancel are all per proposal, so each carries its own in-flight and expiry state.
 */
export interface BulkRenameReviewProposal {
  proposalId: string
  rows: BulkRenameReviewRow[]
  preflighting: boolean
  expired: boolean
  /** The plan revision whose rows are on screen. Preflight and apply send it, so a plan the agent
   *  amended since answers `listChanged` instead of binding rows the user never read. */
  revision: number
  /** The agent amended this batch since the review showed it: its rows stay put, announced, and
   *  Apply leaves it out. */
  changed: boolean
  requestVersion: number
}

/**
 * One review, however many batches a job took.
 *
 * The model can only emit about 101 plan rows per reply, so a big rename arrives as a run of
 * staged proposals. They accumulate here and the user answers all of them once; Apply then
 * starts one operation per proposal, which is what the backend has always expected.
 */
export interface BulkRenameReview {
  proposals: BulkRenameReviewProposal[]
}

interface AskCmdrState {
  open: boolean
  /** Rail width in px (clamped 280-520), persisted. */
  width: number
  /** The active thread, or `null` for an unsaved new chat. */
  conversationId: number | null
  /** The user asked for a new chat (New chat, ⌘N) and it's what the rail shows, so loading
   * the last thread must never replace it: not on open, and not when a gate opens or cloud
   * consent finishes loading. A thread load ends it. */
  freshChat: boolean
  messages: RailMessage[]
  streaming: boolean
  loadingHistory: boolean
  /** Total messages the active thread had at load, so paging knows when older exist. */
  messageTotal: number
  /** History rows loaded so far, from the newest end. `< messageTotal` ⇒ older remain. */
  historyCount: number
  /** True while a "load earlier" page is in flight. */
  loadingOlder: boolean
  /** Files/folders staged in the composer for the next send (path + kind only). */
  attachments: AttachmentRef[]
  renameReview: BulkRenameReview | null
  /** How full the model's view got on the thread's last measured call, for the footer gauge.
   * `null` means no call has been measured, and the gauge shows the slot's baseline instead. */
  contextUsage: ContextUsage | null
  /** Bumped on each `costMetered` event (one per completed model call), so the footer re-reads
   * the meter mid-turn. A counter, never a total: the meter stays the one source of the cost. */
  costRevision: number
  /** Destination names the user turned down in the last review, newest first, waiting to ride
   * the NEXT send so a retry doesn't propose the same style again. Cleared once sent: they're
   * feedback on one decision, not a permanent denylist. */
  deniedNames: string[]
  /** Bumped to ask the composer to take focus (a new chat, the sessions panel closing). A
   * counter so the same request twice still lands; the composer also focuses on mount. */
  composerFocusRequest: number
}

export const RAIL_MIN_WIDTH = 280
export const RAIL_MAX_WIDTH = 520
export const RAIL_DEFAULT_WIDTH = 340

export const askCmdrState = $state<AskCmdrState>({
  open: false,
  width: RAIL_DEFAULT_WIDTH,
  conversationId: null,
  freshChat: false,
  messages: [],
  streaming: false,
  loadingHistory: false,
  messageTotal: 0,
  historyCount: 0,
  loadingOlder: false,
  attachments: [],
  renameReview: null,
  contextUsage: null,
  costRevision: 0,
  deniedNames: [],
  composerFocusRequest: 0,
})

/** True once the thread grows past the soft cap (drives the "start a fresh one?" nudge). Each
 * round counts, as each assistant row did when it was its own bubble (`softCapCount`). */
export function isOverSoftCap(): boolean {
  return softCapCount(askCmdrState.messages) > THREAD_SOFT_CAP_MESSAGES
}

/** True when older history pages exist beyond what's loaded (drives "load earlier"). */
export function hasOlderMessages(): boolean {
  return askCmdrState.historyCount < askCmdrState.messageTotal
}

/**
 * The bubble the running turn writes into: the last item, while it streams. An ended turn's
 * bubble is never written into again, so the next turn (a wake, or one a reload caught before
 * its first row landed) opens its own.
 */
export function streamingAssistant(): AssistantItem | null {
  const last = askCmdrState.messages.at(-1)
  return last?.kind === 'assistant' && last.streaming ? last : null
}

/** Add `item` just above the streaming bubble, which has to stay last for the next chunk to
 * find, or at the end when no turn is streaming. */
export function insertAboveBubble(item: RailMessage): void {
  const assistant = streamingAssistant()
  if (assistant) askCmdrState.messages.splice(askCmdrState.messages.indexOf(assistant), 0, item)
  else askCmdrState.messages.push(item)
}

/**
 * Settle a turn's bubble the way a reload would read its stored rows: stop its cursor, keep only
 * the rounds that persisted (`settleRounds`: a round that never reached `End` goes, a call still
 * running settles with no outcome), take the last persisted round's id, and drop the bubble if
 * nothing of it was stored.
 *
 * ⚠️ So a turn that ends without an answer loses the text of the round it was streaming: it was
 * never stored, and a reload wouldn't show it either.
 */
export function settleAssistant(assistant: AssistantItem): void {
  assistant.streaming = false
  assistant.thinking = false
  assistant.stalled = false
  const lastPersisted = assistant.rounds.findLast((round) => round.messageId !== null)
  if (lastPersisted) assistant.id = lastPersisted.messageId
  assistant.rounds = settleRounds(assistant.rounds)
  if (assistant.rounds.length > 0) return
  const at = askCmdrState.messages.indexOf(assistant)
  if (at !== -1) askCmdrState.messages.splice(at, 1)
}
