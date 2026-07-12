/**
 * The rail's streaming state machine: sending appends a user message and streams the
 * answer into the last assistant bubble; stop cancels and finalizes locally (the runtime
 * sends no terminal event on cancel); width persists; the soft-cap nudge trips past the
 * constant.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { AskCmdrStreamEvent } from '$lib/tauri-commands'

const sendMock = vi.fn<(c: number | null, t: string, o: (e: AskCmdrStreamEvent) => void) => Promise<number>>()
const cancelMock = vi.fn<(id: number) => Promise<void>>()
const listMock = vi.fn()
const getMock = vi.fn()
const saveMock = vi.fn()

vi.mock('$lib/tauri-commands', () => ({
  sendAskCmdrMessage: (c: number | null, t: string, o: (e: AskCmdrStreamEvent) => void) => sendMock(c, t, o),
  cancelAskCmdr: (id: number) => cancelMock(id),
  listAskCmdrConversations: (...a: unknown[]) => listMock(...a),
  getAskCmdrConversation: (...a: unknown[]) => getMock(...a),
}))
vi.mock('$lib/app-status-store', () => ({ saveAppStatus: (s: unknown) => saveMock(s) }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))
vi.mock('$lib/file-explorer/pane/explorer-state.svelte', () => ({
  explorerState: { setRailFocused: vi.fn() },
}))

import {
  askCmdrState,
  isOverSoftCap,
  newChat,
  pathFromArguments,
  RAIL_MAX_WIDTH,
  sendMessage,
  setRailWidth,
  stopStreaming,
  THREAD_SOFT_CAP_MESSAGES,
  type RailMessage,
} from './ask-cmdr-trigger.svelte'

/** Capture the onEvent callback the last send handed us, to drive the stream by hand. */
let lastOnEvent: ((e: AskCmdrStreamEvent) => void) | null = null

beforeEach(() => {
  sendMock.mockReset()
  cancelMock.mockReset()
  saveMock.mockReset()
  sendMock.mockImplementation((c, _t, o) => {
    lastOnEvent = o
    return Promise.resolve(c ?? 1)
  })
  newChat()
  askCmdrState.messages = []
  askCmdrState.conversationId = null
})

function assistantAt(index: number): Extract<RailMessage, { kind: 'assistant' }> {
  const message = askCmdrState.messages[index]
  if (message.kind !== 'assistant') throw new Error('expected an assistant message')
  return message
}

describe('sendMessage + streaming', () => {
  it('appends a user message and streams the answer into an assistant bubble', () => {
    sendMessage('hello')
    expect(askCmdrState.messages[0]).toEqual({ kind: 'user', id: null, text: 'hello' })
    expect(askCmdrState.streaming).toBe(true)
    expect(sendMock).toHaveBeenCalledWith(null, 'hello', expect.any(Function))

    lastOnEvent!({ type: 'started', conversationId: 7 })
    lastOnEvent!({ type: 'assistantStarted' })
    lastOnEvent!({ type: 'textDelta', text: 'Hi ' })
    lastOnEvent!({ type: 'textDelta', text: 'there' })
    expect(assistantAt(1).text).toBe('Hi there')
    expect(assistantAt(1).streaming).toBe(true)

    lastOnEvent!({
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
    lastOnEvent!({ type: 'assistantStarted' })
    lastOnEvent!({ type: 'toolCallStarted', callId: 'c1', tool: 'app_state' })
    expect(assistantAt(1).tools[0]).toMatchObject({ callId: 'c1', tool: 'app_state', running: true })
    lastOnEvent!({ type: 'toolCallFinished', callId: 'c1', ok: true })
    expect(assistantAt(1).tools[0]).toMatchObject({ running: false, ok: true })
  })

  it('a typed failure ends streaming and shows an honest notice', () => {
    sendMessage('hi')
    lastOnEvent!({ type: 'assistantStarted' })
    lastOnEvent!({ type: 'failed', kind: 'rateLimited' })
    expect(askCmdrState.streaming).toBe(false)
    // The empty assistant bubble is dropped; an error item takes its place.
    const last = askCmdrState.messages.at(-1)
    expect(last).toEqual({ kind: 'error', errorKind: 'rateLimited' })
  })

  it('ignores a second send while one is streaming (single-flight)', () => {
    sendMessage('first')
    sendMessage('second')
    expect(sendMock).toHaveBeenCalledTimes(1)
  })
})

describe('stopStreaming', () => {
  it('cancels the active turn and finalizes locally (no terminal event arrives)', () => {
    sendMessage('long one')
    lastOnEvent!({ type: 'started', conversationId: 3 })
    lastOnEvent!({ type: 'assistantStarted' })
    lastOnEvent!({ type: 'textDelta', text: 'partial' })
    stopStreaming()
    expect(cancelMock).toHaveBeenCalledWith(3)
    expect(askCmdrState.streaming).toBe(false)
    expect(assistantAt(1).streaming).toBe(false)
    expect(assistantAt(1).text).toBe('partial')
  })
})

describe('setRailWidth', () => {
  it('persists the width and clamps to the max', () => {
    setRailWidth(400)
    expect(askCmdrState.width).toBe(400)
    expect(saveMock).toHaveBeenCalledWith({ askCmdrRailWidth: 400 })
    setRailWidth(99999)
    expect(askCmdrState.width).toBe(RAIL_MAX_WIDTH)
  })
})

describe('isOverSoftCap', () => {
  it('trips only once the thread crosses the soft cap', () => {
    for (let i = 0; i <= THREAD_SOFT_CAP_MESSAGES; i++) {
      askCmdrState.messages.push({ kind: 'user', id: null, text: `m${i}` })
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
