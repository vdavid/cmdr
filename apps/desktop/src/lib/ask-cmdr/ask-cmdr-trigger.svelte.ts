/**
 * The Ask Cmdr rail itself: open/close, focus, width, and which conversation is loaded. Also
 * the entry point every consumer imports from — it re-exports the slices it composes, so a
 * component asks this module for the rail and gets the whole feature.
 *
 * The slices, each usable on its own:
 *
 * - `ask-cmdr-state.svelte.ts`: the shared `$state` object, its types, and its accessors.
 * - `ask-cmdr-stream.svelte.ts`: sending a message and folding the event stream into it.
 * - `ask-cmdr-rename-review.svelte.ts`: the bulk-rename review and its undo.
 *
 * Modeled on `operation-log-trigger.svelte.ts` (a module-level `$state` object mutated by
 * exported functions).
 */

import { saveAppStatus } from '$lib/app-status-store'
import { explorerState } from '$lib/file-explorer/pane/explorer-state.svelte'
import { getAppLogger } from '$lib/logging/logger'
import { consentState, refreshConsent } from './ask-cmdr-consent.svelte'
import { buildRailMessages } from './ask-cmdr-history'
import { discardRenameReview } from './ask-cmdr-rename-review.svelte'
import { askCmdrState, hasOlderMessages, MESSAGE_PAGE, RAIL_MAX_WIDTH, RAIL_MIN_WIDTH } from './ask-cmdr-state.svelte'
import { stopStreaming } from './ask-cmdr-stream.svelte'
import { growMainWindowForRail, shrinkMainWindowForRail } from './rail-window'
import {
  getAskCmdrConversation,
  listAskCmdrConversations,
  recordAskCmdrModelChange,
  type AttachmentRef,
} from '$lib/tauri-commands'

const log = getAppLogger('askCmdr')

export type { RailMessage, RailToolCall } from './ask-cmdr-messages'
export type { BulkRenameReview, BulkRenameReviewRow } from './ask-cmdr-state.svelte'
export {
  askCmdrState,
  hasOlderMessages,
  isOverSoftCap,
  MESSAGE_PAGE,
  RAIL_MAX_WIDTH,
  RAIL_MIN_WIDTH,
  THREAD_SOFT_CAP_MESSAGES,
} from './ask-cmdr-state.svelte'
export { sendMessage, stopStreaming } from './ask-cmdr-stream.svelte'
export {
  allowAllRenameRows,
  applyRenameReview,
  cancelRenameReview,
  denyAllRenameRows,
  noteRenameApplied,
  renameReviewListingChanged,
  reviseRenameRow,
  setRenameRowAllowed,
  undoRename,
} from './ask-cmdr-rename-review.svelte'

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
  // A fresh chat has measured nothing, so the gauge must not carry the previous thread's fill.
  askCmdrState.contextUsage = null
  askCmdrState.deniedNames = []
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
    // The gauge shows the thread's last measured turn, so reopening a chat reports what it
    // really cost instead of an empty bar. `elidedResults` is 0 here on purpose: whether THAT
    // turn set anything aside isn't persisted, and inventing a count would be a false claim.
    askCmdrState.contextUsage = detail.lastContextUsage
      ? {
          estimatedTokens: detail.lastContextUsage.estimatedTokens,
          budgetTokens: detail.lastContextUsage.budgetTokens,
          elidedResults: 0,
        }
      : null
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
