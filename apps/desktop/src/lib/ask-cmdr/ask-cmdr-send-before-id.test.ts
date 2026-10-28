/**
 * A send whose thread id hasn't come back yet. A brand-new chat has no thread until the send
 * command answers, so whatever happens in that window (a Stop, a New chat, a switch, the turn's
 * own first events, a wake's) has to land where it belongs once the id is known: a Stop still
 * cancels the turn, and a chat the user left is never taken over by it.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { AskCmdrSendOutcome, AskCmdrStreamEvent } from '#lib/tauri-commands/index.js'
import type { CancelOutcomeView } from '#lib/ipc/bindings.js'

const sendMock = vi.fn<(c: number | null, t: string, a: unknown[], d: string[]) => Promise<AskCmdrSendOutcome>>()
const cancelMock = vi.fn<(id: number) => Promise<CancelOutcomeView>>()

vi.mock('#lib/tauri-commands/index.js', () => ({
  sendAskCmdrMessage: (c: number | null, t: string, a: unknown[], d: string[]) => sendMock(c, t, a, d),
  cancelAskCmdr: (id: number) => cancelMock(id),
  listAskCmdrConversations: vi.fn(() => Promise.resolve([])),
  getAskCmdrConversation: vi.fn(() => Promise.resolve(null)),
  recordAskCmdrSlotChange: vi.fn(() => Promise.resolve([])),
}))
vi.mock('#lib/app-status-store.js', () => ({ saveAppStatus: vi.fn() }))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))
vi.mock('#lib/file-explorer/pane/explorer-state.svelte.js', () => ({
  explorerState: { setRailFocused: vi.fn() },
}))
vi.mock('./rail-window', () => ({
  growMainWindowForRail: vi.fn(() => Promise.resolve()),
  shrinkMainWindowForRail: vi.fn(() => Promise.resolve()),
}))
vi.mock('./ask-cmdr-gate.svelte', () => ({
  refreshRailGate: vi.fn(() => Promise.resolve('chat')),
}))

import { askCmdrState, newChat, sendMessage, stopStreaming, switchToThread } from './ask-cmdr-trigger.svelte'
import { handleTurnEvent, resetTurnBookkeeping } from './ask-cmdr-stream.svelte'

/** The thread the backend creates for the brand-new chat. */
const NEW_THREAD = 4
/** A thread a wake is thinking in meanwhile. */
const WAKE_THREAD = 99

/** Hold the next send's answer until the test gives it. */
function holdNextSend(): (outcome: AskCmdrSendOutcome) => Promise<void> {
  let answer: (outcome: AskCmdrSendOutcome) => void = () => {}
  sendMock.mockReturnValueOnce(new Promise((resolve) => (answer = resolve)))
  return async (outcome) => {
    answer(outcome)
    await flushMicrotasks()
  }
}

const accepted = (conversationId: number): AskCmdrSendOutcome => ({ accepted: true, conversationId, run: 1 })

async function flushMicrotasks(): Promise<void> {
  for (let i = 0; i < 5; i++) await Promise.resolve()
}

function fire(conversationId: number, event: AskCmdrStreamEvent): void {
  handleTurnEvent({ conversationId, run: null, event })
}

function bubbleText(): string {
  const bubble = askCmdrState.messages.find((m) => m.kind === 'assistant')
  return bubble?.kind === 'assistant' ? bubble.rounds.map((round) => round.text).join('') : ''
}

beforeEach(() => {
  // The previous test's turn first: clearing it can Stop it, which must not count here.
  newChat()
  resetTurnBookkeeping()
  askCmdrState.messages = []
  askCmdrState.conversationId = null
  sendMock.mockReset()
  cancelMock.mockReset()
  sendMock.mockImplementation((c) => Promise.resolve(accepted(c ?? NEW_THREAD)))
  cancelMock.mockResolvedValue({ tripped: 1, live: 1 })
})

describe('Stop before the thread id is known', () => {
  /** ⚠️ The turn runs whether or not the rail knows its thread yet. A Stop that waited for no
   *  id sent no cancel, and the turn ran on, spending, behind a rail that said it had stopped. */
  it('cancels the turn as soon as its id comes back', async () => {
    const answer = holdNextSend()
    sendMessage('first message')
    stopStreaming()
    expect(askCmdrState.streaming).toBe(false)
    expect(cancelMock).not.toHaveBeenCalled()

    await answer(accepted(NEW_THREAD))

    expect(cancelMock).toHaveBeenCalledWith(NEW_THREAD)
    // The chat is still the one on screen, so it is that thread now, and a next send goes there.
    expect(askCmdrState.conversationId).toBe(NEW_THREAD)
  })

  /** Its chunks after the Stop belong to the stopped turn and its terminal settles it, so
   *  neither puts the rail back into "working…". */
  it('treats the stopped turn like any other stop once the id is known', async () => {
    const answer = holdNextSend()
    sendMessage('first message')
    fire(NEW_THREAD, { type: 'started' })
    stopStreaming()
    await answer(accepted(NEW_THREAD))

    fire(NEW_THREAD, { type: 'textDelta', text: 'after the stop' })
    fire(NEW_THREAD, { type: 'stopped', turn: null })
    sendMessage('next one')

    expect(sendMock).toHaveBeenLastCalledWith(NEW_THREAD, 'next one', [], [])
    expect(askCmdrState.streaming).toBe(true)
    expect(bubbleText()).toBe('')
  })

  it('sends no cancel for a send the backend refused, which ran no turn', async () => {
    const answer = holdNextSend()
    sendMessage('refused')
    stopStreaming()

    await answer({ accepted: false, kind: 'provider', detail: null, managed: null })

    expect(cancelMock).not.toHaveBeenCalled()
  })

  /** The watchdog's timeout is a Stop too, and a fresh chat's first send is where a wedged
   *  provider is most likely to show. */
  it('cancels when the watchdog gives up before the id came back', async () => {
    vi.useFakeTimers()
    try {
      const answer = holdNextSend()
      sendMessage('first message')
      await vi.advanceTimersByTimeAsync(90_000)
      expect(askCmdrState.streaming).toBe(false)

      await answer(accepted(NEW_THREAD))

      expect(cancelMock).toHaveBeenCalledWith(NEW_THREAD)
    } finally {
      vi.useRealTimers()
    }
  })
})

describe('leaving the chat while its send is in flight', () => {
  /** ⚠️ The send's answer used to set the thread unconditionally, so the old thread took over the
   *  new chat the user had just opened. */
  it('keeps a New chat opened meanwhile, and stops the turn it left', async () => {
    const answer = holdNextSend()
    sendMessage('first message')
    newChat()

    await answer(accepted(NEW_THREAD))
    fire(NEW_THREAD, { type: 'textDelta', text: 'the old turn' })

    expect(askCmdrState.conversationId).toBeNull()
    expect(askCmdrState.messages).toEqual([])
    expect(cancelMock).toHaveBeenCalledWith(NEW_THREAD)
  })

  it('keeps a New chat opened during a send to an existing thread', async () => {
    askCmdrState.conversationId = 3
    const answer = holdNextSend()
    sendMessage('follow-up')
    newChat()

    await answer(accepted(3))

    expect(askCmdrState.conversationId).toBeNull()
  })

  it('keeps a thread switched to meanwhile', async () => {
    const answer = holdNextSend()
    sendMessage('first message')
    void switchToThread(8)
    askCmdrState.conversationId = 8

    await answer(accepted(NEW_THREAD))

    expect(askCmdrState.conversationId).toBe(8)
  })

  /** A refusal belongs to the chat that sent it. Painting it into the new chat would also end
   *  the new chat's own turn. */
  it("paints a left chat's refusal nowhere, and leaves the new chat's turn running", async () => {
    const answer = holdNextSend()
    sendMessage('first message')
    newChat()
    sendMessage('in the new chat')

    await answer({ accepted: false, kind: 'provider', detail: null, managed: null })

    expect(askCmdrState.streaming).toBe(true)
    expect(askCmdrState.messages.map((m) => m.kind)).toEqual(['user'])
  })
})

describe("a brand-new chat's events that beat its id", () => {
  /** The backend can announce the turn before the command's answer arrives. Those events are the
   *  chat's own once the id says so, and only then. */
  it('applies them once the id comes back', async () => {
    const answer = holdNextSend()
    sendMessage('first message')
    fire(NEW_THREAD, { type: 'started' })
    fire(NEW_THREAD, { type: 'assistantStarted' })
    fire(NEW_THREAD, { type: 'textDelta', text: 'early ' })

    await answer(accepted(NEW_THREAD))
    fire(NEW_THREAD, { type: 'textDelta', text: 'and on time' })

    expect(bubbleText()).toBe('early and on time')
  })

  /** A wake starting in another thread in that window used to hand the fresh chat ITS id. */
  it("never takes a wake's turn for the chat's", async () => {
    const answer = holdNextSend()
    sendMessage('first message')
    fire(WAKE_THREAD, { type: 'started' })
    fire(WAKE_THREAD, { type: 'textDelta', text: "a wake's thought" })
    expect(askCmdrState.conversationId).toBeNull()

    await answer(accepted(NEW_THREAD))

    expect(askCmdrState.conversationId).toBe(NEW_THREAD)
    expect(bubbleText()).toBe('')
  })
})
