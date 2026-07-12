// Ask Cmdr: the read-only chat rail's IPC surface.
//
// Three thin wrappers over the typed `commands.*` bindings (list/get/cancel), plus the
// streaming `sendAskCmdrMessage`. Send rides a Tauri `Channel<T>` (not specta-friendly
// yet), so it uses raw `invoke` with the documented opt-out, exactly like
// `streamFolderSuggestions`. The wire event type is hand-mirrored from the Rust
// `AskCmdrStreamEvent` (a `Channel`-only enum, absent from the generated bindings).

import { Channel, invoke } from '@tauri-apps/api/core'
import {
  commands,
  type ConversationRow,
  type ConversationDetailView,
  type MessageView,
  type MessageBlock,
} from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

export type { ConversationRow, ConversationDetailView, MessageView, MessageBlock }

/** Why an assistant turn ended, on the wire (mirrors Rust `StopReasonView`). */
export type StopReason = 'completed' | 'toolCall' | 'maxTokens' | 'contentFilter' | 'stopSequence' | 'other'

/** Per-turn token usage (mirrors Rust `UsageView`). */
export interface AskCmdrUsage {
  promptTokens: number
  completionTokens: number
}

/** The typed reasons a turn ends without an answer (mirrors Rust `AgentErrorKindView`). */
export type AskCmdrErrorKind =
  | 'noKey'
  | 'notConfigured'
  | 'unavailable'
  | 'timeout'
  | 'authFailed'
  | 'rateLimited'
  | 'budgetExhausted'
  | 'unfinishedReply'
  | 'provider'

/**
 * A streamed progress event for the rail. Hand-mirrors the Rust `AskCmdrStreamEvent`
 * (a `Channel`-only enum). Never carries a reasoning blob or provider state.
 */
export type AskCmdrStreamEvent =
  | { type: 'started'; conversationId: number }
  | { type: 'queued' }
  | { type: 'userPersisted'; messageId: number; seq: number }
  | { type: 'assistantStarted' }
  | { type: 'textDelta'; text: string }
  | { type: 'reasoningTick' }
  | { type: 'toolCallStarted'; callId: string; tool: string }
  | { type: 'toolCallFinished'; callId: string; ok: boolean }
  | { type: 'done'; messageId: number; seq: number; stop: StopReason; usage: AskCmdrUsage }
  | { type: 'failed'; kind: AskCmdrErrorKind }

/**
 * Send one message and stream the answer. `conversationId` is `null` to start a fresh
 * thread; the resolved id arrives both in the first `started` event and as the promise's
 * value. All progress rides `onEvent`. Cancel via [`cancelAskCmdr`] once the id is known
 * (the `started` event) — Tauri's `Channel::send` is fire-and-forget, so abandonment isn't
 * detectable without the explicit cancel command.
 */
export function sendAskCmdrMessage(
  conversationId: number | null,
  text: string,
  onEvent: (event: AskCmdrStreamEvent) => void,
): Promise<number> {
  const channel = new Channel<AskCmdrStreamEvent>()
  channel.onmessage = onEvent
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- streaming Channel<T> not specta-friendly yet; tracked for follow-up
  return invoke<number>('ask_cmdr_send_message', { conversationId, text, onEvent: channel }).then(
    (id) => id,
    () => conversationId ?? 0, // contracted Ok(i64); webview teardown can reject — fall back to the known id
  )
}

/** Stop the in-flight turn for a thread. Idempotent; safe after natural completion. */
export async function cancelAskCmdr(conversationId: number): Promise<void> {
  await commands.askCmdrCancel(conversationId)
}

/** One conversation's header plus a page of its display messages (oldest first). */
export async function getAskCmdrConversation(
  id: number,
  msgLimit: number,
  msgOffset: number,
): Promise<ConversationDetailView | null> {
  const res = await commands.askCmdrGetConversation(id, msgLimit, msgOffset)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Conversations newest-activity first, paged. Empty when the store never opened. */
export async function listAskCmdrConversations(
  limit: number,
  offset: number,
  includeArchived: boolean,
): Promise<ConversationRow[]> {
  const res = await commands.askCmdrListConversations(limit, offset, includeArchived)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}
