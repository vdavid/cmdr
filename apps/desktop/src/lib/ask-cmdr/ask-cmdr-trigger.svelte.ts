/**
 * Ask Cmdr rail state: open/close, focus, the active conversation, and the live
 * streaming model. Modeled on `operation-log-trigger.svelte.ts` (a module-level `$state`
 * object mutated by exported functions).
 *
 * The thread is a flat list of {@link RailMessage}s. History (loaded via
 * `getAskCmdrConversation`) and the live stream both write into the same list; streaming
 * events mutate the last assistant message in place (Svelte 5 deep-proxies the array and
 * its objects, so field mutation is reactive).
 *
 * **Cancel finalizes locally.** The runtime returns `Cancelled` with NO terminal event,
 * so a stop won't be echoed back — `stopStreaming` finalizes the bubble itself.
 */

import { saveAppStatus } from '$lib/app-status-store'
import { explorerState } from '$lib/file-explorer/pane/explorer-state.svelte'
import { getAppLogger } from '$lib/logging/logger'
import { consentState, refreshConsent } from './ask-cmdr-consent.svelte'
import { growMainWindowForRail, shrinkMainWindowForRail } from './rail-window'
import {
  applyBulkRename,
  cancelAskCmdr,
  cancelBulkRenameProposal,
  getAskCmdrConversation,
  listAskCmdrConversations,
  recordAskCmdrModelChange,
  preflightBulkRename,
  sendAskCmdrMessage,
  type AskCmdrErrorKind,
  type AskCmdrStreamEvent,
  type AttachmentRef,
  type ConversationDetailView,
  type MessageView,
} from '$lib/tauri-commands'

const log = getAppLogger('askCmdr')

/** Past this many thread messages the rail nudges "start a fresh one?" (mirrors the Rust
 * `THREAD_SOFT_CAP_MESSAGES`; no hard cut). */
export const THREAD_SOFT_CAP_MESSAGES = 40

/** How many messages a thread page holds. Threads are small (soft cap ~40), so the first
 * page is usually the whole thread; paging is the insurance for a long one. Loading is
 * tail-first (newest page), with "load earlier" prepending older pages. */
export const MESSAGE_PAGE = 50
const STALL_AFTER_MS = 30_000
const STOP_AFTER_MS = 90_000
let stallTimer: ReturnType<typeof setTimeout> | null = null
let stopTimer: ReturnType<typeof setTimeout> | null = null

/** One tool call the assistant made, as the collapsible "looked at X" line shows it. */
export interface RailToolCall {
  callId: string
  /** The wire tool name; the localized label is derived in `ask-cmdr-labels.ts`. */
  tool: string
  running: boolean
  ok: boolean
  /** A path pulled from the call arguments, shown as escaped plain text. `null` if none. */
  path: string | null
}

/** One rendered item in the thread. `attachments` on a user turn are the chips shown
 * under the sent message; history rows carry none (the refs rode into the envelope, not
 * stored blocks). */
export type RailMessage =
  | { kind: 'user'; id: number | null; text: string; attachments: AttachmentRef[] }
  | {
      kind: 'assistant'
      id: number | null
      text: string
      tools: RailToolCall[]
      thinking: boolean
      stalled?: boolean
      streaming: boolean
    }
  | {
      kind: 'error'
      errorKind: AskCmdrErrorKind
      /** The provider's own wording, shown as escaped plain text under the friendly
       * headline so the user sees what to fix. Display only; never branched on. */
      detail?: string
    }
  /** A timeline line marking that the thread's effective model changed between turns. */
  | { kind: 'modelChange'; model: string }

export interface BulkRenameReviewRow {
  rowId: string
  sourceName: string
  destinationName: string
  allowed: boolean
  blockedReason: string | null
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
}

export const RAIL_MIN_WIDTH = 280
export const RAIL_MAX_WIDTH = 520
const RAIL_DEFAULT_WIDTH = 340

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
})

/** True once the thread grows past the soft cap (drives the "start a fresh one?" nudge). */
export function isOverSoftCap(): boolean {
  return askCmdrState.messages.length > THREAD_SOFT_CAP_MESSAGES
}

/** True when older history pages exist beyond what's loaded (drives "load earlier"). */
export function hasOlderMessages(): boolean {
  return askCmdrState.historyCount < askCmdrState.messageTotal
}

// ── Open / close / focus ───────────────────────────────────────────────────────

/** Apply persisted rail state at startup (called once from `loadPersistedState`). The window
 * is already at its persisted (rail-inclusive) size, so hydration must NOT grow it again. */
export function hydrateRail(open: boolean, width: number): void {
  askCmdrState.width = clampWidth(width)
  if (open) void openRail({ resizeWindow: false })
}

/** Open the rail, focus its composer, and bootstrap the most recent thread if empty. Grows the
 * main window so the panes keep their size (see `rail-window.ts`), except at startup hydration.
 * Also refreshes the consent gate: the rail shows the consent screen until the user opts in, and
 * only then bootstraps history (no chat exists to load before consent). */
export async function openRail(opts: { resizeWindow?: boolean } = {}): Promise<void> {
  const wasOpen = askCmdrState.open
  askCmdrState.open = true
  explorerState.setRailFocused(true)
  saveAppStatus({ askCmdrRailOpen: true })
  // Only a genuine closed→open transition grows the window; re-opens (e.g. after consenting) and
  // startup hydration must not.
  if (!wasOpen && opts.resizeWindow !== false) void growMainWindowForRail(askCmdrState.width)
  await refreshConsent()
  if (consentState.accepted !== true) return
  if (!wasOpen && askCmdrState.conversationId === null && askCmdrState.messages.length === 0) {
    await bootstrapActiveThread()
  }
}

/** Close the rail, shrink the window back to its pre-rail size, and return focus to the pane. */
export function closeRail(): void {
  askCmdrState.open = false
  explorerState.setRailFocused(false)
  saveAppStatus({ askCmdrRailOpen: false })
  void shrinkMainWindowForRail(askCmdrState.width)
  returnFocusToPane()
}

export function toggleRail(): void {
  if (askCmdrState.open) {
    closeRail()
  } else {
    void openRail()
  }
}

/** Mark the rail as holding focus (the composer gained it). */
export function markRailFocused(): void {
  explorerState.setRailFocused(true)
}

/** Return focus from the rail to the dual-pane explorer (the Esc affordance). */
export function returnFocusToPane(): void {
  explorerState.setRailFocused(false)
  document.querySelector<HTMLElement>('.dual-pane-explorer')?.focus()
}

/** Set the rail width, clamped to its bounds, and persist it. */
export function setRailWidth(width: number): void {
  askCmdrState.width = clampWidth(width)
  saveAppStatus({ askCmdrRailWidth: askCmdrState.width })
}

function clampWidth(width: number): number {
  return Math.min(RAIL_MAX_WIDTH, Math.max(RAIL_MIN_WIDTH, Math.round(width)))
}

// ── Threads ────────────────────────────────────────────────────────────────────

/** Start a fresh, unsaved chat (a new thread is created lazily on the first send). */
export function newChat(): void {
  if (askCmdrState.streaming) stopStreaming()
  askCmdrState.conversationId = null
  askCmdrState.messages = []
  askCmdrState.messageTotal = 0
  askCmdrState.historyCount = 0
  askCmdrState.attachments = []
  discardRenameReview()
}

/** Switch the rail to an existing thread and load its most recent page. */
export async function switchToThread(id: number): Promise<void> {
  if (askCmdrState.streaming) stopStreaming()
  askCmdrState.attachments = []
  discardRenameReview()
  await loadConversation(id)
}

async function bootstrapActiveThread(): Promise<void> {
  askCmdrState.loadingHistory = true
  try {
    const recent = await listAskCmdrConversations(1, 0, false)
    const latest = recent.at(0)
    if (latest) {
      await loadConversation(latest.id)
    }
  } catch (e) {
    log.warn('bootstrapping the active thread failed: {error}', { error: String(e) })
  } finally {
    askCmdrState.loadingHistory = false
  }
}

/** Load a thread's most recent page into the rail (tail-first). One probe fetch learns
 * the total; a thread longer than a page then refetches its newest page. */
async function loadConversation(id: number): Promise<void> {
  askCmdrState.loadingHistory = true
  try {
    const probe = await getAskCmdrConversation(id, MESSAGE_PAGE, 0)
    if (!probe) return
    let detail = probe
    if (probe.totalMessages > MESSAGE_PAGE) {
      const tailOffset = probe.totalMessages - MESSAGE_PAGE
      detail = (await getAskCmdrConversation(id, MESSAGE_PAGE, tailOffset)) ?? probe
    }
    askCmdrState.conversationId = id
    askCmdrState.messageTotal = detail.totalMessages
    askCmdrState.historyCount = detail.messages.length
    askCmdrState.messages = buildRailMessages(detail)
  } finally {
    askCmdrState.loadingHistory = false
  }
}

/** Prepend the page of history immediately older than what's shown. Offset is derived
 * from `historyCount` against the load-time total, so pages tile with no overlap and
 * live-streamed messages (newer than the total) are never disturbed. */
export async function loadOlderMessages(): Promise<void> {
  const id = askCmdrState.conversationId
  if (id === null || askCmdrState.loadingOlder || !hasOlderMessages()) return
  askCmdrState.loadingOlder = true
  try {
    const remaining = askCmdrState.messageTotal - askCmdrState.historyCount
    const limit = Math.min(MESSAGE_PAGE, remaining)
    const offset = remaining - limit
    const detail = await getAskCmdrConversation(id, limit, offset)
    if (!detail) return
    askCmdrState.messages = [...buildRailMessages(detail), ...askCmdrState.messages]
    askCmdrState.historyCount += detail.messages.length
  } catch (e) {
    log.warn('loading earlier messages failed: {error}', { error: String(e) })
  } finally {
    askCmdrState.loadingOlder = false
  }
}

/** Fold a loaded conversation's messages into rail items: tool results are attached to the
 * assistant tool call they answer (by `callId`), so the thread shows one line per call. */
function buildRailMessages(detail: ConversationDetailView): RailMessage[] {
  // A plain lookup (not a reactive SvelteMap): purely local to this pure transform.
  const resultOk: Record<string, boolean> = {}
  for (const message of detail.messages) {
    for (const block of message.blocks) {
      if (block.type === 'toolResult') resultOk[block.callId] = block.ok
    }
  }
  const out: RailMessage[] = []
  for (const message of detail.messages) {
    if (message.role === 'user') {
      out.push({ kind: 'user', id: message.id, text: joinText(message), attachments: [] })
    } else if (message.role === 'assistant') {
      out.push({
        kind: 'assistant',
        id: message.id,
        text: joinText(message),
        tools: toolCallsOf(message, resultOk),
        thinking: false,
        streaming: false,
      })
    } else if (message.role === 'event') {
      for (const block of message.blocks) {
        if (block.type === 'modelChanged') out.push({ kind: 'modelChange', model: block.model })
      }
    }
    // `tool`-role messages carry only results, already folded into the tool lines above.
  }
  return out
}

function joinText(message: MessageView): string {
  return message.blocks
    .filter((b): b is Extract<typeof b, { type: 'text' }> => b.type === 'text')
    .map((b) => b.text)
    .join('')
}

function toolCallsOf(message: MessageView, resultOk: Record<string, boolean>): RailToolCall[] {
  return message.blocks
    .filter((b): b is Extract<typeof b, { type: 'toolCall' }> => b.type === 'toolCall')
    .map((b) => ({
      callId: b.callId,
      tool: b.tool,
      running: false,
      ok: resultOk[b.callId] ?? true,
      path: pathFromArguments(b.arguments),
    }))
}

/** Pull a `path` field out of a tool call's JSON arguments for the "looked at X" label. */
export function pathFromArguments(argumentsJson: string): string | null {
  try {
    const parsed = JSON.parse(argumentsJson) as unknown
    if (parsed && typeof parsed === 'object' && 'path' in parsed) {
      const path = parsed.path
      if (typeof path === 'string' && path.length > 0) return path
    }
  } catch {
    // Malformed arguments just yield no path suffix.
  }
  return null
}

// ── Sending + streaming ──────────────────────────────────────────────────────────

/** Send the user's message and stream the answer. No-ops on empty text or while streaming
 * (single-flight per thread; the composer is disabled mid-turn). */
export function sendMessage(text: string): void {
  const trimmed = text.trim()
  if (!trimmed || askCmdrState.streaming) return
  const attachments = askCmdrState.attachments
  askCmdrState.messages.push({ kind: 'user', id: null, text: trimmed, attachments })
  askCmdrState.streaming = true
  resetProgressWatchdog()
  askCmdrState.attachments = []
  void sendAskCmdrMessage(askCmdrState.conversationId, trimmed, attachments, handleStreamEvent).then(
    (id) => {
      askCmdrState.conversationId = id
    },
    (e: unknown) => {
      log.warn('sending a message failed: {error}', { error: String(e) })
      if (askCmdrState.streaming) applyFailed('provider', String(e))
    },
  )
}

// ── Attachments (staged in the composer for the next send) ─────────────────────

/** Stage attachment refs in the composer, de-duplicated by path (counts stay tiny, so a
 * linear check beats a reactive Set). */
export function addAttachments(refs: AttachmentRef[]): void {
  for (const ref of refs) {
    if (!askCmdrState.attachments.some((a) => a.path === ref.path)) {
      askCmdrState.attachments.push(ref)
    }
  }
}

/** Remove one staged attachment by path. */
export function removeAttachment(path: string): void {
  askCmdrState.attachments = askCmdrState.attachments.filter((a) => a.path !== path)
}

/** Stop the in-flight turn. The runtime sends no terminal event on cancel, so finalize the
 * current bubble locally. */
export function stopStreaming(): void {
  if (!askCmdrState.streaming) return
  if (askCmdrState.conversationId !== null) void cancelAskCmdr(askCmdrState.conversationId)
  finalizeAssistant()
  askCmdrState.streaming = false
  clearProgressWatchdog()
}

function handleStreamEvent(event: AskCmdrStreamEvent): void {
  switch (event.type) {
    case 'started':
      askCmdrState.conversationId = event.conversationId
      return
    case 'queued':
      return
    case 'userPersisted':
      applyUserPersisted(event.messageId)
      return
    case 'assistantStarted':
      {
        const assistant = currentAssistant()
        if (assistant) {
          assistant.streaming = true
        } else {
          askCmdrState.messages.push({
            kind: 'assistant',
            id: null,
            text: '',
            tools: [],
            thinking: false,
            stalled: false,
            streaming: true,
          })
        }
      }
      return
    case 'textDelta':
      applyTextDelta(event.text)
      return
    case 'reasoningTick':
      applyThinking()
      return
    case 'toolCallStarted':
      applyToolStarted(event.callId, event.tool)
      return
    case 'toolCallFinished':
      applyToolFinished(event.callId, event.ok)
      return
    case 'proposalReady':
      openRenameReview(event.proposal)
      return
    case 'done':
      applyDone(event.messageId)
      return
    case 'failed':
      applyFailed(event.kind, event.detail)
      return
    case 'modelChanged':
      applyModelChanged(event.model)
  }
}

function applyUserPersisted(messageId: number): void {
  const user = lastUserMessage()
  if (user) user.id = messageId
}

function applyTextDelta(text: string): void {
  const assistant = currentAssistant()
  if (assistant) {
    assistant.text += text
    assistant.thinking = false
    assistant.stalled = false
    resetProgressWatchdog()
  }
}

function applyThinking(): void {
  const assistant = currentAssistant()
  if (assistant) assistant.thinking = true
}

function applyToolStarted(callId: string, tool: string): void {
  currentAssistant()?.tools.push({ callId, tool, running: true, ok: true, path: null })
  resetProgressWatchdog()
}

function applyToolFinished(callId: string, ok: boolean): void {
  const tool = askCmdrState.messages
    .findLast(
      (message): message is Extract<RailMessage, { kind: 'assistant' }> =>
        message.kind === 'assistant' && message.tools.some((candidate) => candidate.callId === callId),
    )
    ?.tools.find((candidate) => candidate.callId === callId)
  if (tool) {
    tool.running = false
    tool.ok = ok
  }
  resetProgressWatchdog()
}

function applyDone(messageId: number): void {
  finalizeAssistant(messageId)
  askCmdrState.streaming = false
  clearProgressWatchdog()
}

function applyFailed(kind: AskCmdrErrorKind, detail: string | null): void {
  finalizeAssistant()
  askCmdrState.messages.push({ kind: 'error', errorKind: kind, detail: detail ?? undefined })
  askCmdrState.streaming = false
  clearProgressWatchdog()
}

function resetProgressWatchdog(): void {
  clearProgressWatchdog()
  if (!askCmdrState.streaming) return
  stallTimer = setTimeout(() => {
    const assistant = currentAssistant()
    if (assistant?.streaming) assistant.stalled = true
  }, STALL_AFTER_MS)
  stopTimer = setTimeout(() => {
    if (!askCmdrState.streaming) return
    stopStreaming()
    askCmdrState.messages.push({ kind: 'error', errorKind: 'timeout' })
  }, STOP_AFTER_MS)
}

function clearProgressWatchdog(): void {
  if (stallTimer) clearTimeout(stallTimer)
  if (stopTimer) clearTimeout(stopTimer)
  stallTimer = null
  stopTimer = null
}

/** The model changed between the previous turn and this one, so the line belongs BEFORE
 * this turn's user bubble (which is already rendered optimistically). */
function applyModelChanged(model: string): void {
  const item: RailMessage = { kind: 'modelChange', model }
  const lastUserIndex = askCmdrState.messages.findLastIndex((m) => m.kind === 'user')
  if (lastUserIndex >= 0) askCmdrState.messages.splice(lastUserIndex, 0, item)
  else askCmdrState.messages.push(item)
}

function openRenameReview(proposal: Extract<AskCmdrStreamEvent, { type: 'proposalReady' }>['proposal']): void {
  discardRenameReview()
  askCmdrState.renameReview = {
    proposalId: proposal.proposalId,
    rows: proposal.rows.map((row) => ({ ...row, allowed: true, blockedReason: null })),
    preflighting: false,
    expired: false,
    requestVersion: 0,
  }
  void refreshRenamePreflight()
}

/** Change one row's user decision, then revalidate the exact allowed subset. */
export function setRenameRowAllowed(rowId: string, allowed: boolean): void {
  const review = askCmdrState.renameReview
  const row = review?.rows.find((candidate) => candidate.rowId === rowId)
  if (!review || !row || (row.blockedReason && allowed)) return
  row.allowed = allowed
  void refreshRenamePreflight()
}

/** Allow every row the latest preflight did not block. */
export function allowAllRenameRows(): void {
  const review = askCmdrState.renameReview
  if (!review) return
  for (const row of review.rows) {
    if (!row.blockedReason) row.allowed = true
  }
  void refreshRenamePreflight()
}

/** Deny every row. This sends no filesystem request and creates no operation. */
export function denyAllRenameRows(): void {
  const review = askCmdrState.renameReview
  if (!review) return
  for (const row of review.rows) row.allowed = false
  void refreshRenamePreflight()
}

/** Cancel closes the review and consumes its server-owned proposal. */
export function cancelRenameReview(): void {
  const review = askCmdrState.renameReview
  if (!review) return
  askCmdrState.renameReview = null
  void cancelBulkRenameProposal(review.proposalId)
}

/** Starts the one managed operation for the rows the user currently allows. */
export async function applyRenameReview(): Promise<void> {
  const review = askCmdrState.renameReview
  if (!review || review.preflighting || review.expired) return
  const allowedRowIds = review.rows.filter((row) => row.allowed && !row.blockedReason).map((row) => row.rowId)
  if (allowedRowIds.length === 0) return
  review.preflighting = true
  try {
    await applyBulkRename(review.proposalId, allowedRowIds)
    if (askCmdrState.renameReview?.proposalId === review.proposalId) askCmdrState.renameReview = null
  } catch (e) {
    const current = askCmdrState.renameReview
    if (!current || current.proposalId !== review.proposalId) return
    current.preflighting = false
    log.warn('starting the rename plan failed: {error}', { error: String(e) })
    void refreshRenamePreflight()
  }
}

function discardRenameReview(): void {
  const review = askCmdrState.renameReview
  if (!review) return
  askCmdrState.renameReview = null
  void cancelBulkRenameProposal(review.proposalId)
}

async function refreshRenamePreflight(): Promise<void> {
  const review = askCmdrState.renameReview
  if (!review) return
  const version = review.requestVersion + 1
  review.requestVersion = version
  review.preflighting = true
  const allowedRowIds = review.rows.filter((row) => row.allowed).map((row) => row.rowId)
  try {
    const result = await preflightBulkRename(review.proposalId, allowedRowIds)
    const current = askCmdrState.renameReview
    if (!current || current.proposalId !== review.proposalId || current.requestVersion !== version) return
    current.preflighting = false
    current.expired = result.status === 'expired'
    if (current.expired) return
    for (const row of current.rows) {
      const backend = result.rows.find((candidate) => candidate.rowId === row.rowId)
      row.blockedReason = backend?.status === 'blocked' ? backend.reason : null
      if (row.blockedReason) row.allowed = false
    }
  } catch (e) {
    const current = askCmdrState.renameReview
    if (!current || current.proposalId !== review.proposalId || current.requestVersion !== version) return
    current.preflighting = false
    log.warn('checking the rename plan failed: {error}', { error: String(e) })
  }
}

/** How long to wait after a model-affecting settings change before asking the backend to
 * record it: outlasts the settings store's 500 ms debounced disk flush (the backend
 * re-reads `settings.json`) and collapses the model text field's keystrokes. */
const MODEL_CHANGE_DEBOUNCE_MS = 1000

let modelChangeTimer: ReturnType<typeof setTimeout> | null = null

/** A model-affecting setting changed (wired from `settings-applier.ts`). After the
 * debounce, asks the backend to record the change for the active thread — the backend
 * queues on the thread's single-flight lock, so with a turn in flight the line lands
 * right after the reply. The backend answers `null` when nothing actually changed for
 * this thread (no turn yet, or the effective model is the same). */
export function noteModelSettingChanged(): void {
  if (modelChangeTimer) clearTimeout(modelChangeTimer)
  modelChangeTimer = setTimeout(() => {
    modelChangeTimer = null
    void recordModelChangeForActiveThread()
  }, MODEL_CHANGE_DEBOUNCE_MS)
}

async function recordModelChangeForActiveThread(): Promise<void> {
  const conversationId = askCmdrState.conversationId
  if (conversationId == null) return
  try {
    const event = await recordAskCmdrModelChange(conversationId)
    if (!event) return
    // The backend may have waited out an in-flight turn; if the user switched threads
    // meanwhile, the row is persisted (it shows on revisit) but doesn't belong here.
    if (askCmdrState.conversationId !== conversationId) return
    for (const block of event.blocks) {
      if (block.type === 'modelChanged') askCmdrState.messages.push({ kind: 'modelChange', model: block.model })
    }
  } catch (e) {
    log.warn('recording a model change failed: {error}', { error: String(e) })
  }
}

function currentAssistant(): Extract<RailMessage, { kind: 'assistant' }> | null {
  const last = askCmdrState.messages.at(-1)
  return last?.kind === 'assistant' ? last : null
}

function lastUserMessage(): Extract<RailMessage, { kind: 'user' }> | null {
  for (let i = askCmdrState.messages.length - 1; i >= 0; i--) {
    const message = askCmdrState.messages[i]
    if (message.kind === 'user') return message
  }
  return null
}

/** Finalize the streaming assistant bubble: retire unfinished activity, stop its cursor,
 * and drop it if it never produced anything. Finished tool history stays visible. */
function finalizeAssistant(messageId?: number): void {
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
