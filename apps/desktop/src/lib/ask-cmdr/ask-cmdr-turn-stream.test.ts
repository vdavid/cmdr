/**
 * The one transport, from the frontend's side: which thread an event belongs to, what happens
 * when the rail wasn't watching from the start, and what happens when the thread goes away.
 *
 * These are the three things a per-invoke reply channel could not do, so they are the tests
 * that would fail if anybody put one back.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { AskCmdrSendOutcome, AskCmdrStreamEvent, AskCmdrTurn, ConversationRow } from '$lib/tauri-commands'

const sendMock =
  vi.fn<(c: number | null, t: string, a: unknown[], d: string[]) => Promise<AskCmdrSendOutcome>>()
const listMock = vi.fn<(limit: number, offset: number, archived: boolean) => Promise<ConversationRow[]>>()
const unlistenMock = vi.fn()
const listenMock = vi.fn<(cb: (payload: AskCmdrTurn) => void) => Promise<() => void>>()

vi.mock('$lib/tauri-commands', () => ({
  sendAskCmdrMessage: (c: number | null, t: string, a: unknown[], d: string[]) => sendMock(c, t, a, d),
  onAskCmdrTurn: (cb: (payload: AskCmdrTurn) => void) => listenMock(cb),
  cancelAskCmdr: vi.fn(() => Promise.resolve()),
  listAskCmdrConversations: (l: number, o: number, a: boolean) => listMock(l, o, a),
  getAskCmdrConversation: vi.fn(() => Promise.resolve(null)),
  searchAskCmdrConversations: vi.fn(() => Promise.resolve([])),
  renameAskCmdrConversation: vi.fn(() => Promise.resolve()),
  archiveAskCmdrConversation: vi.fn(() => Promise.resolve()),
  recordAskCmdrModelChange: vi.fn(() => Promise.resolve(null)),
  preflightBulkRename: vi.fn(() => Promise.resolve({ status: 'ready', rows: [] })),
  cancelBulkRenameProposal: vi.fn(() => Promise.resolve()),
  applyBulkRename: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  reviseBulkRenameRow: vi.fn(() => Promise.resolve()),
}))
vi.mock('$lib/app-status-store', () => ({ saveAppStatus: vi.fn() }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))
vi.mock('$lib/file-explorer/pane/explorer-state.svelte', () => ({
  explorerState: { setRailFocused: vi.fn() },
}))
vi.mock('./rail-window', () => ({
  growMainWindowForRail: vi.fn(() => Promise.resolve()),
  shrinkMainWindowForRail: vi.fn(() => Promise.resolve()),
}))
vi.mock('./ask-cmdr-consent.svelte', () => ({
  consentState: { accepted: true, acceptedAt: null },
  refreshConsent: vi.fn(() => Promise.resolve()),
}))

import { askCmdrState, newChat, sendMessage, type RailMessage } from './ask-cmdr-trigger.svelte'
import { resetStoppedTurns } from './ask-cmdr-stream.svelte'
import { sessionsState } from './ask-cmdr-sessions.svelte'
import { routeTurnEvent, startAskCmdrTurnStream, stopAskCmdrTurnStream } from './ask-cmdr-turn-stream.svelte'

/** The thread the rail is reading in these tests. */
const OPEN_THREAD = 7
/** A thread the rail knows nothing about — a wake's, or another window's. */
const OTHER_THREAD = 99

function fire(conversationId: number, event: AskCmdrStreamEvent): void {
  routeTurnEvent({ conversationId, event })
}

function row(id: number): ConversationRow {
  return { id, title: `Chat ${String(id)}`, createdAt: 0, updatedAt: id, archived: false, origin: null }
}

function assistantAt(index: number): Extract<RailMessage, { kind: 'assistant' }> {
  const message = askCmdrState.messages[index]
  if (message.kind !== 'assistant') throw new Error('expected an assistant message')
  return message
}

beforeEach(() => {
  stopAskCmdrTurnStream()
  sendMock.mockReset()
  sendMock.mockImplementation((c) => Promise.resolve({ accepted: true, conversationId: c ?? OPEN_THREAD }))
  listMock.mockReset()
  listMock.mockResolvedValue([])
  unlistenMock.mockReset()
  listenMock.mockReset()
  listenMock.mockResolvedValue(unlistenMock)
  newChat()
  resetStoppedTurns()
  askCmdrState.messages = []
  askCmdrState.conversationId = null
  askCmdrState.streaming = false
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

describe('the main window subscription', () => {
  /** One listener for the whole window. A second `start` (a re-mount) would otherwise leave
   *  two, and every event would be applied twice. */
  it('subscribes once and routes what arrives', async () => {
    await startAskCmdrTurnStream()
    await startAskCmdrTurnStream()
    expect(listenMock).toHaveBeenCalledTimes(1)

    askCmdrState.conversationId = OPEN_THREAD
    listenMock.mock.calls[0][0]({ conversationId: OPEN_THREAD, event: { type: 'textDelta', text: 'live' } })
    expect(assistantAt(0).text).toBe('live')
  })

  it('stops listening on teardown, and can be started again', async () => {
    await startAskCmdrTurnStream()
    stopAskCmdrTurnStream()
    expect(unlistenMock).toHaveBeenCalledTimes(1)

    await startAskCmdrTurnStream()
    expect(listenMock).toHaveBeenCalledTimes(2)
  })
})

describe('which thread an event belongs to', () => {
  /** The whole reason the event carries a conversation id. A wake thinks in its own thread
   *  while the user reads another, and its answer must not appear in what they are reading. */
  it('drops a turn that belongs to another thread', () => {
    askCmdrState.conversationId = OPEN_THREAD

    fire(OTHER_THREAD, { type: 'assistantStarted' })
    fire(OTHER_THREAD, { type: 'textDelta', text: "someone else's answer" })

    expect(askCmdrState.messages).toEqual([])
    expect(askCmdrState.streaming).toBe(false)
  })

  it("adopts a new thread's id from its own started event", () => {
    sendMessage('first message in a fresh chat')
    expect(askCmdrState.conversationId).toBeNull()

    fire(OPEN_THREAD, { type: 'started' })

    expect(askCmdrState.conversationId).toBe(OPEN_THREAD)
  })
})

describe('picking a turn up part-way through', () => {
  /** ⚠️ The reload case, and the reason this is an event rather than a reply channel. The
   *  webview goes away mid-answer and the turn keeps running into the database; the reloaded
   *  rail lands on the same thread with `streaming` false and no bubble, and there is no
   *  second `assistantStarted` coming. Anything live has to be enough. */
  it('renders a turn that was already running when the rail started listening', () => {
    askCmdrState.conversationId = OPEN_THREAD

    fire(OPEN_THREAD, { type: 'textDelta', text: 'the second half ' })
    fire(OPEN_THREAD, { type: 'textDelta', text: 'of an answer' })

    expect(askCmdrState.streaming).toBe(true)
    expect(assistantAt(0).text).toBe('the second half of an answer')
    expect(assistantAt(0).streaming).toBe(true)

    fire(OPEN_THREAD, {
      type: 'done',
      messageId: 12,
      seq: 3,
      stop: 'completed',
      usage: { promptTokens: 1, completionTokens: 2 },
    })

    expect(askCmdrState.streaming).toBe(false)
    expect(assistantAt(0).id).toBe(12)
  })

  /** A cancel gets no terminal event back, so the backend keeps dribbling for a chunk or two.
   *  Without the stopped list those chunks would read as a live turn and put the rail back
   *  into "working…" with nothing coming to clear it. */
  it('a chunk arriving after the user stopped does not restart the turn', async () => {
    sendMessage('long one')
    fire(OPEN_THREAD, { type: 'started' })
    fire(OPEN_THREAD, { type: 'assistantStarted' })
    fire(OPEN_THREAD, { type: 'textDelta', text: 'partial' })
    const { stopStreaming } = await import('./ask-cmdr-trigger.svelte')
    stopStreaming()

    fire(OPEN_THREAD, { type: 'textDelta', text: ' and more' })

    expect(askCmdrState.streaming).toBe(false)
    expect(assistantAt(1).text).toBe('partial')
  })
})

describe('a thread that disappears mid-subscription', () => {
  /** A quiet wake opens a thread, thinks in it, and deletes it seconds later. Anyone reading
   *  that thread cannot recover by re-reading it — there is nothing left to read. */
  it('steps the rail off a thread a quiet wake took away', () => {
    askCmdrState.conversationId = OPEN_THREAD
    fire(OPEN_THREAD, { type: 'textDelta', text: 'looking…' })

    fire(OPEN_THREAD, { type: 'discarded' })

    expect(askCmdrState.conversationId).toBeNull()
    expect(askCmdrState.messages).toEqual([])
    expect(askCmdrState.streaming).toBe(false)
  })

  it('leaves a thread the rail is not showing alone', () => {
    askCmdrState.conversationId = OPEN_THREAD

    fire(OTHER_THREAD, { type: 'discarded' })

    expect(askCmdrState.conversationId).toBe(OPEN_THREAD)
  })
})

describe('what the session list hears', () => {
  /** Nothing else announces a thread the agent opened for itself: `suggestions-changed` fires
   *  on proposals, and a wake that only looks makes none. */
  it('reloads the open list when a turn starts in a thread it does not know', async () => {
    sessionsState.open = true
    sessionsState.conversations = [row(OPEN_THREAD)]
    listMock.mockResolvedValue([row(OTHER_THREAD), row(OPEN_THREAD)])

    fire(OTHER_THREAD, { type: 'started' })

    await vi.waitFor(() => {
      expect(sessionsState.conversations.map((c) => c.id)).toEqual([OTHER_THREAD, OPEN_THREAD])
    })
  })

  it('does not reload for a thread already in the list', () => {
    sessionsState.open = true
    sessionsState.conversations = [row(OPEN_THREAD)]

    fire(OPEN_THREAD, { type: 'started' })

    expect(listMock).not.toHaveBeenCalled()
  })

  /** Closed, the panel reloads from the top on its next open anyway, so a wake mid-flight
   *  costs it nothing. */
  it('stays quiet while the panel is closed', () => {
    sessionsState.open = false

    fire(OTHER_THREAD, { type: 'started' })

    expect(listMock).not.toHaveBeenCalled()
  })

  it('drops the row when a quiet wake takes its thread back', () => {
    sessionsState.open = true
    sessionsState.conversations = [row(OTHER_THREAD), row(OPEN_THREAD)]

    fire(OTHER_THREAD, { type: 'discarded' })

    expect(sessionsState.conversations.map((c) => c.id)).toEqual([OPEN_THREAD])
  })
})
