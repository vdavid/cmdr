/**
 * Tier 3 a11y tests for `AskCmdrSessions.svelte`: the past-chats panel (list, search box,
 * archived toggle, rename/archive row actions). State is seeded directly into
 * `sessionsState`; the IPC layer and the trigger's side-effectful deps are mocked.
 */

import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { ConversationRow, ConversationSearchHit } from '$lib/tauri-commands'

vi.mock('$lib/tauri-commands', () => ({
  listAskCmdrConversations: vi.fn(() => Promise.resolve([])),
  searchAskCmdrConversations: vi.fn(() => Promise.resolve([])),
  renameAskCmdrConversation: vi.fn(() => Promise.resolve()),
  archiveAskCmdrConversation: vi.fn(() => Promise.resolve()),
}))
vi.mock('$lib/app-status-store', () => ({ saveAppStatus: vi.fn() }))
vi.mock('$lib/file-explorer/pane/explorer-state.svelte', () => ({
  explorerState: { setRailFocused: vi.fn() },
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import AskCmdrSessions from './AskCmdrSessions.svelte'
import { sessionsState } from './ask-cmdr-sessions.svelte'

function row(id: number, archived = false): ConversationRow {
  return { id, title: `Chat ${String(id)}`, createdAt: 0, updatedAt: id, archived, origin: null }
}

function hit(id: number): ConversationSearchHit {
  return { conversationId: id, title: `Chat ${String(id)}`, updatedAt: id, snippet: 'a matching snippet' }
}

function mountPanel(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AskCmdrSessions, { target, props: {} })
  return target
}

beforeEach(() => {
  document.body.innerHTML = ''
  Object.assign(sessionsState, {
    open: true,
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

describe('AskCmdrSessions a11y', () => {
  it('an empty list has no a11y violations', async () => {
    const target = mountPanel()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('a populated list with row actions has no a11y violations', async () => {
    sessionsState.conversations = [row(1), row(2, true)]
    const target = mountPanel()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('search results have no a11y violations', async () => {
    sessionsState.query = 'budget'
    sessionsState.hits = [hit(1), hit(2)]
    const target = mountPanel()
    await tick()
    await expectNoA11yViolations(target)
  })
})
