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
  type AgentWakeStaged,
  type AgentWakeStatus,
  type WakeReadinessView,
  type AskCmdrStreamEvent,
  type AskCmdrTurn,
  type CancelOutcomeView,
  type ConversationRow,
  type ConversationDetailView,
  type ConversationSearchHit,
  type EvidenceCoverage,
  type EvidenceSource,
  type MessageView,
  type MessageBlock,
  type AttachmentRef,
  type AttachmentKindView,
  type LegacyAskCmdrOptIn,
  type ManagedAiRefusal,
  type ConversationCost,
  type CostSummary,
  type ModelWindowView,
  type NextCallPricing,
  type ProviderReadiness,
  type SlotStatus,
  type RenameEvidence,
  type RenameProposalRowSnapshot,
  type StopReasonView,
  type UsageView,
  type WakeDigestFolderView,
  type WakeDigestRollupView,
  type ProposalDecision,
  type AskCmdrToolResultError,
  type ToolResultDetailView,
} from '#lib/ipc/bindings.js'
import { throwIpcError } from './ipc-types'

export type {
  LegacyAskCmdrOptIn,
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
  ConversationCost,
  CostSummary,
  ModelWindowView,
  NextCallPricing,
  ProviderReadiness,
  SlotStatus,
  RenameEvidence,
  /** One folder a wake's digest named outright, with its four counts. Numbers and a path
   * only: every word around them is the rail's, so ten locales can say it their own way. */
  WakeDigestFolderView,
  /** The folders a wake's digest had no room to name, summarized under a shared ancestor. */
  WakeDigestRollupView,
  /** One proposal group the user answered: the verb, the count, and what became of it.
   * Numbers and the group's own display text only, so the rail owns every word around them. */
  ProposalDecision,
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
  /** `run` is the turn's, which every event of it carries (`AskCmdrTurn.run`). */
  | { accepted: true; conversationId: number; run: number }
  /** Decided before the turn existed (Ask Cmdr off, cloud AI not allowed, no slot, a window
   * too small), so it can't arrive as a stream event — some of these happen before there IS a
   * thread to key one on. */
  | { accepted: false; kind: AskCmdrErrorKind; detail: string | null; managed: ManagedAiRefusal | null }

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
  if (res.status === 'error') {
    return { accepted: false, kind: res.error.kind, detail: res.error.detail, managed: res.error.managed }
  }
  return { accepted: true, conversationId: res.data.conversationId, run: res.data.run }
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

/** Stop the in-flight turn for a thread. Idempotent; safe after natural completion. Answers what
 * the Stop reached: how many turns it newly tripped, and how many still owe a terminal. */
export async function cancelAskCmdr(conversationId: number): Promise<CancelOutcomeView> {
  return commands.askCmdrCancel(conversationId)
}

/** Rechecks the user-selected rows of a server-owned rename proposal. Only opaque ids
 * cross the IPC boundary: source paths and destination names stay in the backend. `revision` is the
 * one the review showed (`RenameProposalSnapshot.revision`); an agent amend since answers status
 * `listChanged` and records nothing. */
export async function preflightBulkRename(proposalId: string, revision: number, allowedRowIds: string[]) {
  const res = await commands.preflightBulkRename(proposalId, revision, allowedRowIds)
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

/** Starts the one queued operation for the exact rows the user allowed after preflight, for the
 * revision the review showed. An agent amend since refuses as the typed `listChanged` error. */
export async function applyBulkRename(proposalId: string, revision: number, allowedRowIds: string[]) {
  const res = await commands.applyBulkRename(proposalId, revision, allowedRowIds)
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Discard a staged rename proposal after the user closes its review. */
export async function cancelBulkRenameProposal(proposalId: string): Promise<void> {
  await commands.cancelBulkRenameProposal(proposalId)
}

/** Record that a settings change moved a thread's slot: the model it sends to, the chat
 * memory each message carries, or both. Resolves once any in-flight turn finished (the
 * backend queues on the thread's single-flight lock), with one display view per change in
 * timeline order — empty when nothing changed for this thread. */
export async function recordAskCmdrSlotChange(conversationId: number): Promise<MessageView[]> {
  const res = await commands.askCmdrRecordSlotChange(conversationId)
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

/** What one tool line's call returned: the preview and the raw result for Copy, or why there's
 * nothing to show. */
export type AskCmdrToolResult =
  | { found: true; detail: ToolResultDetailView }
  | { found: false; reason: AskCmdrToolResultError }

/**
 * What the call `(messageId, callId)` of a thread returned, for its expanded line; `occurrence`
 * picks among the calls one row names with that id (0 for the first). The one door a raw tool
 * result crosses by: on the user's expand, never in the stream or a history page.
 * ❌ Never log it: it can hold file text and photo locations.
 */
export async function askCmdrToolResult(
  conversationId: number,
  messageId: number,
  callId: string,
  occurrence: number,
): Promise<AskCmdrToolResult> {
  const res = await commands.askCmdrToolResult(conversationId, messageId, callId, occurrence)
  return res.status === 'ok' ? { found: true, detail: res.data } : { found: false, reason: res.error }
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

/** Resolve dragged LOCAL paths into typed attachment refs (kind from known pane state).
 * Only for local-volume drags; virtual-volume paths mis-resolve and aren't supported. */
export async function resolveAskCmdrAttachments(paths: string[]): Promise<AttachmentRef[]> {
  return commands.askCmdrResolveAttachments(paths)
}

/**
 * What the legacy Ask Cmdr opt-in says, for the one-time `askCmdr.enabled` mapping at startup.
 * `storeUnavailable` means write nothing and ask again next launch.
 */
export async function askCmdrLegacyOptIn(): Promise<LegacyAskCmdrOptIn> {
  return commands.askCmdrLegacyOptIn()
}

/**
 * Tell the proactive loop `askCmdr.enabled` moved, so its cached readiness re-reads the switch.
 * The send path reads the setting fresh; the wake loop doesn't, so without this a switch-off
 * would leave the loop admitting work until the next readiness refresh.
 */
export async function askCmdrEnabledChanged(): Promise<void> {
  await commands.askCmdrEnabledChanged()
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
 * What the rail knows before anything is sent: whether an AI provider is set up, how the next
 * call is priced, and what every call of a new chat costs at minimum (the gauge's baseline).
 * `null` only when the backend's read itself broke; keep what was last known then.
 */
export async function askCmdrSlotStatus(): Promise<SlotStatus | null> {
  return commands.askCmdrSlotStatus()
}

/**
 * Where the agent's notes live, creating the folder if it isn't there yet.
 *
 * ⚠️ The path comes from Rust because it moves with `CMDR_DATA_DIR` (prod, plain dev, and
 * every worktree slug get their own). Building it in the frontend would walk someone into
 * the production folder from a dev build.
 */
export async function askCmdrMemoryFolder(): Promise<string> {
  const res = await commands.askCmdrMemoryFolder()
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
}

/** Delete every note the agent has written about the user, and say how many went. */
export async function askCmdrForgetMemory(): Promise<number> {
  const res = await commands.askCmdrForgetMemory()
  if (res.status === 'error') throwIpcError(res.error)
  return res.data
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

/**
 * Subscribe to "a wake staged proposals": the one thing the proactive agent says out loud.
 *
 * ⚠️ One-shot, and NOT seeded. Unlike the indicator's status there is no "current value" to
 * read at startup: a proposal staged before this window existed is already waiting in the
 * suggestions badge, and re-announcing it on every reload would toast the same thing twice.
 *
 * ⚠️ It reaches EVERY window. Subscribe from the main window only, or the settings window
 * raises its own copy of the toast.
 */
export function onAgentWakeStaged(callback: (payload: AgentWakeStaged) => void): Promise<UnlistenFn> {
  return events.agentWakeStaged.listen((event) => {
    callback(event.payload)
  })
}
