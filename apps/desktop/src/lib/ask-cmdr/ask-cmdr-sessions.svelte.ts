/**
 * Ask Cmdr sessions state: the thread list (recent first, archived filter), cross-thread
 * FTS5 search, new/rename/archive, and switch-thread. A separate `$state` slice from the
 * live-chat trigger (`ask-cmdr-trigger.svelte.ts`), which it calls into to switch threads.
 *
 * Paging mirrors the operation-log dialog: the offset is `conversations.length` (one
 * source of truth), so an append can't overlap or desync from what's shown. Search
 * replaces the list while a query is active; clearing it restores the list.
 */

import { getAppLogger } from '$lib/logging/logger'
import {
  archiveAskCmdrConversation,
  listAskCmdrConversations,
  renameAskCmdrConversation,
  searchAskCmdrConversations,
  type ConversationRow,
  type ConversationSearchHit,
} from '$lib/tauri-commands'
import { newChat, switchToThread } from './ask-cmdr-trigger.svelte'

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

/** Close the sessions panel (leaves the loaded list in place for a quick reopen). */
export function closeSessions(): void {
  sessionsState.open = false
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
    log.warn('loading the thread list failed: {error}', { error: String(e) })
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
    log.warn('loading more threads failed: {error}', { error: String(e) })
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

/** Set the query and (debounced) run the cross-thread search. An empty query restores
 * the list. */
export function setSearchQuery(query: string): void {
  sessionsState.query = query
  if (searchTimer) clearTimeout(searchTimer)
  if (query.trim().length === 0) {
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
    if (seq !== searchSeq) return // a newer search superseded this one
    sessionsState.hits = hits
  } catch (e) {
    if (seq === searchSeq) sessionsState.hits = []
    log.warn('searching threads failed: {error}', { error: String(e) })
  } finally {
    if (seq === searchSeq) sessionsState.searching = false
  }
}

/** Clear the query and restore the list. */
export function clearSearch(): void {
  setSearchQuery('')
}

// ── Actions ──────────────────────────────────────────────────────────────────

/** Switch the rail to a thread and close the panel. */
export async function chooseThread(id: number): Promise<void> {
  await switchToThread(id)
  closeSessions()
}

/** Start a fresh chat and close the panel. */
export function startNewChat(): void {
  newChat()
  closeSessions()
}

/** Rename a thread and reflect the new title in the list (and any matching search hit). */
export async function renameConversation(id: number, title: string): Promise<void> {
  const trimmed = title.trim()
  if (trimmed.length === 0) return
  await renameAskCmdrConversation(id, trimmed)
  const row = sessionsState.conversations.find((c) => c.id === id)
  if (row) row.title = trimmed
  const hit = sessionsState.hits.find((h) => h.conversationId === id)
  if (hit) hit.title = trimmed
}

/** Archive or unarchive a thread. The archived view shows ALL threads (active + archived,
 * badged — the backend `include_archived` returns everything), so a flip there just
 * updates the badge in place. The active-only view excludes archived, so archiving a
 * thread there removes its row. */
export async function setArchived(id: number, archived: boolean): Promise<void> {
  await archiveAskCmdrConversation(id, archived)
  if (!sessionsState.showArchived && archived) {
    sessionsState.conversations = sessionsState.conversations.filter((c) => c.id !== id)
  } else {
    const row = sessionsState.conversations.find((c) => c.id === id)
    if (row) row.archived = archived
  }
}
