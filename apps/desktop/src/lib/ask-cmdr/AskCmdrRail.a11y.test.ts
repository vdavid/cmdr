/**
 * Tier 3 a11y tests for `AskCmdrRail.svelte`.
 *
 * The whole rail: header (title + ALPHA badge + new-chat + close), the thread, the
 * soft-cap nudge, and the composer. Covers the empty state, a populated thread, and the
 * over-soft-cap nudge. The trigger store is mocked to a plain object so the rail + its
 * child composer mount without the full explorer-state chain.
 */

import { describe, it, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { RailMessage } from './ask-cmdr-trigger.svelte'

// `vi.hoisted` so the shared mutable state exists before the hoisted `vi.mock` factory runs.
const { triggerState, flags } = vi.hoisted(() => ({
  triggerState: {
    streaming: false,
    width: 340,
    conversationId: null as number | null,
    messages: [] as RailMessage[],
    attachments: [] as unknown[],
  },
  flags: { overSoftCap: false },
}))

vi.mock('./ask-cmdr-trigger.svelte', () => ({
  askCmdrState: triggerState,
  isOverSoftCap: () => flags.overSoftCap,
  hasOlderMessages: () => false,
  loadOlderMessages: vi.fn(),
  closeRail: vi.fn(),
  newChat: vi.fn(),
  setRailWidth: vi.fn(),
  sendMessage: vi.fn(),
  stopStreaming: vi.fn(),
  markRailFocused: vi.fn(),
  returnFocusToPane: vi.fn(),
  addAttachments: vi.fn(),
  removeAttachment: vi.fn(),
}))
vi.mock('./ask-cmdr-sessions.svelte', () => ({
  sessionsState: { open: false },
  openSessions: vi.fn(),
}))

import AskCmdrRail from './AskCmdrRail.svelte'

function mountRail(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AskCmdrRail, { target, props: {} })
  return target
}

beforeEach(() => {
  triggerState.streaming = false
  triggerState.messages = []
  flags.overSoftCap = false
})

describe('AskCmdrRail a11y', () => {
  it('the empty rail has no a11y violations', async () => {
    const target = mountRail()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('a populated thread has no a11y violations', async () => {
    triggerState.messages = [
      { kind: 'user', id: 1, text: 'What is my biggest folder?', attachments: [] },
      {
        kind: 'assistant',
        id: 2,
        text: 'Your **Downloads** folder is the largest.',
        tools: [{ callId: 'c1', tool: 'largest_dirs', running: false, ok: true, path: '/Users/me' }],
        thinking: false,
        streaming: false,
      },
    ]
    const target = mountRail()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('the over-soft-cap nudge has no a11y violations', async () => {
    triggerState.messages = [{ kind: 'user', id: 1, text: 'hi', attachments: [] }]
    flags.overSoftCap = true
    const target = mountRail()
    await tick()
    await expectNoA11yViolations(target)
  })
})
