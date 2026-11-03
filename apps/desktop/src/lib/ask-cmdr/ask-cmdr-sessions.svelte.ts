/**
 * Ask Cmdr sessions state: the thread list (recent first, archived filter), cross-thread
 * search (ranked by the backend: title match, then FTS5 relevance, then recency),
 * new/rename/archive, and switch-thread. A separate `$state` slice from the
 * live-chat trigger (`ask-cmdr-trigger.svelte.ts`), which it calls into to switch threads.
 *
 * Paging mirrors the operation-log dialog: the offset is `conversations.length` (one
 * source of truth), so an append can't overlap or desync from what's shown. Search
 * replaces the list while a query is active; clearing it, or closing the panel,
 * restores the list.
 */

import { getAppLogger } from '#lib/logging/logger.js'
import { LogOnceGate } from '#lib/logging/log-once.js'
import {
  archiveAskCmdrConversation,
  listAskCmdrConversations,
  renameAskCmdrConversation,
  searchAskCmdrConversations,
  type ConversationRow,
  type ConversationSearchHit,
} from '#lib/tauri-commands/index.js'
import { openNewChat, requestComposerFocus, switchToThread } from './ask-cmdr-trigger.svelte'

const log = getAppLogger('askCmdr')

/** How many conversations a list page holds (offset = `conversations.length`). */
export const SESSIONS_PAGE = 30

/** How many search hits a page holds. */
const SEARCH_LIMIT = 30

/** Debounce before a typed query hits the backend, so each keystroke doesn't search. */
const SEARCH_DEBOUNCE_MS = 200

interface SessionsState {
  /** Whether the sessions panel overlays the rail. */
  open: boolean
  /** The thread list (recent first), or the archived list when `showArchived`. */
  conversations: ConversationRow[]
  loading: boolean
  loadingMore: boolean
  /** True while a full page came back, so more may exist. */
  hasMore: boolean
  /** Show archived threads instead of active ones. */
  showArchived: boolean
  /** The live search query; empty shows the list, non-empty shows `hits`. */
  query: string
  hits: ConversationSearchHit[]
  searching: boolean
}

export const sessionsState = $state<SessionsState>({
  open: false,
  conversations: [],
  loading: false,
  loadingMore: false,
  hasMore: false,
  showArchived: false,
  query: '',
  hits: [],
  searching: false,
})

/** True when a query is active (the panel shows search hits, not the list). */
export function isSearching(): boolean {
  return sessionsState.query.trim().length > 0
}

// ── Open / close ───────────────────────────────────────────────────────────────

/** Open the sessions panel and load the first page. */
export function openSessions(): void {
  sessionsState.open = true
  void loadSessions()
}

/** Close the sessions panel. The loaded list stays for a quick reopen, but the search goes, so
 * the panel reopens on the recent threads rather than old hits. The panel's focused control
 * leaves the page with it, so focus goes to the prompt box, never `<body>`. */
export function closeSessions(): void {
  dismissSessions()
  requestComposerFocus()
}

/** Close the panel the way {@link closeSessions} does, minus the focus: the rail itself is going
 * away (`AskCmdrRail` calls this as it unmounts), and closing the rail puts focus on the pane. */
export function dismissSessions(): void {
  sessionsState.open = false
  clearSearch()
}

/** Escape in the panel steps back one layer, like the app's other search fields: a search
 * clears first, and an Escape with nothing typed closes the panel. */
export function escapeSessions(): void {
  if (sessionsState.query.length > 0) clearSearch()
  else closeSessions()
}

// ── Listing + paging ─────────────────────────────────────────────────────────

/** Reload the thread list from the top (recent first), honoring the archived filter. */
export async function loadSessions(): Promise<void> {
  sessionsState.loading = true
  try {
    const page = await listAskCmdrConversations(SESSIONS_PAGE, 0, sessionsState.showArchived)
    sessionsState.conversations = page
    sessionsState.hasMore = page.length === SESSIONS_PAGE
  } catch (e) {
    sessionsState.hasMore = false
    log.warn('loading the thread list failed: {error}', { error: e })
  } finally {
    sessionsState.loading = false
  }
}

/** Append the next page. Offset is `conversations.length` (single source of truth), so
 * pages tile with no overlap or desync. */
export async function loadMoreSessions(): Promise<void> {
  if (sessionsState.loadingMore || !sessionsState.hasMore) return
  sessionsState.loadingMore = true
  try {
    const page = await listAskCmdrConversations(
      SESSIONS_PAGE,
      sessionsState.conversations.length,
      sessionsState.showArchived,
    )
    sessionsState.conversations = [...sessionsState.conversations, ...page]
    sessionsState.hasMore = page.length === SESSIONS_PAGE
  } catch (e) {
    sessionsState.hasMore = false
    log.warn('loading more threads failed: {error}', { error: e })
  } finally {
    sessionsState.loadingMore = false
  }
}

/** Flip the active/archived filter and reload from the top. */
export function toggleShowArchived(): void {
  sessionsState.showArchived = !sessionsState.showArchived
  void loadSessions()
}

// ── Search ───────────────────────────────────────────────────────────────────

let searchTimer: ReturnType<typeof setTimeout> | null = null
/** Guards against an out-of-order search response overwriting a newer one. */
let searchSeq = 0
/** A broken search fails again on every typed query: logged once until a search works. */
const searchFailures = new LogOnceGate()

/** Set the query and (debounced) run the cross-thread search. An empty query restores
 * the list and drops any search still in flight. */
export function setSearchQuery(query: string): void {
  sessionsState.query = query
  if (searchTimer) clearTimeout(searchTimer)
  if (query.trim().length === 0) {
    searchSeq++ // an answer still in flight belongs to a query that's gone
    sessionsState.hits = []
    sessionsState.searching = false
    return
  }
  searchTimer = setTimeout(() => void runSearch(query), SEARCH_DEBOUNCE_MS)
}

async function runSearch(query: string): Promise<void> {
  const seq = ++searchSeq
  sessionsState.searching = true
  try {
    const hits = await searchAskCmdrConversations(query, SEARCH_LIMIT, 0)
    searchFailures.clear()
    if (seq !== searchSeq) return // a newer search superseded this one
    sessionsState.hits = hits
  } catch (e) {
    if (seq === searchSeq) sessionsState.hits = []
    const error = String(e)
    if (searchFailures.shouldLog(error)) log.warn('searching threads failed: {error}', { error })
  } finally {
    if (seq === searchSeq) sessionsState.searching = false
  }
}

/** Clear the query and restore the list. */
export function clearSearch(): void {
  setSearchQuery('')
}

// ── Actions ──────────────────────────────────────────────────────────────────

/** Switch the rail to a thread and close the panel. A thread that won't load leaves the panel
 * open on the list, so the user can see nothing moved and pick again. */
export async function chooseThread(id: number): Promise<void> {
  try {
    await switchToThread(id)
    closeSessions()
  } catch (e) {
    log.warn('switching threads failed: {error}', { error: e })
  }
}

/** Start a fresh chat and close the panel. */
export function startNewChat(): void {
  openNewChat()
  closeSessions()
}

// ── What the turn stream tells the list ──────────────────────────────────────

/**
 * A turn started in a thread the list doesn't know, so a thread was created since it loaded.
 * That is how a WAKE shows up: the agent opens its own thread and nothing else announces it
 * (`suggestions-changed` fires on proposals, not on thread creation).
 *
 * Only while the panel is open, because opening it reloads from the top anyway. A search is
 * left alone: the list isn't what's on screen then.
 */
export function noteThreadStarted(conversationId: number): void {
  if (!sessionsState.open || isSearching()) return
  if (sessionsState.conversations.some((row) => row.id === conversationId)) return
  void loadSessions()
}

/**
 * A thread the list may be showing is gone: a quiet wake takes its own thread away seconds
 * after opening it. Drop the row rather than leaving one that opens onto nothing.
 */
export function noteThreadDiscarded(conversationId: number): void {
  sessionsState.conversations = sessionsState.conversations.filter((row) => row.id !== conversationId)
  sessionsState.hits = sessionsState.hits.filter((hit) => hit.conversationId !== conversationId)
}

/** Rename a thread and reflect the new title in the list (and any matching search hit). A rename
 * that didn't land leaves the old title, which is what the list already shows. */
export async function renameConversation(id: number, title: string): Promise<void> {
  const trimmed = title.trim()
  if (trimmed.length === 0) return
  try {
    await renameAskCmdrConversation(id, trimmed)
  } catch (e) {
    log.warn('renaming the thread failed: {error}', { error: e })
    return
  }
  // A renamed wake thread shows the user's name now, not "Activity in …" (the backend drops the
  // place once the title stops being the wake's own; this mirrors it without a reload).
  const row = sessionsState.conversations.find((c) => c.id === id)
  if (row) {
    row.title = trimmed
    row.wakePlace = null
  }
  const hit = sessionsState.hits.find((h) => h.conversationId === id)
  if (hit) {
    hit.title = trimmed
    hit.wakePlace = null
  }
}

/** Archive or unarchive a thread. The archived view shows ALL threads (active + archived,
 * badged — the backend `include_archived` returns everything), so a flip there just
 * updates the badge in place. The active-only view excludes archived, so archiving a
 * thread there removes its row. */
export async function setArchived(id: number, archived: boolean): Promise<void> {
  try {
    await archiveAskCmdrConversation(id, archived)
  } catch (e) {
    log.warn('archiving the thread failed: {error}', { error: e })
    return
  }
  if (!sessionsState.showArchived && archived) {
    sessionsState.conversations = sessionsState.conversations.filter((c) => c.id !== id)
  } else {
    const row = sessionsState.conversations.find((c) => c.id === id)
    if (row) row.archived = archived
  }
}
