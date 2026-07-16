/**
 * The rail's streaming state machine: sending appends a user message and streams the
 * answer into the last assistant bubble; stop cancels and finalizes locally (the runtime
 * sends no terminal event on cancel); width persists; the soft-cap nudge trips past the
 * constant.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { AskCmdrStreamEvent } from '$lib/tauri-commands'

const sendMock =
  vi.fn<(c: number | null, t: string, a: unknown[], o: (e: AskCmdrStreamEvent) => void) => Promise<number>>()
const cancelMock = vi.fn<(id: number) => Promise<void>>()
const listMock = vi.fn<(...a: unknown[]) => Promise<unknown>>()
const getMock = vi.fn<(...a: unknown[]) => Promise<unknown>>()
const recordMock = vi.fn<(id: number) => Promise<unknown>>()
const saveMock = vi.fn()
const growWindowMock = vi.fn<(w: number) => Promise<void>>()
const shrinkWindowMock = vi.fn<(w: number) => Promise<void>>()

vi.mock('$lib/tauri-commands', () => ({
  sendAskCmdrMessage: (c: number | null, t: string, a: unknown[], o: (e: AskCmdrStreamEvent) => void) =>
    sendMock(c, t, a, o),
  cancelAskCmdr: (id: number) => cancelMock(id),
  listAskCmdrConversations: (...a: unknown[]) => listMock(...a),
  getAskCmdrConversation: (...a: unknown[]) => getMock(...a),
  recordAskCmdrModelChange: (id: number) => recordMock(id),
}))
vi.mock('$lib/app-status-store', () => ({
  saveAppStatus: (s: unknown) => {
    saveMock(s)
  },
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))
vi.mock('$lib/file-explorer/pane/explorer-state.svelte', () => ({
  explorerState: { setRailFocused: vi.fn() },
}))
vi.mock('./rail-window', () => ({
  growMainWindowForRail: (w: number) => growWindowMock(w),
  shrinkMainWindowForRail: (w: number) => shrinkWindowMock(w),
}))
// Consent is granted for these tests, so `openRail` proceeds past the gate to bootstrap.
vi.mock('./ask-cmdr-consent.svelte', () => ({
  consentState: { accepted: true, acceptedAt: null },
  refreshConsent: vi.fn(() => Promise.resolve()),
  acceptConsent: vi.fn(() => Promise.resolve(true)),
  revokeConsent: vi.fn(() => Promise.resolve()),
}))

import {
  addAttachments,
  askCmdrState,
  hasOlderMessages,
  hydrateRail,
  isOverSoftCap,
  loadOlderMessages,
  MESSAGE_PAGE,
  closeRail,
  newChat,
  noteModelSettingChanged,
  openRail,
  pathFromArguments,
  RAIL_MAX_WIDTH,
  RAIL_MIN_WIDTH,
  removeAttachment,
  sendMessage,
  setRailWidth,
  stopStreaming,
  switchToThread,
  THREAD_SOFT_CAP_MESSAGES,
  type RailMessage,
} from './ask-cmdr-trigger.svelte'

/** Capture the onEvent callback the last send handed us, to drive the stream by hand. */
let lastOnEvent: ((e: AskCmdrStreamEvent) => void) | null = null

/** Feed one stream event through the captured callback (fails loudly if no send is live). */
function fire(event: AskCmdrStreamEvent): void {
  if (!lastOnEvent) throw new Error('no active send to fire an event into')
  lastOnEvent(event)
}

beforeEach(() => {
  sendMock.mockReset()
  cancelMock.mockReset()
  saveMock.mockReset()
  listMock.mockReset()
  getMock.mockReset()
  recordMock.mockReset()
  growWindowMock.mockReset()
  shrinkWindowMock.mockReset()
  growWindowMock.mockResolvedValue()
  shrinkWindowMock.mockResolvedValue()
  listMock.mockResolvedValue([])
  sendMock.mockImplementation((c, _t, _a, o) => {
    lastOnEvent = o
    return Promise.resolve(c ?? 1)
  })
  newChat()
  askCmdrState.messages = []
  askCmdrState.conversationId = null
  askCmdrState.open = false
})

function conversationRow(id: number) {
  return { id, title: 't', createdAt: 0, updatedAt: 0, archived: false, origin: null }
}

function assistantAt(index: number): Extract<RailMessage, { kind: 'assistant' }> {
  const message = askCmdrState.messages[index]
  if (message.kind !== 'assistant') throw new Error('expected an assistant message')
  return message
}

describe('sendMessage + streaming', () => {
  it('appends a user message and streams the answer into an assistant bubble', () => {
    sendMessage('hello')
    expect(askCmdrState.messages[0]).toEqual({ kind: 'user', id: null, text: 'hello', attachments: [] })
    expect(askCmdrState.streaming).toBe(true)
    expect(sendMock).toHaveBeenCalledWith(null, 'hello', [], expect.any(Function))

    fire({ type: 'started', conversationId: 7 })
    fire({ type: 'assistantStarted' })
    fire({ type: 'textDelta', text: 'Hi ' })
    fire({ type: 'textDelta', text: 'there' })
    expect(assistantAt(1).text).toBe('Hi there')
    expect(assistantAt(1).streaming).toBe(true)

    fire({
      type: 'done',
      messageId: 42,
      seq: 2,
      stop: 'completed',
      usage: { promptTokens: 1, completionTokens: 2 },
    })
    expect(assistantAt(1).streaming).toBe(false)
    expect(assistantAt(1).id).toBe(42)
    expect(askCmdrState.conversationId).toBe(7)
    expect(askCmdrState.streaming).toBe(false)
  })

  it('renders tool call lines and their finished status', () => {
    sendMessage('what am I looking at?')
    fire({ type: 'assistantStarted' })
    fire({ type: 'toolCallStarted', callId: 'c1', tool: 'app_state' })
    expect(assistantAt(1).tools[0]).toMatchObject({ callId: 'c1', tool: 'app_state', running: true })
    fire({ type: 'toolCallFinished', callId: 'c1', ok: true })
    expect(assistantAt(1).tools[0]).toMatchObject({ running: false, ok: true })
  })

  it('a typed failure ends streaming and shows an honest notice', () => {
    sendMessage('hi')
    fire({ type: 'assistantStarted' })
    fire({ type: 'failed', kind: 'rateLimited', detail: null })
    expect(askCmdrState.streaming).toBe(false)
    // The empty assistant bubble is dropped; an error item takes its place.
    const last = askCmdrState.messages.at(-1)
    expect(last).toEqual({ kind: 'error', errorKind: 'rateLimited', detail: undefined })
  })

  it("a failure with provider detail keeps the provider's wording for display", () => {
    sendMessage('hi')
    fire({ type: 'assistantStarted' })
    fire({ type: 'failed', kind: 'provider', detail: 'HTTP 404: This model is unavailable for free.' })
    const last = askCmdrState.messages.at(-1)
    expect(last).toEqual({
      kind: 'error',
      errorKind: 'provider',
      detail: 'HTTP 404: This model is unavailable for free.',
    })
  })

  it('ignores a second send while one is streaming (single-flight)', () => {
    sendMessage('first')
    sendMessage('second')
    expect(sendMock).toHaveBeenCalledTimes(1)
  })

  it('a modelChanged stream event inserts a timeline line before the current user bubble', () => {
    sendMessage('hi')
    fire({ type: 'assistantStarted' })
    fire({ type: 'modelChanged', messageId: 9, seq: 0, model: 'model-two' })
    expect(askCmdrState.messages.map((m) => m.kind)).toEqual(['modelChange', 'user', 'assistant'])
    expect(askCmdrState.messages[0]).toEqual({ kind: 'modelChange', model: 'model-two' })
  })
})

describe('model settings changes', () => {
  it('records an event for the active thread and appends the line (debounced)', async () => {
    vi.useFakeTimers()
    try {
      askCmdrState.conversationId = 7
      recordMock.mockResolvedValue({
        id: 9,
        seq: 4,
        role: 'event',
        blocks: [{ type: 'modelChanged', model: 'model-two' }],
        promptTokens: null,
        completionTokens: null,
        createdAt: 0,
      })
      noteModelSettingChanged()
      noteModelSettingChanged() // rapid keystrokes collapse to one backend call
      await vi.advanceTimersByTimeAsync(1500)
      expect(recordMock).toHaveBeenCalledTimes(1)
      expect(recordMock).toHaveBeenCalledWith(7)
      expect(askCmdrState.messages.at(-1)).toEqual({ kind: 'modelChange', model: 'model-two' })
    } finally {
      vi.useRealTimers()
    }
  })

  it('does nothing without an active thread, or when the backend reports no change', async () => {
    vi.useFakeTimers()
    try {
      noteModelSettingChanged()
      await vi.advanceTimersByTimeAsync(1500)
      expect(recordMock).not.toHaveBeenCalled()

      askCmdrState.conversationId = 7
      recordMock.mockResolvedValue(null) // same effective model (e.g. masked by the override)
      noteModelSettingChanged()
      await vi.advanceTimersByTimeAsync(1500)
      expect(recordMock).toHaveBeenCalledTimes(1)
      expect(askCmdrState.messages).toEqual([])
    } finally {
      vi.useRealTimers()
    }
  })

  it('drops a recorded event that resolves after the user switched threads', async () => {
    vi.useFakeTimers()
    try {
      askCmdrState.conversationId = 7
      let resolveRecord: (v: unknown) => void = () => {}
      recordMock.mockImplementation(
        () =>
          new Promise((resolve) => {
            resolveRecord = resolve
          }),
      )
      noteModelSettingChanged()
      await vi.advanceTimersByTimeAsync(1500)
      askCmdrState.conversationId = 8 // switched threads while the backend waited
      resolveRecord({
        id: 9,
        seq: 4,
        role: 'event',
        blocks: [{ type: 'modelChanged', model: 'model-two' }],
        promptTokens: null,
        completionTokens: null,
        createdAt: 0,
      })
      await vi.advanceTimersByTimeAsync(0)
      expect(askCmdrState.messages).toEqual([])
    } finally {
      vi.useRealTimers()
    }
  })
})

describe('stopStreaming', () => {
  it('cancels the active turn and finalizes locally (no terminal event arrives)', () => {
    sendMessage('long one')
    fire({ type: 'started', conversationId: 3 })
    fire({ type: 'assistantStarted' })
    fire({ type: 'textDelta', text: 'partial' })
    stopStreaming()
    expect(cancelMock).toHaveBeenCalledWith(3)
    expect(askCmdrState.streaming).toBe(false)
    expect(assistantAt(1).streaming).toBe(false)
    expect(assistantAt(1).text).toBe('partial')
  })
})

describe('openRail bootstrap + newChat + hydrate', () => {
  it('opens, bootstraps the most recent thread, and folds tool results into their lines', async () => {
    listMock.mockResolvedValue([conversationRow(9)])
    getMock.mockResolvedValue({
      conversation: conversationRow(9),
      totalMessages: 3,
      messages: [
        {
          id: 1,
          seq: 0,
          role: 'user',
          blocks: [{ type: 'text', text: 'hi' }],
          promptTokens: null,
          completionTokens: null,
          createdAt: 0,
        },
        {
          id: 2,
          seq: 1,
          role: 'assistant',
          blocks: [
            { type: 'text', text: 'hello' },
            { type: 'toolCall', callId: 'c1', tool: 'list_dir', arguments: '{"path":"/x"}' },
          ],
          promptTokens: null,
          completionTokens: null,
          createdAt: 0,
        },
        {
          id: 3,
          seq: 2,
          role: 'tool',
          blocks: [{ type: 'toolResult', callId: 'c1', ok: true, elided: false }],
          promptTokens: null,
          completionTokens: null,
          createdAt: 0,
        },
      ],
    })
    await openRail()
    expect(askCmdrState.open).toBe(true)
    expect(askCmdrState.conversationId).toBe(9)
    expect(saveMock).toHaveBeenCalledWith({ askCmdrRailOpen: true })
    // The user + assistant items; the tool-result row folds into the assistant's tool line.
    expect(askCmdrState.messages).toHaveLength(2)
    const assistant = assistantAt(1)
    expect(assistant.text).toBe('hello')
    expect(assistant.tools[0]).toMatchObject({ callId: 'c1', tool: 'list_dir', ok: true, path: '/x' })
  })

  it('folds persisted event rows into model-change timeline lines', async () => {
    listMock.mockResolvedValue([conversationRow(9)])
    getMock.mockResolvedValue({
      conversation: conversationRow(9),
      totalMessages: 2,
      messages: [
        {
          id: 1,
          seq: 0,
          role: 'user',
          blocks: [{ type: 'text', text: 'hi' }],
          promptTokens: null,
          completionTokens: null,
          createdAt: 0,
        },
        {
          id: 2,
          seq: 1,
          role: 'event',
          blocks: [{ type: 'modelChanged', model: 'model-two' }],
          promptTokens: null,
          completionTokens: null,
          createdAt: 0,
        },
      ],
    })
    await openRail()
    expect(askCmdrState.messages).toEqual([
      { kind: 'user', id: 1, text: 'hi', attachments: [] },
      { kind: 'modelChange', model: 'model-two' },
    ])
  })

  it('opens cleanly when there is no prior thread', async () => {
    listMock.mockResolvedValue([])
    await openRail()
    expect(askCmdrState.open).toBe(true)
    expect(askCmdrState.conversationId).toBeNull()
    expect(askCmdrState.messages).toHaveLength(0)
  })

  it('newChat clears the active thread', () => {
    askCmdrState.conversationId = 5
    askCmdrState.messages.push({ kind: 'user', id: null, text: 'x', attachments: [] })
    newChat()
    expect(askCmdrState.conversationId).toBeNull()
    expect(askCmdrState.messages).toHaveLength(0)
  })

  it('hydrateRail applies a persisted width and opens when the flag is set', () => {
    hydrateRail(true, 420)
    expect(askCmdrState.width).toBe(420)
    expect(askCmdrState.open).toBe(true)
  })
})

describe('window growth wiring', () => {
  it('grows the main window by the current rail width on a real open', async () => {
    askCmdrState.width = 360
    await openRail()
    expect(growWindowMock).toHaveBeenCalledWith(360)
  })

  it('does not grow the window when the rail is already open', async () => {
    askCmdrState.open = true
    await openRail()
    expect(growWindowMock).not.toHaveBeenCalled()
  })

  it('does not grow the window on startup hydration (the window is already rail-inclusive)', () => {
    hydrateRail(true, 420)
    expect(growWindowMock).not.toHaveBeenCalled()
  })

  it('shrinks the window back when the rail closes', () => {
    askCmdrState.open = true
    askCmdrState.width = 300
    closeRail()
    expect(shrinkWindowMock).toHaveBeenCalledWith(300)
  })
})

describe('setRailWidth', () => {
  it('persists the width and clamps to the bounds', () => {
    setRailWidth(400)
    expect(askCmdrState.width).toBe(400)
    expect(saveMock).toHaveBeenCalledWith({ askCmdrRailWidth: 400 })
    setRailWidth(99999)
    expect(askCmdrState.width).toBe(RAIL_MAX_WIDTH)
    setRailWidth(10)
    expect(askCmdrState.width).toBe(RAIL_MIN_WIDTH)
  })
})

describe('isOverSoftCap', () => {
  it('trips only once the thread crosses the soft cap', () => {
    for (let i = 0; i <= THREAD_SOFT_CAP_MESSAGES; i++) {
      askCmdrState.messages.push({ kind: 'user', id: null, text: `m${String(i)}`, attachments: [] })
    }
    expect(askCmdrState.messages.length).toBe(THREAD_SOFT_CAP_MESSAGES + 1)
    expect(isOverSoftCap()).toBe(true)
  })
})

describe('pathFromArguments', () => {
  it('extracts a path field and tolerates malformed JSON', () => {
    expect(pathFromArguments('{"path":"/Users/x/Documents"}')).toBe('/Users/x/Documents')
    expect(pathFromArguments('{"limit":10}')).toBeNull()
    expect(pathFromArguments('not json')).toBeNull()
  })
})

/** A minimal user-role MessageView for paging fixtures. */
function userMessage(id: number, seq: number) {
  return {
    id,
    seq,
    role: 'user' as const,
    blocks: [{ type: 'text' as const, text: `m${String(seq)}` }],
    promptTokens: null,
    completionTokens: null,
    createdAt: 0,
  }
}

/** A detail page covering seqs [offset, offset+limit) out of `total`. */
function detailPage(total: number, limit: number, offset: number) {
  const upper = Math.min(offset + limit, total)
  const messages = []
  for (let seq = offset; seq < upper; seq++) messages.push(userMessage(seq + 1, seq))
  return { conversation: conversationRow(9), totalMessages: total, messages }
}

describe('message paging (tail-first, load older)', () => {
  it('loads the most recent page for a long thread and offers older', async () => {
    const total = MESSAGE_PAGE + 20
    getMock.mockImplementation((...args: unknown[]) => {
      const [, limit, offset] = args as [number, number, number]
      return Promise.resolve(detailPage(total, limit, offset))
    })
    await switchToThread(9)
    // A long thread refetches its tail: the last page, not the oldest.
    expect(getMock).toHaveBeenLastCalledWith(9, MESSAGE_PAGE, total - MESSAGE_PAGE)
    expect(askCmdrState.messages).toHaveLength(MESSAGE_PAGE)
    expect(askCmdrState.historyCount).toBe(MESSAGE_PAGE)
    expect(hasOlderMessages()).toBe(true)
    // The newest message shown is the last one (seq total-1).
    const last = askCmdrState.messages.at(-1)
    expect(last?.kind === 'user' && last.text).toBe(`m${String(total - 1)}`)
  })

  it('load-older prepends the previous page with a non-overlapping offset', async () => {
    const total = MESSAGE_PAGE + 20
    getMock.mockImplementation((...args: unknown[]) => {
      const [, limit, offset] = args as [number, number, number]
      return Promise.resolve(detailPage(total, limit, offset))
    })
    await switchToThread(9)
    await loadOlderMessages()
    // The older page starts at 0 and fills the gap up to the tail (20 messages).
    expect(getMock).toHaveBeenLastCalledWith(9, 20, 0)
    expect(askCmdrState.messages).toHaveLength(total)
    expect(askCmdrState.historyCount).toBe(total)
    expect(hasOlderMessages()).toBe(false)
    // Ordering holds: oldest at the top, newest at the bottom, no gaps or repeats.
    const seqs = askCmdrState.messages.map((m) => (m.kind === 'user' ? m.text : ''))
    expect(seqs).toEqual(Array.from({ length: total }, (_, i) => `m${String(i)}`))
  })

  it('a short thread loads in one page with nothing older', async () => {
    getMock.mockImplementation((...args: unknown[]) => {
      const [, limit, offset] = args as [number, number, number]
      return Promise.resolve(detailPage(3, limit, offset))
    })
    await switchToThread(9)
    expect(askCmdrState.messages).toHaveLength(3)
    expect(hasOlderMessages()).toBe(false)
  })
})

describe('attachments', () => {
  it('adds and de-duplicates by path, and removes by path', () => {
    addAttachments([
      { path: '/a', kind: 'file' },
      { path: '/b', kind: 'folder' },
    ])
    addAttachments([{ path: '/a', kind: 'file' }]) // duplicate ignored
    expect(askCmdrState.attachments.map((a) => a.path)).toEqual(['/a', '/b'])
    removeAttachment('/a')
    expect(askCmdrState.attachments.map((a) => a.path)).toEqual(['/b'])
  })

  it('send passes staged attachments, echoes them on the user bubble, then clears them', () => {
    addAttachments([{ path: '/a', kind: 'file' }])
    sendMessage('about this')
    expect(sendMock).toHaveBeenCalledWith(null, 'about this', [{ path: '/a', kind: 'file' }], expect.any(Function))
    const first = askCmdrState.messages[0]
    expect(first.kind === 'user' && first.attachments).toEqual([{ path: '/a', kind: 'file' }])
    // Staged attachments are cleared after the send.
    expect(askCmdrState.attachments).toEqual([])
  })
})

describe('switchToThread', () => {
  it('loads the chosen thread into the rail', async () => {
    getMock.mockResolvedValue(detailPage(2, MESSAGE_PAGE, 0))
    await switchToThread(9)
    expect(askCmdrState.conversationId).toBe(9)
    expect(askCmdrState.messages).toHaveLength(2)
  })
})
