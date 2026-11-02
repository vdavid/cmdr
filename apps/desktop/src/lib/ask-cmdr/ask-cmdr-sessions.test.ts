/**
 * The sessions state slice: list paging (offset = list length, no overlap/desync), the
 * archived filter, cross-thread search (debounce + out-of-order supersede), and
 * rename/archive reflected in the list.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { ConversationRow, ConversationSearchHit } from '#lib/tauri-commands/index.js'

const listMock =
  vi.fn<(payload: { limit: number; offset: number; includeArchived: boolean }) => Promise<ConversationRow[]>>()
const searchMock =
  vi.fn<(payload: { query: string; limit: number; offset: number }) => Promise<ConversationSearchHit[]>>()
const renameMock = vi.fn<(id: number, title: string) => Promise<void>>()
const archiveMock = vi.fn<(id: number, archived: boolean) => Promise<void>>()
const switchMock = vi.fn<(id: number) => Promise<void>>()
const newChatMock = vi.fn()

vi.mock('#lib/tauri-commands/index.js', () => ({
  listAskCmdrConversations: (l: number, o: number, a: boolean) => listMock({ limit: l, offset: o, includeArchived: a }),
  searchAskCmdrConversations: (q: string, l: number, o: number) => searchMock({ query: q, limit: l, offset: o }),
  renameAskCmdrConversation: (id: number, t: string) => renameMock(id, t),
  archiveAskCmdrConversation: (id: number, a: boolean) => archiveMock(id, a),
}))
const { warn } = vi.hoisted(() => ({ warn: vi.fn() }))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn, info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))
vi.mock('./ask-cmdr-trigger.svelte', () => ({
  switchToThread: (id: number) => switchMock(id),
  openNewChat: () => {
    newChatMock()
  },
  requestComposerFocus: vi.fn(),
}))

import {
  SESSIONS_PAGE,
  chooseThread,
  clearSearch,
  closeSessions,
  loadMoreSessions,
  loadSessions,
  openSessions,
  renameConversation,
  sessionsState,
  setArchived,
  setSearchQuery,
  toggleShowArchived,
} from './ask-cmdr-sessions.svelte'

function row(id: number, archived = false): ConversationRow {
  return { id, title: `Chat ${String(id)}`, createdAt: 0, updatedAt: id, archived, origin: null }
}

function hit(id: number): ConversationSearchHit {
  return { conversationId: id, title: `Chat ${String(id)}`, updatedAt: id, snippet: 's', origin: null }
}

function fullPage(): ConversationRow[] {
  return Array.from({ length: SESSIONS_PAGE }, (_, i) => row(i + 1))
}

beforeEach(() => {
  listMock.mockReset()
  searchMock.mockReset()
  renameMock.mockReset()
  archiveMock.mockReset()
  switchMock.mockReset()
  renameMock.mockResolvedValue()
  archiveMock.mockResolvedValue()
  Object.assign(sessionsState, {
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
})

describe('sessions listing + paging', () => {
  it('loads the first page and sets hasMore only when the page is full', async () => {
    listMock.mockResolvedValueOnce(fullPage())
    await loadSessions()
    expect(listMock).toHaveBeenCalledWith({ limit: SESSIONS_PAGE, offset: 0, includeArchived: false })
    expect(sessionsState.conversations).toHaveLength(SESSIONS_PAGE)
    expect(sessionsState.hasMore).toBe(true)
  })

  it('a short first page means no more', async () => {
    listMock.mockResolvedValueOnce([row(1), row(2)])
    await loadSessions()
    expect(sessionsState.hasMore).toBe(false)
  })

  it('pages tile with no overlap: offset is the current list length', async () => {
    listMock.mockResolvedValueOnce(fullPage())
    await loadSessions()
    const second = [row(SESSIONS_PAGE + 1), row(SESSIONS_PAGE + 2)]
    listMock.mockResolvedValueOnce(second)
    await loadMoreSessions()
    expect(listMock).toHaveBeenLastCalledWith({ limit: SESSIONS_PAGE, offset: SESSIONS_PAGE, includeArchived: false })
    expect(sessionsState.conversations).toHaveLength(SESSIONS_PAGE + 2)
    // No id repeats — pages don't overlap.
    const ids = sessionsState.conversations.map((c) => c.id)
    expect(new Set(ids).size).toBe(ids.length)
    expect(sessionsState.hasMore).toBe(false)
  })

  it('does not load more when hasMore is false', async () => {
    sessionsState.hasMore = false
    await loadMoreSessions()
    expect(listMock).not.toHaveBeenCalled()
  })

  it('toggling the archived filter reloads from the top with the flag', async () => {
    listMock.mockResolvedValue([])
    toggleShowArchived()
    await Promise.resolve()
    await Promise.resolve()
    expect(sessionsState.showArchived).toBe(true)
    expect(listMock).toHaveBeenLastCalledWith({ limit: SESSIONS_PAGE, offset: 0, includeArchived: true })
  })
})

describe('sessions search', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('debounces, then shows hits for a non-empty query', async () => {
    searchMock.mockResolvedValueOnce([hit(1)])
    setSearchQuery('budget')
    expect(searchMock).not.toHaveBeenCalled() // debounced
    await vi.runAllTimersAsync()
    expect(searchMock).toHaveBeenCalledWith({ query: 'budget', limit: expect.any(Number) as number, offset: 0 })
    expect(sessionsState.hits).toEqual([hit(1)])
  })

  it('an empty query clears hits without searching', async () => {
    sessionsState.hits = [hit(1)]
    setSearchQuery('   ')
    await vi.runAllTimersAsync()
    expect(searchMock).not.toHaveBeenCalled()
    expect(sessionsState.hits).toEqual([])
  })

  it('a stale response never overwrites a newer one', async () => {
    let resolveFirst: (v: ConversationSearchHit[]) => void = () => {}
    searchMock.mockReturnValueOnce(new Promise((r) => (resolveFirst = r)))
    searchMock.mockResolvedValueOnce([hit(2)])

    setSearchQuery('a')
    await vi.advanceTimersByTimeAsync(250)
    setSearchQuery('ab')
    await vi.advanceTimersByTimeAsync(250)
    // The newer search resolved to hit(2); now the older one resolves late.
    resolveFirst([hit(1)])
    await vi.runAllTimersAsync()
    expect(sessionsState.hits).toEqual([hit(2)])
  })

  it('logs a broken search once while the user keeps typing, and again after a search works', async () => {
    warn.mockClear()
    searchMock.mockRejectedValue(new Error('index unavailable'))
    for (const query of ['b', 'bu', 'bud']) {
      setSearchQuery(query)
      await vi.runAllTimersAsync()
    }
    expect(warn).toHaveBeenCalledOnce()

    searchMock.mockResolvedValueOnce([hit(1)])
    setSearchQuery('budg')
    await vi.runAllTimersAsync()
    setSearchQuery('budge')
    await vi.runAllTimersAsync()
    expect(warn).toHaveBeenCalledTimes(2)
  })

  it('clearSearch restores the list view', async () => {
    sessionsState.query = 'x'
    sessionsState.hits = [hit(1)]
    clearSearch()
    await vi.runAllTimersAsync()
    expect(sessionsState.query).toBe('')
    expect(sessionsState.hits).toEqual([])
  })

  it('reopening the panel after a search shows the recent threads with an empty box', async () => {
    searchMock.mockResolvedValueOnce([hit(1)])
    listMock.mockResolvedValue([row(1), row(2)])
    openSessions()
    setSearchQuery('budget')
    await vi.runAllTimersAsync()
    expect(sessionsState.hits).toEqual([hit(1)])

    closeSessions()
    openSessions()
    await vi.runAllTimersAsync()

    expect(sessionsState.query).toBe('')
    expect(sessionsState.hits).toEqual([])
    expect(sessionsState.conversations.map((c) => c.id)).toEqual([1, 2])
  })

  it('closing the panel drops a search that was still waiting out its debounce', async () => {
    setSearchQuery('budget')
    closeSessions()
    await vi.runAllTimersAsync()
    expect(searchMock).not.toHaveBeenCalled()
  })

  it('a search still in flight when the panel closes lands nowhere', async () => {
    let resolveSearch: (v: ConversationSearchHit[]) => void = () => {}
    searchMock.mockReturnValueOnce(new Promise((r) => (resolveSearch = r)))
    setSearchQuery('budget')
    await vi.advanceTimersByTimeAsync(250)
    expect(sessionsState.searching).toBe(true)

    closeSessions()
    resolveSearch([hit(1)])
    await vi.runAllTimersAsync()

    expect(sessionsState.hits).toEqual([])
    expect(sessionsState.searching).toBe(false)
  })
})

describe('sessions rename + archive', () => {
  it('rename round-trips and updates the row title', async () => {
    sessionsState.conversations = [row(1), row(2)]
    await renameConversation(1, '  New title  ')
    expect(renameMock).toHaveBeenCalledWith(1, 'New title')
    expect(sessionsState.conversations.find((c) => c.id === 1)?.title).toBe('New title')
  })

  it('an empty rename is a no-op', async () => {
    sessionsState.conversations = [row(1)]
    await renameConversation(1, '   ')
    expect(renameMock).not.toHaveBeenCalled()
  })

  it('archiving from the active list removes the row', async () => {
    sessionsState.showArchived = false
    sessionsState.conversations = [row(1), row(2)]
    await setArchived(1, true)
    expect(archiveMock).toHaveBeenCalledWith(1, true)
    expect(sessionsState.conversations.map((c) => c.id)).toEqual([2])
  })

  it('unarchiving in the archived (all) view keeps the row and clears its flag', async () => {
    sessionsState.showArchived = true
    sessionsState.conversations = [row(1, true), row(2, true)]
    await setArchived(1, false)
    expect(sessionsState.conversations.map((c) => c.id)).toEqual([1, 2])
    expect(sessionsState.conversations.find((c) => c.id === 1)?.archived).toBe(false)
  })
})

/** A hiccup in Cmdr's own database used to escape as an unhandled rejection, which logs at error
 * and auto-sends a report, and a thread rename's failure went unlogged altogether. */
describe('a hiccup in the panel’s own database', () => {
  beforeEach(() => {
    warn.mockClear()
  })

  it('keeps the panel open and logs at warn when a thread won’t load', async () => {
    sessionsState.open = true
    switchMock.mockRejectedValueOnce(new Error('locked'))

    await chooseThread(7)

    expect(sessionsState.open).toBe(true)
    expect(warn).toHaveBeenCalledTimes(1)
  })

  it('leaves the row as it was and logs when archiving doesn’t land', async () => {
    sessionsState.conversations = [row(7)]
    archiveMock.mockRejectedValueOnce(new Error('locked'))

    await setArchived(7, true)

    expect(sessionsState.conversations.map((c) => c.id)).toEqual([7])
    expect(sessionsState.conversations[0].archived).toBe(false)
    expect(warn).toHaveBeenCalledTimes(1)
  })

  it('keeps the old title and logs when a rename doesn’t land', async () => {
    sessionsState.conversations = [row(7)]
    renameMock.mockRejectedValueOnce(new Error('locked'))

    await renameConversation(7, 'New')

    expect(sessionsState.conversations[0].title).toBe('Chat 7')
    expect(warn).toHaveBeenCalledTimes(1)
  })
})
