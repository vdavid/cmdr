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

import { saveAppStatus } from '#lib/app-status-store.js'
import { explorerState } from '#lib/file-explorer/pane/explorer-state.svelte.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { getEffectiveShortcuts, toDisplayShortcut } from '#lib/shortcuts/index.js'
import { focusHome } from '#lib/ui/focus-restore.js'
import { refreshRailGate } from './ask-cmdr-gate.svelte'
import { buildRailMessages } from './ask-cmdr-history'
import { discardRenameReview } from './ask-cmdr-rename-review.svelte'
import { mergeAtSeam } from './ask-cmdr-rounds'
import { refreshSlotStatus } from './ask-cmdr-slot-status.svelte'
import { carryGroupStateAcrossSeam, forgetToolDetails } from './ask-cmdr-tool-groups.svelte'
import { askCmdrState, hasOlderMessages, MESSAGE_PAGE, RAIL_MAX_WIDTH, RAIL_MIN_WIDTH } from './ask-cmdr-state.svelte'
import { followJoinedBubble, leaveChat, resumeThread, stopStreaming } from './ask-cmdr-stream.svelte'
import { settleDecisionsHeardWhileLoading } from './ask-cmdr-decisions'
import { growMainWindowForRail, shrinkMainWindowForRail } from './rail-window'
import {
  getAskCmdrConversation,
  listAskCmdrConversations,
  recordAskCmdrSlotChange,
  type AttachmentRef,
} from '#lib/tauri-commands/index.js'

const log = getAppLogger('askCmdr')

export type { RailMessage, RailToolCall } from './ask-cmdr-messages'
export type { BulkRenameReview, BulkRenameReviewProposal, BulkRenameReviewRow } from './ask-cmdr-state.svelte'
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
 * Also refreshes the rail's gate (`ask-cmdr-gate.svelte.ts`): while Ask Cmdr is off, or Cloud
 * waits for "Allow cloud AI", the rail shows the gate and loads no history. When the gate later
 * opens, the rail calls {@link ensureThreadLoaded} itself. A new chat the user asked for
 * ({@link openNewChat}) is never bootstrapped over (`askCmdrState.freshChat`). */
export async function openRail(opts: { resizeWindow?: boolean } = {}): Promise<void> {
  const wasOpen = askCmdrState.open
  askCmdrState.open = true
  explorerState.setRailFocused(true)
  saveAppStatus({ askCmdrRailOpen: true })
  // Only a genuine closed→open transition grows the window; re-opens (e.g. after consenting) and
  // startup hydration must not.
  if (!wasOpen && opts.resizeWindow !== false) void growMainWindowForRail(askCmdrState.width)
  if ((await refreshRailGate()) !== 'chat') return
  if (!wasOpen) await ensureThreadLoaded()
}

/** Ask the composer to take focus (it listens on `composerFocusRequest`). A no-op while no
 * composer is mounted: the rail is closed or behind a gate. */
export function requestComposerFocus(): void {
  askCmdrState.composerFocusRequest++
}

/**
 * The New chat action (the header button, the getting-long nudge, and ⌘N): start a fresh chat,
 * open the rail if it's closed, and put the cursor in the prompt box so typing starts at once.
 */
export function openNewChat(): void {
  newChat()
  askCmdrState.freshChat = true
  if (!askCmdrState.open) void openRail()
  requestComposerFocus()
}

/** The New chat shortcut as the tooltip shows it (`⌘N` by default, or the user's rebind), or
 * `undefined` when it's unbound. Read at render, so a rebind shows on the next one. */
export function newChatShortcutText(): string | undefined {
  // `.at(0)`, not `[0]`: an unbound command has no shortcuts, and `[0]` would type that away.
  const shortcut = getEffectiveShortcuts('askCmdr.newChat').at(0)
  return shortcut === undefined ? undefined : toDisplayShortcut(shortcut)
}

/** Load the most recent thread into an empty rail. A no-op once a thread (or a fresh chat's
 * first message) is showing, on a new chat the user asked for, or while a load is already
 * running. */
export async function ensureThreadLoaded(): Promise<void> {
  if (askCmdrState.loadingHistory || askCmdrState.freshChat) return
  if (askCmdrState.conversationId !== null || askCmdrState.messages.length > 0) return
  await bootstrapActiveThread()
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

/** Return focus from the rail to the dual-pane explorer, on the focused pane's list (the Esc affordance). */
export function returnFocusToPane(): void {
  explorerState.setRailFocused(false)
  focusHome()
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

/** Start a fresh, unsaved chat (a new thread is created lazily on the first send). A send
 * still waiting for its thread id stays with the chat it was typed in ({@link leaveChat}). */
export function newChat(): void {
  if (askCmdrState.streaming) stopStreaming()
  leaveChat()
  askCmdrState.conversationId = null
  askCmdrState.freshChat = false
  askCmdrState.messages = []
  askCmdrState.messageTotal = 0
  askCmdrState.historyCount = 0
  askCmdrState.attachments = []
  forgetToolDetails()
  // A fresh chat has measured nothing, so the gauge must not carry the previous thread's fill.
  askCmdrState.contextUsage = null
  askCmdrState.deniedNames = []
  discardRenameReview()
  abandonThreadLoad()
}

/** Switch the rail to an existing thread and load its most recent page. */
export async function switchToThread(id: number): Promise<void> {
  if (askCmdrState.streaming) stopStreaming()
  leaveChat()
  askCmdrState.attachments = []
  discardRenameReview()
  await loadConversation(id)
}

async function bootstrapActiveThread(): Promise<void> {
  const load = beginThreadLoad()
  try {
    const recent = await listAskCmdrConversations(1, 0, false)
    const latest = recent.at(0)
    // Picking a thread (or starting a new chat) while the list was on its way back wins.
    if (latest && load === latestThreadLoad) {
      await loadConversation(latest.id)
    }
  } catch (e) {
    log.warn('bootstrapping the active thread failed: {error}', { error: e })
  } finally {
    finishThreadLoad(load)
  }
}

/**
 * The newest thread load, so an older one can tell it has been overtaken.
 *
 * Each read is its own command, so the first one asked can be the last one answered. Without
 * this, picking thread A and then thread B could leave A on screen (its answer arriving second),
 * and a late answer could repaint a chat the user had already cleared. Only the latest load may
 * touch the rail; an overtaken one ends without writing anything, the loading flag included.
 */
let latestThreadLoad = 0

/** Start a thread load, overtaking any that is still in flight. Returns its token. */
function beginThreadLoad(): number {
  askCmdrState.loadingHistory = true
  return ++latestThreadLoad
}

/** End `load`, unless a newer one has taken over (which then owns the ending). */
function finishThreadLoad(load: number): void {
  if (load === latestThreadLoad) endThreadLoad()
}

/** The rail moved on without loading anything (a new chat): whatever is in flight is stale. */
function abandonThreadLoad(): void {
  latestThreadLoad++
  endThreadLoad()
}

/** A thread load is over, whether or not it put anything on screen. Decisions the rail heard
 * meanwhile go back in here, because the load replaced the list they were shown in (or the
 * rail wasn't on their thread yet) and its read may have been too early to include them. */
function endThreadLoad(): void {
  askCmdrState.loadingHistory = false
  settleDecisionsHeardWhileLoading()
}

/** Load a thread's most recent page into the rail (tail-first). One probe fetch learns
 * the total; a thread longer than a page then refetches its newest page. */
async function loadConversation(id: number): Promise<void> {
  const load = beginThreadLoad()
  try {
    const probe = await getAskCmdrConversation(id, MESSAGE_PAGE, 0)
    if (!probe) return
    let detail = probe
    if (probe.totalMessages > MESSAGE_PAGE) {
      const tailOffset = probe.totalMessages - MESSAGE_PAGE
      detail = (await getAskCmdrConversation(id, MESSAGE_PAGE, tailOffset)) ?? probe
    }
    if (load !== latestThreadLoad) return
    if (askCmdrState.conversationId !== id) forgetToolDetails()
    askCmdrState.conversationId = id
    askCmdrState.freshChat = false
    askCmdrState.messageTotal = detail.totalMessages
    askCmdrState.historyCount = detail.messages.length
    askCmdrState.messages = buildRailMessages(detail)
    resumeThread(id, detail.turnInFlightSinceSeq !== null)
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
    finishThreadLoad(load)
  }
}

/** Prepend the page of history immediately older than what's shown. Offset is derived
 * from `historyCount` against the load-time total, so pages tile with no overlap and
 * live-streamed messages (newer than the total) are never disturbed. */
export async function loadOlderMessages(): Promise<void> {
  const id = askCmdrState.conversationId
  if (id === null || askCmdrState.loadingOlder || !hasOlderMessages()) return
  askCmdrState.loadingOlder = true
  // The page tiles against the list as it was loaded, so any thread load since (another
  // thread, or this one again) makes it a page of something no longer on screen.
  const shownBy = latestThreadLoad
  try {
    const remaining = askCmdrState.messageTotal - askCmdrState.historyCount
    const limit = Math.min(MESSAGE_PAGE, remaining)
    const offset = remaining - limit
    const detail = await getAskCmdrConversation(id, limit, offset)
    if (!detail || shownBy !== latestThreadLoad) return
    // A turn longer than the page boundary arrives in two halves; they fold back into one bubble,
    // and a group the seam joined keeps the open state it had, a stop waiting there its bubble.
    const head = askCmdrState.messages.find((item) => item.kind === 'assistant')
    askCmdrState.messages = mergeAtSeam(buildRailMessages(detail), askCmdrState.messages)
    carryGroupStateAcrossSeam(head, askCmdrState.messages)
    const joined = askCmdrState.messages.find((item) => item.kind === 'assistant' && item.turnId === head?.turnId)
    if (head?.turnId != null && joined?.kind === 'assistant') followJoinedBubble(head, joined)
    askCmdrState.historyCount += detail.messages.length
  } catch (e) {
    log.warn('loading earlier messages failed: {error}', { error: e })
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

/** How long to wait after a slot-affecting settings change before asking the backend to
 * record it: outlasts the settings store's 500 ms debounced disk flush (the backend
 * re-reads `settings.json`) and collapses the model text field's keystrokes. */
const SLOT_CHANGE_DEBOUNCE_MS = 1000

let slotChangeTimer: ReturnType<typeof setTimeout> | null = null

/** A setting that moves the Ask Cmdr slot changed — the model it sends to or the chat memory
 * each message carries (wired from `settings-applier.ts`). After the debounce, asks the
 * backend to record what actually moved for the active thread — the backend queues on the
 * thread's single-flight lock, so with a turn in flight the lines land right after the
 * reply. The backend answers an empty list when nothing changed for this thread (no turn
 * yet, or the effective values are the same). */
export function noteSlotSettingChanged(): void {
  if (slotChangeTimer) clearTimeout(slotChangeTimer)
  slotChangeTimer = setTimeout(() => {
    slotChangeTimer = null
    // The same move changes the rail's baseline (budget, memory slice) and pricing.
    void refreshSlotStatus()
    void recordSlotChangeForActiveThread()
  }, SLOT_CHANGE_DEBOUNCE_MS)
}

async function recordSlotChangeForActiveThread(): Promise<void> {
  const conversationId = askCmdrState.conversationId
  if (conversationId == null) return
  try {
    const events = await recordAskCmdrSlotChange(conversationId)
    if (events.length === 0) return
    // The backend may have waited out an in-flight turn; if the user switched threads
    // meanwhile, the rows are persisted (they show on revisit) but don't belong here.
    if (askCmdrState.conversationId !== conversationId) return
    for (const event of events) {
      for (const block of event.blocks) {
        if (block.type === 'modelChanged') askCmdrState.messages.push({ kind: 'modelChange', model: block.model })
        if (block.type === 'chatMemoryChanged') {
          askCmdrState.messages.push({ kind: 'chatMemoryChange', chatMemoryTokens: block.chatMemoryTokens })
        }
      }
    }
  } catch (e) {
    log.warn('recording a slot change failed: {error}', { error: e })
  }
}
