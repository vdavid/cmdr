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
import type { RailMessage } from './ask-cmdr-messages'
import type { AttachmentRef, RenameEvidence, RenameEvidenceCoverage } from '$lib/tauri-commands'

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

export interface BulkRenameReview {
  proposalId: string
  rows: BulkRenameReviewRow[]
  preflighting: boolean
  expired: boolean
  requestVersion: number
}

interface AskCmdrState {
  open: boolean
  /** Rail width in px (clamped 280-520), persisted. */
  width: number
  /** The active thread, or `null` for an unsaved new chat. */
  conversationId: number | null
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
  /** How full the model's view got on the thread's last measured turn, for the footer gauge.
   * `null` means no turn has been measured, which the gauge shows as nothing at all. */
  contextUsage: ContextUsage | null
  /** Destination names the user turned down in the last review, newest first, waiting to ride
   * the NEXT send so the following batch doesn't propose the same style again. Cleared once
   * sent: they're feedback on one decision, not a permanent denylist. */
  deniedNames: string[]
}

export const RAIL_MIN_WIDTH = 280
export const RAIL_MAX_WIDTH = 520
export const RAIL_DEFAULT_WIDTH = 340

export const askCmdrState = $state<AskCmdrState>({
  open: false,
  width: RAIL_DEFAULT_WIDTH,
  conversationId: null,
  messages: [],
  streaming: false,
  loadingHistory: false,
  messageTotal: 0,
  historyCount: 0,
  loadingOlder: false,
  attachments: [],
  renameReview: null,
  contextUsage: null,
  deniedNames: [],
})

/** True once the thread grows past the soft cap (drives the "start a fresh one?" nudge). */
export function isOverSoftCap(): boolean {
  return askCmdrState.messages.length > THREAD_SOFT_CAP_MESSAGES
}

/** True when older history pages exist beyond what's loaded (drives "load earlier"). */
export function hasOlderMessages(): boolean {
  return askCmdrState.historyCount < askCmdrState.messageTotal
}

export function currentAssistant(): Extract<RailMessage, { kind: 'assistant' }> | null {
  const last = askCmdrState.messages.at(-1)
  return last?.kind === 'assistant' ? last : null
}

export function lastUserMessage(): Extract<RailMessage, { kind: 'user' }> | null {
  for (let i = askCmdrState.messages.length - 1; i >= 0; i--) {
    const message = askCmdrState.messages[i]
    if (message.kind === 'user') return message
  }
  return null
}

/** Finalize the streaming assistant bubble: retire unfinished activity, stop its cursor,
 * and drop it if it never produced anything. Finished tool history stays visible. */
export function finalizeAssistant(messageId?: number): void {
  const assistant = currentAssistant()
  if (!assistant) return
  assistant.streaming = false
  assistant.thinking = false
  assistant.stalled = false
  assistant.tools = assistant.tools.filter((tool) => !tool.running)
  if (messageId !== undefined) assistant.id = messageId
  if (assistant.text.length === 0 && assistant.tools.length === 0) {
    askCmdrState.messages.pop()
  }
}
