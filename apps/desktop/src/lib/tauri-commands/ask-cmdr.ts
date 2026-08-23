// Ask Cmdr: the read-only chat rail's IPC surface.
//
// Thin wrappers over the typed `commands.*` bindings, plus `onAskCmdrTurn`, the subscription
// every turn's progress arrives on. Nothing here is hand-mirrored and nothing rides raw
// `invoke`: the stream is a specta event keyed by conversation, so a reload re-subscribes and
// drift between the Rust enum and this file is a compile error rather than a silent bug.

import type { UnlistenFn } from '@tauri-apps/api/event'
import {
  commands,
  events,
  type AgentErrorKindView,
  type AgentWakeStatus,
  type WakeReadinessView,
  type AskCmdrStreamEvent,
  type AskCmdrTurn,
  type ConversationRow,
  type ConversationDetailView,
  type ConversationSearchHit,
  type EvidenceCoverage,
  type EvidenceSource,
  type MessageView,
  type MessageBlock,
  type AttachmentRef,
  type AttachmentKindView,
  type AskCmdrConsentStatus,
  type ConversationCost,
  type CostSummary,
  type ModelWindowView,
  type RenameEvidence,
  type RenameProposalRowSnapshot,
  type StopReasonView,
  type UsageView,
} from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

export type {
  WakeReadinessView,
  AskCmdrStreamEvent,
  AskCmdrTurn,
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
  ModelWindowView,
  RenameEvidence,
}

/** Why an assistant turn ended, on the wire (the generated `StopReasonView`). */
export type StopReason = StopReasonView

/** Per-turn token usage (the generated `UsageView`). */
export type AskCmdrUsage = UsageView

/** The typed reasons a turn ends without an answer (the generated `AgentErrorKindView`). */
export type AskCmdrErrorKind = AgentErrorKindView

/**
 * Where a proposed rename name came from (the generated `EvidenceSource`, under the name the
 * rail uses). The two image sources are the only ones claiming the file's contents were read,
 * and the backend refuses a plan that claims them without `image_facts` having delivered that
 * content; `userEdited` means the user typed the name themselves, and only the backend's revise
 * path can set it.
 */
export type RenameEvidenceSource = EvidenceSource

/**
 * How much of the text Cmdr read in an image the quote behind a name actually covers (the
 * generated `EvidenceCoverage`). Present on `imageText` rows only.
 *
 * Backend-derived from the delivery the evidence check matched against, never model-authored
 * and never an input, so the review row can show that a 7-character hit inside 3,140
 * characters is thin. Every count is in characters of the delivered text. The three text
 * fields are OCR output: render them as plain text, never `{@html}`.
 */
export type RenameEvidenceCoverage = EvidenceCoverage

/** One review row as the backend owns it (the generated `RenameProposalRowSnapshot`). */
export type RenameProposalRow = RenameProposalRowSnapshot

/** What a send answered: the thread it landed in, or why it never started. */
export type AskCmdrSendOutcome =
  | { accepted: true; conversationId: number }
  /** Decided before the turn existed (no consent, no slot, a window too small), so it can't
   * arrive as a stream event — some of these happen before there IS a thread to key one on. */
  | { accepted: false; kind: AskCmdrErrorKind; detail: string | null }

/**
 * Send one message. `conversationId` is `null` to start a fresh thread; the resolved id comes
 * back here, and the answer streams over [`onAskCmdrTurn`] keyed on it. Cancel via
 * [`cancelAskCmdr`] once the id is known.
 *
 * `deniedNames` are destination names the user turned down in this thread's last rename review.
 * They ride the turn envelope so the next batch doesn't re-propose a style the user rejected.
 * Names only, never a reason: a model-authored "why" would come back as a rationalization.
 */
export async function sendAskCmdrMessage(
  conversationId: number | null,
  text: string,
  attachments: AttachmentRef[],
  deniedNames: string[],
): Promise<AskCmdrSendOutcome> {
  const res = await commands.askCmdrSendMessage(conversationId, text, attachments, deniedNames)
  if (res.status === 'error') return { accepted: false, kind: res.error.kind, detail: res.error.detail }
  return { accepted: true, conversationId: res.data }
}

/**
 * Every Ask Cmdr turn's progress, whoever started it: a rail send or a wake the user never
 * asked for. Each event names the thread it belongs to, so a subscriber filters rather than
 * owning a stream — which is what lets a reloaded webview, or a rail opened onto a wake
 * already in flight, pick the turn up mid-answer.
 *
 * ⚠️ It reaches EVERY window. Subscribe from the main window only.
 */
export function onAskCmdrTurn(callback: (payload: AskCmdrTurn) => void): Promise<UnlistenFn> {
  return events.askCmdrTurn.listen((event) => {
    callback(event.payload)
  })
}

/** Stop the in-flight turn for a thread. Idempotent; safe after natural completion. */
export async function cancelAskCmdr(conversationId: number): Promise<void> {
  await commands.askCmdrCancel(conversationId)
}

/** Rechecks the user-selected rows of a server-owned rename proposal. Only opaque ids
 * cross the IPC boundary: source paths and destination names stay in the backend. */
export async function preflightBulkRename(proposalId: string, allowedRowIds: string[]) {
  const res = await commands.preflightBulkRename(proposalId, allowedRowIds)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Replace one row's proposed name with the one the user typed. The backend validates the name,
 * swaps the row's evidence for the "you typed this" marker, and invalidates the accepted
 * preflight, so the edited name is rechecked before it can reach the filesystem. Answers the row
 * as the dialog should now show it. */
export async function reviseBulkRenameRow(proposalId: string, rowId: string, destinationName: string) {
  const res = await commands.reviseBulkRenameRow(proposalId, rowId, destinationName)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Starts the one queued operation for the exact rows the user allowed after preflight. */
export async function applyBulkRename(proposalId: string, allowedRowIds: string[]) {
  const res = await commands.applyBulkRename(proposalId, allowedRowIds)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Discard a staged rename proposal after the user closes its review. */
export async function cancelBulkRenameProposal(proposalId: string): Promise<void> {
  await commands.cancelBulkRenameProposal(proposalId)
}

/** Record that a settings change switched a thread's effective model. Resolves once any
 * in-flight turn finished (the backend queues on the thread's single-flight lock), with
 * the persisted event's display view — or `null` when nothing changed for this thread. */
export async function recordAskCmdrModelChange(conversationId: number): Promise<MessageView | null> {
  const res = await commands.askCmdrRecordModelChange(conversationId)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
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

/** True when the Ask Cmdr send path is served by the deterministic scripted fake LLM
 * (`CMDR_E2E_ASK_CMDR_FAKE`, E2E only). The composer treats the fake as an active
 * provider so Send isn't gated off while `ai.provider` is `off` under E2E. Returns
 * false when not, or when the backend isn't reachable (non-Tauri context, like Vitest). */
export async function askCmdrFakeActive(): Promise<boolean> {
  try {
    return await commands.askCmdrFakeActive()
  } catch {
    return false
  }
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

/**
 * The model Ask Cmdr would send to right now, and the context window Cmdr believes it has.
 * `knownWindowTokens` is `null` when nothing knows the window, in which case no size can
 * honestly be called too large.
 */
export async function askCmdrModelWindow(): Promise<ModelWindowView> {
  return commands.askCmdrModelWindow()
}

/**
 * Tell the proactive loop its settings moved, so it re-reads them and re-arms its timer.
 *
 * ⚠️ The live-apply push behind `askCmdr.proactive`, `askCmdr.wakeDelay`, and
 * `askCmdr.wakeToast`. Unlike the other two `askCmdr.*` settings, which the backend reads
 * fresh on each send, these drive a SLEEPING timer: without this the loop keeps running on
 * whatever it read at launch until the app restarts. No value crosses; the loop re-reads
 * `settings.json` itself.
 */
export async function askCmdrWakeSettingsChanged(): Promise<void> {
  await commands.askCmdrWakeSettingsChanged()
}

/**
 * What the status corner's wake indicator should show right now.
 *
 * ⚠️ A SEED, not a poll: [`onAgentWakeStatus`] announces every move. This covers the two the
 * corner can miss — a wake already running when the window opened, and a gate that closed
 * before it did.
 */
export async function agentWakeStatus(): Promise<AgentWakeStatus> {
  return commands.agentWakeStatus()
}

/**
 * Subscribe to the wake indicator's status: a wake starting or finishing, and a readiness gate
 * moving.
 *
 * ⚠️ It reaches EVERY window, and only the main window has a status corner. Subscribe from
 * there only, exactly as [`onAskCmdrTurn`] is scoped.
 */
export function onAgentWakeStatus(callback: (payload: AgentWakeStatus) => void): Promise<UnlistenFn> {
  return events.agentWakeStatus.listen((event) => {
    callback(event.payload)
  })
}
