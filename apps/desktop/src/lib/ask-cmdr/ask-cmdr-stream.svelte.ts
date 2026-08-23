/**
 * The live turn: sending a message, folding the stream of events into the thread, and
 * stopping.
 *
 * `applyStreamEvent` is the whole reducer — one arm per `AskCmdrStreamEvent`, each mutating
 * the last assistant message in place. Around it sit the two entry points (`sendMessage`,
 * `stopStreaming`) and the progress watchdog that marks a quiet turn stalled and eventually
 * ends it.
 *
 * **The rail subscribes by CONVERSATION, not by send.** Events arrive for every thread the
 * backend is working on, including wakes nobody asked for, so `handleTurnEvent` keeps only
 * the ones about the thread on screen. That is what makes a reload mid-turn recoverable:
 * the rail reloads, lands on the same thread, and the turn it never stopped writing keeps
 * rendering. `adoptLiveTurn` is the other half — after a reload there is no `assistantStarted`
 * left to hear, so any live event is taken as proof a turn is running.
 *
 * **Cancel finalizes locally.** The runtime returns `Cancelled` with NO terminal event, so a
 * stop won't be echoed back — `stopStreaming` finalizes the bubble itself, and the thread
 * goes on the stopped list so a late chunk can't resurrect the "working…" state.
 */

import { getAppLogger } from '$lib/logging/logger'
import type { RailMessage } from './ask-cmdr-messages'
import { openRenameReview } from './ask-cmdr-rename-review.svelte'
import { askCmdrState, currentAssistant, finalizeAssistant, lastUserMessage } from './ask-cmdr-state.svelte'
import {
  cancelAskCmdr,
  sendAskCmdrMessage,
  type AskCmdrErrorKind,
  type AskCmdrStreamEvent,
  type AskCmdrTurn,
} from '$lib/tauri-commands'

const log = getAppLogger('askCmdr')

/** How long a turn may go without any event before its bubble says so, and before the rail
 * gives up on it entirely. */
const STALL_AFTER_MS = 30_000
const STOP_AFTER_MS = 90_000
let stallTimer: ReturnType<typeof setTimeout> | null = null
let stopTimer: ReturnType<typeof setTimeout> | null = null

/**
 * Threads whose in-flight turn the user stopped. A cancel produces no terminal event, so the
 * backend keeps dribbling for a chunk or two; without this, one of those late events would
 * read as "a turn is live here" and put the rail back into working state with no `done`
 * coming to clear it. Cleared when the thread is sent to again.
 */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- bookkeeping only; nothing renders from it
let stoppedTurns = new Set<number>()

/** Forget every stopped turn. Test isolation only; production clears per thread. */
export function resetStoppedTurns(): void {
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- see `stoppedTurns`
  stoppedTurns = new Set<number>()
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
  if (askCmdrState.conversationId !== null) stoppedTurns.delete(askCmdrState.conversationId)
  // The names the user turned down ride this turn only. Clearing them here (not on the
  // response) keeps a retry from re-sending feedback the model already acted on.
  const deniedNames = askCmdrState.deniedNames
  askCmdrState.deniedNames = []
  void sendAskCmdrMessage(askCmdrState.conversationId, trimmed, attachments, deniedNames).then(
    (outcome) => {
      if (outcome.accepted) {
        askCmdrState.conversationId = outcome.conversationId
        stoppedTurns.delete(outcome.conversationId)
      } else if (askCmdrState.streaming) {
        applyFailed(outcome.kind, outcome.detail)
      }
    },
    (e: unknown) => {
      log.warn('sending a message failed: {error}', { error: String(e) })
      if (askCmdrState.streaming) applyFailed('provider', String(e))
    },
  )
}

/** Stop the in-flight turn. The runtime sends no terminal event on cancel, so finalize the
 * current bubble locally and stop listening to what the backend is still finishing. */
export function stopStreaming(): void {
  if (!askCmdrState.streaming) return
  if (askCmdrState.conversationId !== null) {
    stoppedTurns.add(askCmdrState.conversationId)
    void cancelAskCmdr(askCmdrState.conversationId)
  }
  finalizeAssistant()
  askCmdrState.streaming = false
  clearProgressWatchdog()
}

/**
 * One turn event off the shared stream. Applies it when it is about the thread on screen and
 * drops it otherwise — a wake thinking in another thread must not write into what the user is
 * reading.
 *
 * A fresh chat is the one case where the rail has no id yet, so a `started` arriving while it
 * waits adopts the id. That can in principle catch a wake's `started` in the same tick; the
 * send's own promise assigns the real id right after and corrects it.
 */
export function handleTurnEvent(turn: AskCmdrTurn): void {
  const { conversationId, event } = turn
  if (askCmdrState.conversationId === null) {
    if (event.type === 'started' && askCmdrState.streaming) askCmdrState.conversationId = conversationId
    return
  }
  if (conversationId !== askCmdrState.conversationId) return
  if (isTerminal(event)) stoppedTurns.delete(conversationId)
  else if (stoppedTurns.has(conversationId)) return
  applyStreamEvent(event)
}

/** The events that end a turn. They're allowed through on a stopped thread so a `discarded`
 * can still take the thread away. */
function isTerminal(event: AskCmdrStreamEvent): boolean {
  return event.type === 'done' || event.type === 'failed' || event.type === 'discarded'
}

function applyStreamEvent(event: AskCmdrStreamEvent): void {
  switch (event.type) {
    case 'started':
      return
    case 'queued':
      adoptLiveTurn()
      return
    case 'userPersisted':
      adoptLiveTurn()
      applyUserPersisted(event.messageId)
      return
    case 'assistantStarted':
      adoptLiveTurn()
      ensureAssistant()
      return
    case 'textDelta':
      adoptLiveTurn()
      applyTextDelta(event.text)
      return
    case 'reasoningTick':
      adoptLiveTurn()
      applyThinking()
      return
    case 'toolCallStarted':
      adoptLiveTurn()
      applyToolStarted(event.callId, event.tool)
      return
    case 'toolCallFinished':
      adoptLiveTurn()
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
    case 'discarded':
      applyDiscarded()
      return
    default:
      handleContextEvent(event)
  }
}

/** The two events that report on the CONTEXT rather than on the answer: what the assembly set
 * aside, and what it cost. Split out so `applyStreamEvent` stays under the complexity ceiling
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

/**
 * Take an event as proof this thread's turn is running.
 *
 * The rail sets `streaming` itself when the user presses Send, so this only bites when it
 * DIDN'T: a reload mid-answer, or opening a thread a wake is still thinking in. Neither
 * replays `assistantStarted`, so any live event has to be the signal.
 */
function adoptLiveTurn(): void {
  if (askCmdrState.streaming) return
  askCmdrState.streaming = true
  resetProgressWatchdog()
}

/** The streaming bubble this turn writes into, created if the rail wasn't watching when it
 * started (a reload lands on a thread whose `assistantStarted` is already in the past). */
function ensureAssistant(): Extract<RailMessage, { kind: 'assistant' }> {
  const assistant = currentAssistant()
  if (assistant) {
    assistant.streaming = true
    return assistant
  }
  const fresh: Extract<RailMessage, { kind: 'assistant' }> = {
    kind: 'assistant',
    id: null,
    text: '',
    tools: [],
    thinking: false,
    stalled: false,
    streaming: true,
  }
  askCmdrState.messages.push(fresh)
  return fresh
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
  const assistant = ensureAssistant()
  assistant.text += text
  assistant.thinking = false
  assistant.stalled = false
  resetProgressWatchdog()
}

function applyThinking(): void {
  ensureAssistant().thinking = true
}

function applyToolStarted(callId: string, tool: string): void {
  ensureAssistant().tools.push({ callId, tool, running: true, ok: true, path: null })
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

/**
 * The thread the rail is showing was deleted under it: a wake looked, found nothing worth
 * raising, and took its thread with it. There is nothing left to re-read, so the rail steps
 * off it into an empty chat rather than showing a thread that no longer exists.
 */
function applyDiscarded(): void {
  askCmdrState.conversationId = null
  askCmdrState.messages = []
  askCmdrState.messageTotal = 0
  askCmdrState.historyCount = 0
  askCmdrState.contextUsage = null
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
