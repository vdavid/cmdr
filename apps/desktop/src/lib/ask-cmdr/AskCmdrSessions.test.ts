/**
 * Behavior tests for `AskCmdrSessions.svelte`: the row/search/archive actions and the
 * inline-rename flow (enter edit, commit on Enter, cancel on Escape). The sessions state
 * slice is mocked so the component's own handlers are what's exercised.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import type { ConversationRow, ConversationSearchHit } from '#lib/tauri-commands/index.js'

const spies = vi.hoisted(() => ({
  chooseThread: vi.fn(),
  clearSearch: vi.fn(),
  closeSessions: vi.fn(),
  escapeSessions: vi.fn(),
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
  return { id, title: `Chat ${String(id)}`, createdAt: 0, updatedAt: id, archived, origin: null, wakePlace: null }
}

function hit(id: number, overrides: Partial<ConversationSearchHit> = {}): ConversationSearchHit {
  return {
    conversationId: id,
    title: `Chat ${String(id)}`,
    updatedAt: 1_800_000_000,
    archived: false,
    snippet: [
      { text: 'about ', matched: false },
      { text: 'kumquat', matched: true },
      { text: ' trees', matched: false },
    ],
    origin: null,
    wakePlace: null,
    ...overrides,
  }
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
    expect(marked.querySelector('[aria-label="Cmdr started this chat"]')).not.toBeNull()
  })

  /** The same mark in search results. A hit that lost it would read as a thread the user
   *  started and forgot, which is exactly the confusion the glyph exists to prevent. */
  it('marks an agent-opened thread in search results too', async () => {
    sessionsState.state.query = 'kumquat'
    sessionsState.state.hits = [hit(1, { title: 'Mine' }), hit(2, { title: 'Theirs', origin: 'notification' })]
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

  it('Escape in the search box steps back through the panel', async () => {
    const target = mountPanel()
    await tick()
    const input = target.querySelector<HTMLInputElement>('input[type="search"]')
    if (!input) throw new Error('expected a search input')
    const escape = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
    input.dispatchEvent(escape)
    expect(spies.escapeSessions).toHaveBeenCalledOnce()
    // Claimed, so the window's own Escape (leaving full screen) doesn't also fire.
    expect(escape.defaultPrevented).toBe(true)
  })

  it('Escape on a row steps back too', async () => {
    const target = mountPanel()
    await tick()
    target
      .querySelector<HTMLButtonElement>('.row')
      ?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
    expect(spies.escapeSessions).toHaveBeenCalledOnce()
  })

  it('Escape in the rename field only cancels the rename', async () => {
    const target = mountPanel()
    await tick()
    target.querySelector<HTMLButtonElement>('[aria-label="Rename"]')?.click()
    await tick()
    target
      .querySelector<HTMLInputElement>('input[type="text"]')
      ?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
    expect(spies.escapeSessions).not.toHaveBeenCalled()
  })

  it('the back button closes the panel', async () => {
    const target = mountPanel()
    await tick()
    target.querySelector<HTMLButtonElement>('[aria-label="Back to chat"]')?.click()
    expect(spies.closeSessions).toHaveBeenCalled()
  })

  it('a wake thread the user never renamed is titled by where its activity was', async () => {
    sessionsState.state.conversations = [
      { ...row(1), title: 'tmp', origin: 'notification', wakePlace: { folder: '~/tmp', otherFolders: 2 } },
    ]
    const target = mountPanel()
    await tick()
    expect(target.querySelector('.row-title')?.textContent).toBe('Activity in ~/tmp and 2 other folders')
  })

  it('renaming a thread opened on its display title, unchanged, renames nothing', async () => {
    sessionsState.state.conversations = [
      { ...row(1), title: 'tmp', origin: 'notification', wakePlace: { folder: '~/tmp', otherFolders: 0 } },
    ]
    const target = mountPanel()
    await tick()
    target.querySelector<HTMLButtonElement>('[aria-label="Rename"]')?.click()
    await tick()
    const field = target.querySelector<HTMLInputElement>('input[type="text"]')
    expect(field?.value).toBe('Activity in ~/tmp')
    field?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))
    await tick()
    expect(spies.renameConversation).not.toHaveBeenCalled()
  })
})

describe('AskCmdrSessions search results', () => {
  beforeEach(() => {
    Element.prototype.scrollIntoView = vi.fn()
    sessionsState.state.query = 'kumquat'
    sessionsState.state.hits = [hit(1), hit(2), hit(3)]
  })

  function searchField(target: HTMLElement): HTMLInputElement {
    const input = target.querySelector<HTMLInputElement>('input[type="search"]')
    if (!input) throw new Error('expected a search input')
    return input
  }

  function press(el: Element, key: string): KeyboardEvent {
    const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true })
    el.dispatchEvent(event)
    return event
  }

  function cursorRow(target: HTMLElement): string | null {
    return target.querySelector('[role="option"][aria-selected="true"]')?.getAttribute('data-conversation-hit') ?? null
  }

  it('shows each hit’s title, age, and excerpt with the match marked as plain text', async () => {
    sessionsState.state.hits = [
      hit(1, {
        snippet: [
          { text: '<img src=x onerror=alert(1)> ', matched: false },
          { text: 'kumquat', matched: true },
        ],
      }),
    ]
    const target = mountPanel()
    await tick()
    const option = target.querySelector('[role="option"]')
    expect(option?.querySelector('.row-title')?.textContent).toBe('Chat 1')
    expect(option?.querySelector('.hit-age')?.textContent).toMatch(/ago|yesterday|last|now/)
    expect(option?.querySelector('mark')?.textContent).toBe('kumquat')
    expect(option?.querySelector('img')).toBeNull()
    expect(option?.querySelector('.hit-snippet')?.textContent).toBe('<img src=x onerror=alert(1)> kumquat')
  })

  it('is a combobox over a listbox, the cursor on the best match', async () => {
    const target = mountPanel()
    await tick()
    const input = searchField(target)
    const listbox = target.querySelector('[role="listbox"]')
    expect(input.getAttribute('role')).toBe('combobox')
    expect(input.getAttribute('aria-controls')).toBe(listbox?.id)
    expect(input.getAttribute('aria-expanded')).toBe('true')
    expect(cursorRow(target)).toBe('1')
    expect(input.getAttribute('aria-activedescendant')).toBe(
      target.querySelector('[role="option"][aria-selected="true"]')?.id,
    )
  })

  it('↑/↓ in the field move the cursor, wrapping at the ends, and keep focus in the field', async () => {
    const target = mountPanel()
    await tick()
    const input = searchField(target)
    input.focus()
    expect(press(input, 'ArrowDown').defaultPrevented).toBe(true)
    await tick()
    expect(cursorRow(target)).toBe('2')
    press(input, 'ArrowDown')
    press(input, 'ArrowDown')
    await tick()
    expect(cursorRow(target)).toBe('1')
    press(input, 'ArrowUp')
    await tick()
    expect(cursorRow(target)).toBe('3')
    expect(document.activeElement).toBe(input)
  })

  it('Enter in the field opens the cursor’s thread', async () => {
    const target = mountPanel()
    await tick()
    const input = searchField(target)
    press(input, 'ArrowDown')
    await tick()
    expect(press(input, 'Enter').defaultPrevented).toBe(true)
    expect(spies.chooseThread).toHaveBeenCalledWith(2)
  })

  it('Home and End stay the field’s caret keys', async () => {
    const target = mountPanel()
    await tick()
    expect(press(searchField(target), 'End').defaultPrevented).toBe(false)
  })

  it('a focused result answers the same keys, and a click opens it', async () => {
    const target = mountPanel()
    await tick()
    const first = target.querySelector<HTMLElement>('[role="option"]')
    if (!first) throw new Error('expected an option')
    press(first, 'ArrowDown')
    await tick()
    await tick()
    expect(cursorRow(target)).toBe('2')
    expect(document.activeElement?.getAttribute('data-conversation-hit')).toBe('2')
    press(document.activeElement as Element, 'Enter')
    expect(spies.chooseThread).toHaveBeenCalledWith(2)
    target.querySelectorAll<HTMLElement>('[role="option"]').item(2).click()
    expect(spies.chooseThread).toHaveBeenCalledWith(3)
  })

  it('the clear button says what it does', async () => {
    const target = mountPanel()
    await tick()
    target.querySelector<HTMLButtonElement>('[aria-label="Clear search"]')?.click()
    expect(spies.clearSearch).toHaveBeenCalledOnce()
  })
})
