/**
 * Behavior tests for `AskCmdrSessions.svelte`: the row/search/archive actions and the
 * inline-rename flow (enter edit, commit on Enter, cancel on Escape). The sessions state
 * slice is mocked so the component's own handlers are what's exercised.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import type { ConversationRow } from '$lib/tauri-commands'

const spies = vi.hoisted(() => ({
  chooseThread: vi.fn(),
  clearSearch: vi.fn(),
  closeSessions: vi.fn(),
  loadMoreSessions: vi.fn(),
  renameConversation: vi.fn(() => Promise.resolve()),
  setArchived: vi.fn(),
  setSearchQuery: vi.fn(),
  startNewChat: vi.fn(),
  toggleShowArchived: vi.fn(),
}))
const sessionsState = vi.hoisted(() => ({
  state: {
    open: true,
    conversations: [] as ConversationRow[],
    loading: false,
    loadingMore: false,
    hasMore: false,
    showArchived: false,
    query: '',
    hits: [] as unknown[],
    searching: false,
  },
}))

vi.mock('./ask-cmdr-sessions.svelte', () => ({
  sessionsState: sessionsState.state,
  isSearching: () => sessionsState.state.query.trim().length > 0,
  ...spies,
}))

import AskCmdrSessions from './AskCmdrSessions.svelte'

function row(id: number, archived = false): ConversationRow {
  return { id, title: `Chat ${String(id)}`, createdAt: 0, updatedAt: id, archived, origin: null }
}

function mountPanel(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AskCmdrSessions, { target, props: {} })
  return target
}

beforeEach(() => {
  document.body.innerHTML = ''
  Object.values(spies).forEach((s) => s.mockClear())
  Object.assign(sessionsState.state, {
    open: true,
    conversations: [row(1), row(2)],
    loading: false,
    loadingMore: false,
    hasMore: true,
    showArchived: false,
    query: '',
    hits: [],
    searching: false,
  })
})

describe('AskCmdrSessions interactions', () => {
  it('clicking a row switches to that thread', async () => {
    const target = mountPanel()
    await tick()
    target.querySelector<HTMLButtonElement>('.row')?.click()
    expect(spies.chooseThread).toHaveBeenCalledWith(1)
  })

  it('typing in the search box sets the query', async () => {
    const target = mountPanel()
    await tick()
    const input = target.querySelector<HTMLInputElement>('input[type="search"]')
    if (!input) throw new Error('expected a search input')
    input.value = 'budget'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    expect(spies.setSearchQuery).toHaveBeenCalledWith('budget')
  })

  it('the archived toggle flips the filter', async () => {
    const target = mountPanel()
    await tick()
    target.querySelector<HTMLButtonElement>('.archived-toggle')?.click()
    expect(spies.toggleShowArchived).toHaveBeenCalled()
  })

  it('load more requests the next page', async () => {
    const target = mountPanel()
    await tick()
    target.querySelector<HTMLButtonElement>('.load-more')?.click()
    expect(spies.loadMoreSessions).toHaveBeenCalled()
  })

  it('the archive action archives the row', async () => {
    const target = mountPanel()
    await tick()
    const archiveButton = target.querySelector<HTMLButtonElement>('[aria-label="Archive"]')
    archiveButton?.click()
    expect(spies.setArchived).toHaveBeenCalledWith(1, true)
  })

  it('inline rename commits the edited title on Enter', async () => {
    const target = mountPanel()
    await tick()
    target.querySelector<HTMLButtonElement>('[aria-label="Rename"]')?.click()
    await tick()
    const input = target.querySelector<HTMLInputElement>('input[type="text"]')
    if (!input) throw new Error('expected a rename input')
    input.value = 'Taxes 2024'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await tick()
    expect(spies.renameConversation).toHaveBeenCalledWith(1, 'Taxes 2024')
  })

  it('inline rename cancels on Escape without renaming', async () => {
    const target = mountPanel()
    await tick()
    target.querySelector<HTMLButtonElement>('[aria-label="Rename"]')?.click()
    await tick()
    const input = target.querySelector<HTMLInputElement>('input[type="text"]')
    if (!input) throw new Error('expected a rename input')
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await tick()
    expect(spies.renameConversation).not.toHaveBeenCalled()
    expect(target.querySelector('input[type="text"]')).toBeNull()
  })

  /** A thread nobody typed the first message in wears a glyph, and one the user started does
   *  not. Without the mark the two are indistinguishable in the list, which is the whole
   *  reason a wake's thread is allowed to appear there at all. */
  it('marks only the threads the agent opened for itself', async () => {
    sessionsState.state.conversations = [row(1), { ...row(2), origin: 'notification' }]
    const target = mountPanel()
    await tick()
    const marks = target.querySelectorAll('.conversation .started-by-agent')
    expect(marks).toHaveLength(1)
    const marked = target.querySelectorAll('.conversation')[1]
    expect(marked.querySelector('.started-by-agent')).not.toBeNull()
    expect(marked.querySelector('[aria-label="Ask Cmdr started this chat"]')).not.toBeNull()
  })

  /** The same mark in search results. A hit that lost it would read as a thread the user
   *  started and forgot, which is exactly the confusion the glyph exists to prevent. */
  it('marks an agent-opened thread in search results too', async () => {
    sessionsState.state.query = 'kumquat'
    sessionsState.state.hits = [
      { conversationId: 1, title: 'Mine', updatedAt: 0, snippet: 'kumquat', origin: null },
      { conversationId: 2, title: 'Theirs', updatedAt: 0, snippet: 'kumquat', origin: 'notification' },
    ]
    const target = mountPanel()
    await tick()
    const rows = target.querySelectorAll('.row')
    expect(rows[0].querySelector('.started-by-agent')).toBeNull()
    expect(rows[1].querySelector('.started-by-agent')).not.toBeNull()
  })

  /** ❌ Not "origin is not null": the reserved quiet-wakes ledger row carries an origin too,
   *  and so will whatever token comes next. The mark belongs to the wake thread alone. */
  it('leaves a non-wake origin unmarked', async () => {
    sessionsState.state.conversations = [{ ...row(1), origin: 'quietWakes' }]
    const target = mountPanel()
    await tick()
    expect(target.querySelector('.started-by-agent')).toBeNull()
  })

  it('the back button closes the panel', async () => {
    const target = mountPanel()
    await tick()
    target.querySelector<HTMLButtonElement>('[aria-label="Back to chat"]')?.click()
    expect(spies.closeSessions).toHaveBeenCalled()
  })
})
