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
import {
  cancelAskCmdr,
  getAskCmdrConversation,
  listAskCmdrConversations,
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
  | { kind: 'assistant'; id: number | null; text: string; tools: RailToolCall[]; thinking: boolean; streaming: boolean }
  | { kind: 'error'; errorKind: AskCmdrErrorKind }

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

/** Apply persisted rail state at startup (called once from `loadPersistedState`). */
export function hydrateRail(open: boolean, width: number): void {
  askCmdrState.width = clampWidth(width)
  if (open) void openRail()
}

/** Open the rail, focus its composer, and bootstrap the most recent thread if empty. Also
 * refreshes the consent gate: the rail shows the consent screen until the user opts in, and
 * only then bootstraps history (no chat exists to load before consent). */
export async function openRail(): Promise<void> {
  const wasOpen = askCmdrState.open
  askCmdrState.open = true
  explorerState.setRailFocused(true)
  saveAppStatus({ askCmdrRailOpen: true })
  await refreshConsent()
  if (consentState.accepted !== true) return
  if (!wasOpen && askCmdrState.conversationId === null && askCmdrState.messages.length === 0) {
    await bootstrapActiveThread()
  }
}

/** Close the rail and return focus to the active pane. */
export function closeRail(): void {
  askCmdrState.open = false
  explorerState.setRailFocused(false)
  saveAppStatus({ askCmdrRailOpen: false })
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
}

/** Switch the rail to an existing thread and load its most recent page. */
export async function switchToThread(id: number): Promise<void> {
  if (askCmdrState.streaming) stopStreaming()
  askCmdrState.attachments = []
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
  askCmdrState.attachments = []
  void sendAskCmdrMessage(askCmdrState.conversationId, trimmed, attachments, handleStreamEvent).then(
    (id) => {
      askCmdrState.conversationId = id
    },
    (e: unknown) => {
      log.warn('sending a message failed: {error}', { error: String(e) })
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
      askCmdrState.messages.push({ kind: 'assistant', id: null, text: '', tools: [], thinking: false, streaming: true })
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
    case 'done':
      applyDone(event.messageId)
      return
    case 'failed':
      applyFailed(event.kind)
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
  }
}

function applyThinking(): void {
  const assistant = currentAssistant()
  if (assistant) assistant.thinking = true
}

function applyToolStarted(callId: string, tool: string): void {
  currentAssistant()?.tools.push({ callId, tool, running: true, ok: true, path: null })
}

function applyToolFinished(callId: string, ok: boolean): void {
  const tool = currentAssistant()?.tools.find((t) => t.callId === callId)
  if (tool) {
    tool.running = false
    tool.ok = ok
  }
}

function applyDone(messageId: number): void {
  const assistant = currentAssistant()
  if (assistant) {
    assistant.streaming = false
    assistant.thinking = false
    assistant.id = messageId
  }
  askCmdrState.streaming = false
}

function applyFailed(kind: AskCmdrErrorKind): void {
  finalizeAssistant()
  askCmdrState.messages.push({ kind: 'error', errorKind: kind })
  askCmdrState.streaming = false
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

/** Finalize the streaming assistant bubble: stop its cursor, and drop it if it never
 * produced anything (an empty bubble left by a failure or a cancel before any output). */
function finalizeAssistant(): void {
  const assistant = currentAssistant()
  if (!assistant) return
  assistant.streaming = false
  assistant.thinking = false
  if (assistant.text.length === 0 && assistant.tools.length === 0) {
    askCmdrState.messages.pop()
  }
}
