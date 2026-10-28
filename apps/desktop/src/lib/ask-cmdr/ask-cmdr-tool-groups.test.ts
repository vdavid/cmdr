/**
 * What the rail remembers about tool groups and lines outside the thread: a group opens while its
 * turn runs, the turn's end applies the setting, a reload starts closed, a "load earlier" seam
 * carries a group's state to its merged key, and an expanded line fetches what its call returned
 * into a cache the message list never holds.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AskCmdrSendOutcome, AskCmdrStreamEvent, AskCmdrToolResult } from '#lib/tauri-commands/index.js'
import type { ResolvedCall, TurnStats } from '#lib/ipc/bindings.js'

const sendMock = vi.fn<(c: number | null, t: string, a: unknown[], d: string[]) => Promise<AskCmdrSendOutcome>>()
/** The line a result fetch asked for, as one payload. */
interface ResultAsk {
  conversationId: number
  messageId: number
  callId: string
  occurrence: number
}
const resultMock = vi.fn<(ask: ResultAsk) => Promise<AskCmdrToolResult>>()
const settings = vi.hoisted(() => ({ toolGroupsOnAnswer: 'collapse' }))

vi.mock('#lib/tauri-commands/index.js', () => ({
  sendAskCmdrMessage: (c: number | null, t: string, a: unknown[], d: string[]) => sendMock(c, t, a, d),
  askCmdrToolResult: (conversationId: number, messageId: number, callId: string, occurrence: number) =>
    resultMock({ conversationId, messageId, callId, occurrence }),
  cancelAskCmdr: vi.fn(() => Promise.resolve({ tripped: 1, live: 1 })),
  listAskCmdrConversations: vi.fn(() => Promise.resolve([])),
  getAskCmdrConversation: vi.fn(() => Promise.resolve(null)),
  recordAskCmdrSlotChange: vi.fn(() => Promise.resolve([])),
  preflightBulkRename: vi.fn(() => Promise.resolve({ status: 'ready', rows: [] })),
  cancelBulkRenameProposal: vi.fn(() => Promise.resolve()),
  applyBulkRename: vi.fn(() => Promise.resolve({ operationId: 'op-1' })),
  reviseBulkRenameRow: vi.fn(() => Promise.resolve()),
}))
vi.mock('#lib/settings/index.js', async (importOriginal) => {
  const original = await importOriginal<typeof import('#lib/settings/index.js')>()
  return {
    ...original,
    getSetting: (id: Parameters<typeof original.getSetting>[0]) =>
      id === 'askCmdr.toolGroupsOnAnswer' ? settings.toolGroupsOnAnswer : original.getSetting(id),
  }
})
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

import { askCmdrState, newChat, sendMessage, stopStreaming } from './ask-cmdr-trigger.svelte'
import { handleTurnEvent, resetTurnBookkeeping } from './ask-cmdr-stream.svelte'
import type { AssistantItem, RailMessage, Round } from './ask-cmdr-messages'
import { displaySegments, groupKey, mergeAtSeam } from './ask-cmdr-rounds'
import {
  _resetToolGroupsForTests,
  carryGroupStateAcrossSeam,
  claimGroupAnnouncement,
  forgetToolDetails,
  isGroupOpen,
  isLineExpanded,
  setGroupOpen,
  toggleLineExpanded,
  toolDetail,
} from './ask-cmdr-tool-groups.svelte'

const THREAD_ID = 3
const STATS: TurnStats = { toolCalls: 2, wallMs: 900, promptTokens: 10, completionTokens: 2 }

function resolved(callId: string): ResolvedCall {
  return {
    callId,
    tool: 'list_dir',
    subject: { kind: 'folder', path: { text: `/${callId}`, linkable: true } },
    repeatOf: null,
  }
}

function fire(event: AskCmdrStreamEvent): void {
  askCmdrState.conversationId = THREAD_ID
  handleTurnEvent({ conversationId: THREAD_ID, run: null, event })
}

const done = (messageId: number): AskCmdrStreamEvent => ({
  type: 'done',
  messageId,
  seq: messageId,
  stop: 'completed',
  usage: { promptTokens: 1, completionTokens: 1 },
  turn: STATS,
})

/** A turn whose first round (row 5) called `c1` and `c2`, both finished, still streaming. */
function streamOneGroup(): void {
  sendMessage('look around')
  fire({ type: 'started' })
  fire({ type: 'assistantStarted' })
  fire({ type: 'toolCallsResolved', messageId: 5, calls: [resolved('c1'), resolved('c2')] })
  for (const callId of ['c1', 'c2']) {
    fire({
      type: 'toolCallFinished',
      messageId: 5,
      callId,
      outcome: { kind: 'answered' },
      subject: { kind: 'folder', path: { text: `/${callId}`, linkable: true } },
    })
  }
}

function bubble(): AssistantItem {
  const item = askCmdrState.messages.findLast((message) => message.kind === 'assistant')
  if (item?.kind !== 'assistant') throw new Error('expected a bubble')
  return item
}

/** The bubble's first group's key and whether it shows open, the way `AskCmdrToolGroup` reads it. */
function firstGroup(item: AssistantItem = bubble()): { key: string | null; open: boolean } {
  const segment = displaySegments(item.rounds).find((candidate) => candidate.kind === 'tools')
  if (segment?.kind !== 'tools') throw new Error('expected a group')
  const key = groupKey(segment.lines)
  const running = segment.lines.some((line) => line.call.running)
  return { key, open: isGroupOpen(key, { live: item.streaming, running }) }
}

beforeEach(() => {
  sendMock.mockReset()
  resultMock.mockReset()
  sendMock.mockImplementation(() => Promise.resolve({ accepted: true, conversationId: THREAD_ID, run: 1 }))
  newChat()
  resetTurnBookkeeping()
  _resetToolGroupsForTests()
  askCmdrState.messages = []
  askCmdrState.conversationId = null
  settings.toolGroupsOnAnswer = 'collapse'
})

describe('group open state through a turn', () => {
  it('opens while the turn runs, then closes when the answer arrives under collapse', () => {
    streamOneGroup()
    expect(firstGroup()).toEqual({ key: '5:c1', open: true })
    fire(done(7))
    expect(firstGroup().open).toBe(false)
  })

  it('keeps each group as it stood under keep: an untouched one stays open, a closed one closed', () => {
    settings.toolGroupsOnAnswer = 'keep'
    streamOneGroup()
    fire({ type: 'textDelta', text: 'Now the second batch.' })
    fire({ type: 'toolCallsResolved', messageId: 6, calls: [resolved('c1'), resolved('c3')] })
    const second = '6:c1'
    setGroupOpen(second, false)
    fire(done(7))
    expect(firstGroup().open).toBe(true)
    expect(isGroupOpen(second, { live: false, running: false })).toBe(false)
  })

  it('lets a toggle made mid-turn beat the auto-open until the turn ends', () => {
    streamOneGroup()
    setGroupOpen('5:c1', false)
    fire({ type: 'textDelta', text: 'still going' })
    expect(firstGroup().open).toBe(false)
  })

  it.each([
    ['failed', { type: 'failed', kind: 'unfinishedReply', detail: null, managed: null, turn: STATS }],
    ['stopped elsewhere', { type: 'stopped', turn: STATS }],
  ] as const)('applies the setting on a turn that %s', (_, terminal) => {
    streamOneGroup()
    fire(terminal)
    expect(firstGroup().open).toBe(false)
  })

  it('holds a stopped turn open until its terminal, then applies the setting', () => {
    streamOneGroup()
    stopStreaming()
    expect(firstGroup().open).toBe(true)
    fire({ type: 'stopped', turn: STATS })
    expect(firstGroup().open).toBe(false)
  })

  /** A round still streaming at the click resolves after it, which gives its group a new key. That
   *  group is held open too: the stopped turn's end, and the setting, only come with its terminal. */
  it('holds open a group the stopped turn resolved after the click', () => {
    streamOneGroup()
    fire({ type: 'textDelta', text: 'Now the second batch.' })
    fire({ type: 'toolCallStarted', callId: 'c3', tool: 'list_dir' })
    stopStreaming()

    fire({ type: 'toolCallsResolved', messageId: 6, calls: [resolved('c3')] })
    fire({
      type: 'toolCallFinished',
      messageId: 6,
      callId: 'c3',
      outcome: { kind: 'answered' },
      subject: { kind: 'folder', path: { text: '/c3', linkable: true } },
    })

    expect(isGroupOpen('6:c3', { live: false, running: false })).toBe(true)
    fire({ type: 'stopped', turn: STATS })
    expect(isGroupOpen('6:c3', { live: false, running: false })).toBe(false)
  })

  it('keeps its key as calls append mid-turn', () => {
    streamOneGroup()
    // A round with no narration joins the same group.
    fire({ type: 'assistantStarted' })
    fire({ type: 'toolCallsResolved', messageId: 6, calls: [resolved('c3')] })
    expect(firstGroup().key).toBe('5:c1')
  })
})

describe('a reloaded group', () => {
  function settledBubble(running: boolean): AssistantItem {
    const call = { callId: 'c1', tool: 'list_dir', running, subject: null, outcome: null, repeatOf: null }
    return {
      kind: 'assistant',
      id: 5,
      turnId: 4,
      rounds: [{ messageId: 5, text: '', calls: [call, { ...call, callId: 'c2' }] }],
      thinking: false,
      stalled: false,
      streaming: false,
      stats: null,
    }
  }

  it('starts closed, under either setting', () => {
    for (const mode of ['collapse', 'keep'] as const) {
      settings.toolGroupsOnAnswer = mode
      expect(firstGroup(settledBubble(false)).open).toBe(false)
    }
  })

  /** Reopening a thread in the same session shows what a cold reload would: a group left open
   *  before the switch starts closed again. */
  it('starts closed after a thread switch, like a cold reload', () => {
    settings.toolGroupsOnAnswer = 'keep'
    streamOneGroup()
    fire(done(7))
    expect(firstGroup().open).toBe(true)

    forgetToolDetails()

    expect(isGroupOpen('5:c1', { live: false, running: false })).toBe(false)
  })

  it('starts open when a line in it still runs (a reload mid-turn)', () => {
    expect(firstGroup(settledBubble(true)).open).toBe(true)
  })
})

describe('carryGroupStateAcrossSeam', () => {
  const line = (callId: string) => ({
    callId,
    tool: 'list_dir',
    running: false,
    subject: null,
    outcome: { kind: 'answered' as const },
    repeatOf: null,
  })
  const turn = (rounds: Round[], id: number): AssistantItem => ({
    kind: 'assistant',
    id,
    turnId: 1,
    rounds,
    thinking: false,
    stalled: false,
    streaming: false,
    stats: null,
  })

  it('moves the open state of the newer half’s first group to the merged group’s key', () => {
    const older: RailMessage[] = [turn([{ messageId: 2, text: '', calls: [line('a')] }], 2)]
    const head = turn([{ messageId: 4, text: '', calls: [line('b'), line('c')] }], 4)
    setGroupOpen('4:b', true)

    const merged = mergeAtSeam(older, [head])
    carryGroupStateAcrossSeam(head, merged)
    const mergedBubble = merged[0] as AssistantItem
    expect(firstGroup(mergedBubble)).toEqual({ key: '2:a', open: true })
  })
})

describe('the expanded line', () => {
  const ready: AskCmdrToolResult = {
    found: true,
    detail: {
      rawJson: '{ "secret": "gps 59.3, 18.0" }',
      preview: { rows: [], rowsGivenToModel: null, total: null, lowerBound: false, problem: null, setAside: false },
    },
  }

  it('fetches what the call returned on its first expand, by its line', async () => {
    askCmdrState.conversationId = THREAD_ID
    resultMock.mockResolvedValue(ready)
    toggleLineExpanded('5:c1', { messageId: 5, callId: 'c1', occurrence: 0 })
    expect(isLineExpanded('5:c1')).toBe(true)
    expect(toolDetail('5:c1')).toEqual({ status: 'loading' })
    await vi.waitFor(() => {
      expect(toolDetail('5:c1')).toEqual({ status: 'ready', detail: ready.detail })
    })
    expect(resultMock).toHaveBeenCalledWith({ conversationId: THREAD_ID, messageId: 5, callId: 'c1', occurrence: 0 })

    toggleLineExpanded('5:c1', { messageId: 5, callId: 'c1', occurrence: 0 })
    toggleLineExpanded('5:c1', { messageId: 5, callId: 'c1', occurrence: 0 })
    expect(resultMock).toHaveBeenCalledTimes(1)
  })

  it('says so calmly when the result is gone', async () => {
    askCmdrState.conversationId = THREAD_ID
    resultMock.mockResolvedValue({ found: false, reason: 'notFound' })
    toggleLineExpanded('5:c1', { messageId: 5, callId: 'c1', occurrence: 0 })
    await vi.waitFor(() => {
      expect(toolDetail('5:c1')).toEqual({ status: 'notFound' })
    })
  })

  it('drops an answer that arrives after the person moved to another thread', async () => {
    askCmdrState.conversationId = THREAD_ID
    let answer: (result: AskCmdrToolResult) => void = () => {}
    resultMock.mockReturnValue(new Promise((resolve) => (answer = resolve)))
    toggleLineExpanded('5:c1', { messageId: 5, callId: 'c1', occurrence: 0 })
    askCmdrState.conversationId = 99
    answer(ready)
    await vi.waitFor(() => {
      expect(toolDetail('5:c1')).toBeUndefined()
    })
  })

  it('never puts the raw result into the message list', async () => {
    streamOneGroup()
    resultMock.mockResolvedValue(ready)
    toggleLineExpanded('5:c1', { messageId: 5, callId: 'c1', occurrence: 0 })
    await vi.waitFor(() => {
      expect(toolDetail('5:c1')?.status).toBe('ready')
    })
    expect(JSON.stringify(askCmdrState)).not.toContain('gps')
  })
})

describe('claimGroupAnnouncement', () => {
  it('lets a settled group be announced once', () => {
    expect(claimGroupAnnouncement('5:c1')).toBe(true)
    expect(claimGroupAnnouncement('5:c1')).toBe(false)
  })
})
