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
  type ConversationSearchHit,
  type MessageView,
  type MessageBlock,
  type AttachmentRef,
  type AttachmentKindView,
  type AskCmdrConsentStatus,
  type ConversationCost,
  type CostSummary,
} from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

export type {
  ConversationRow,
  ConversationDetailView,
  ConversationSearchHit,
  MessageView,
  MessageBlock,
  AttachmentRef,
  AttachmentKindView,
  AskCmdrConsentStatus,
  ConversationCost,
  CostSummary,
}

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
  | 'noConsent'
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
  /** `detail` is the source error's own wording for display under the typed headline
   * (a retired model slug, a quota reset time); never branch on it. */
  | { type: 'failed'; kind: AskCmdrErrorKind; detail: string | null }

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
  attachments: AttachmentRef[],
  onEvent: (event: AskCmdrStreamEvent) => void,
): Promise<number> {
  const channel = new Channel<AskCmdrStreamEvent>()
  channel.onmessage = onEvent
  // eslint-disable-next-line cmdr/no-raw-tauri-invoke -- streaming Channel<T> not specta-friendly yet; tracked for follow-up
  return invoke<number>('ask_cmdr_send_message', { conversationId, text, attachments, onEvent: channel }).then(
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

/** Conversations whose messages match `query` (newest-match first, paged), each with a
 * plain-text snippet. Empty for a blank/punctuation-only query. */
export async function searchAskCmdrConversations(
  query: string,
  limit: number,
  offset: number,
): Promise<ConversationSearchHit[]> {
  const res = await commands.askCmdrSearchConversations(query, limit, offset)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Rename a conversation. */
export async function renameAskCmdrConversation(id: number, title: string): Promise<void> {
  const res = await commands.askCmdrRenameConversation(id, title)
  if (res.status === 'error') throwIpcError(res.error)
}

/** Archive or unarchive a conversation (no delete in v1 — the flag filters the list). */
export async function archiveAskCmdrConversation(id: number, archived: boolean): Promise<void> {
  const res = await commands.askCmdrArchiveConversation(id, archived)
  if (res.status === 'error') throwIpcError(res.error)
}

/** Attachment refs for the focused pane's current selection (or its cursor item when
 * nothing is selected) — the "ask about selection" affordance. Path + kind only. */
export async function askCmdrSelectionAttachments(): Promise<AttachmentRef[]> {
  return commands.askCmdrSelectionAttachments()
}

/** Resolve dragged LOCAL paths into typed attachment refs (kind from known pane state).
 * Only for local-volume drags; virtual-volume paths mis-resolve and aren't supported. */
export async function resolveAskCmdrAttachments(paths: string[]): Promise<AttachmentRef[]> {
  return commands.askCmdrResolveAttachments(paths)
}

/** Whether the user has opted into the CURRENT Ask Cmdr consent copy, plus the audit of
 * what/when they accepted. The rail gates on `accepted`. */
export async function askCmdrConsentStatus(): Promise<AskCmdrConsentStatus> {
  const res = await commands.askCmdrConsentStatus()
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Record the user's opt-in to the current consent copy (timestamp + copy version). */
export async function acceptAskCmdrConsent(): Promise<void> {
  const res = await commands.askCmdrAcceptConsent()
  if (res.status === 'error') throwIpcError(res.error)
}

/** Turn Ask Cmdr off by clearing consent (chats are kept; the next open re-shows consent). */
export async function revokeAskCmdrConsent(): Promise<void> {
  const res = await commands.askCmdrRevokeConsent()
  if (res.status === 'error') throwIpcError(res.error)
}

/** One conversation's cumulative token + cost total (all days, all models). */
export async function askCmdrConversationCost(id: number): Promise<ConversationCost> {
  const res = await commands.askCmdrConversationCost(id)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** The per-day cost rollup across every thread and model, newest day first. */
export async function askCmdrCostSummary(): Promise<CostSummary> {
  const res = await commands.askCmdrCostSummary()
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}
