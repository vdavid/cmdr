/**
 * The live turn: sending a message, folding the stream of events into the thread, and
 * stopping.
 *
 * `handleStreamEvent` is the whole reducer — one arm per `AskCmdrStreamEvent`, each mutating
 * the last assistant message in place. Around it sit the two entry points (`sendMessage`,
 * `stopStreaming`) and the progress watchdog that marks a quiet turn stalled and eventually
 * ends it.
 *
 * **Cancel finalizes locally.** The runtime returns `Cancelled` with NO terminal event, so a
 * stop won't be echoed back — `stopStreaming` finalizes the bubble itself.
 */

import { getAppLogger } from '$lib/logging/logger'
import type { RailMessage } from './ask-cmdr-messages'
import { openRenameReview } from './ask-cmdr-rename-review.svelte'
import { askCmdrState, currentAssistant, finalizeAssistant, lastUserMessage } from './ask-cmdr-state.svelte'
import { cancelAskCmdr, sendAskCmdrMessage, type AskCmdrErrorKind, type AskCmdrStreamEvent } from '$lib/tauri-commands'

const log = getAppLogger('askCmdr')

/** How long a turn may go without any event before its bubble says so, and before the rail
 * gives up on it entirely. */
const STALL_AFTER_MS = 30_000
const STOP_AFTER_MS = 90_000
let stallTimer: ReturnType<typeof setTimeout> | null = null
let stopTimer: ReturnType<typeof setTimeout> | null = null

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
  // The names the user turned down ride this turn only. Clearing them here (not on the
  // response) keeps a retry from re-sending feedback the model already acted on.
  const deniedNames = askCmdrState.deniedNames
  askCmdrState.deniedNames = []
  void sendAskCmdrMessage(askCmdrState.conversationId, trimmed, attachments, deniedNames, handleStreamEvent).then(
    (id) => {
      askCmdrState.conversationId = id
    },
    (e: unknown) => {
      log.warn('sending a message failed: {error}', { error: String(e) })
      if (askCmdrState.streaming) applyFailed('provider', String(e))
    },
  )
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
      return
    default:
      handleContextEvent(event)
  }
}

/** The two events that report on the CONTEXT rather than on the answer: what the assembly set
 * aside, and what it cost. Split out so `handleStreamEvent` stays under the complexity ceiling
 * and both halves stay exhaustive. */
function handleContextEvent(event: Extract<AskCmdrStreamEvent, { type: 'contextTrimmed' | 'contextUsage' }>): void {
  switch (event.type) {
    case 'contextTrimmed':
      applyContextTrimmed(event.elidedResults)
      return
    case 'contextUsage':
      applyContextUsage(event.estimatedTokens, event.budgetTokens, event.elidedResults)
  }
}

/** Record what this turn's prompt cost, for the footer gauge. One event per answered turn, so
 * this is a straight replace rather than an accumulation. */
function applyContextUsage(estimatedTokens: number, budgetTokens: number, elidedResults: number): void {
  askCmdrState.contextUsage = { estimatedTokens, budgetTokens, elidedResults }
}

/** Show that the budget pushed older lookups out of this turn's context. It goes before the
 * streaming bubble, where the drop happened. */
function applyContextTrimmed(elidedResults: number): void {
  const assistant = currentAssistant()
  const notice: RailMessage = { kind: 'contextTrimmed', count: elidedResults }
  if (assistant) {
    askCmdrState.messages.splice(askCmdrState.messages.indexOf(assistant), 0, notice)
  } else {
    askCmdrState.messages.push(notice)
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
