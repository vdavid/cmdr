/** Component tests for the per-thread cost footer: the honest free / estimate / unknown
 * miss-path, and hidden when the thread has no metered turn. */

import { describe, it, expect, vi, beforeEach, beforeAll } from 'vitest'
import { mount, flushSync } from 'svelte'
import { _setLocaleForTests } from '$lib/intl/locale'
import type { ConversationCost } from '$lib/tauri-commands'

const { triggerState, costMock } = vi.hoisted(() => ({
  triggerState: { conversationId: null as number | null, streaming: false },
  costMock: vi.fn<(id: number) => Promise<ConversationCost>>(),
}))

vi.mock('./ask-cmdr-trigger.svelte', () => ({ askCmdrState: triggerState }))
vi.mock('$lib/tauri-commands', () => ({ askCmdrConversationCost: (id: number) => costMock(id) }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import AskCmdrCostFooter from './AskCmdrCostFooter.svelte'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
beforeEach(() => {
  vi.clearAllMocks()
  triggerState.conversationId = 1
  triggerState.streaming = false
})

async function renderWith(cost: ConversationCost): Promise<HTMLElement> {
  costMock.mockResolvedValue(cost)
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(AskCmdrCostFooter, { target, props: {} })
  flushSync()
  await Promise.resolve() // let the cost promise resolve
  flushSync()
  return target
}

const base: ConversationCost = {
  promptTokens: 300,
  completionTokens: 70,
  costMicros: 0,
  fullyPriced: true,
  providers: [],
}

describe('AskCmdrCostFooter', () => {
  it('reads "free, on-device" for a local-only thread', async () => {
    const target = await renderWith({ ...base, providers: ['local'] })
    expect(target.textContent).toContain('370 tokens')
    expect(target.textContent).toContain('free')
    target.remove()
  })

  it('shows an estimated amount for a priced cloud thread', async () => {
    const target = await renderWith({ ...base, costMicros: 1_230_000, fullyPriced: true, providers: ['openai'] })
    expect(target.textContent).toContain('about')
    expect(target.textContent).toContain('$1.23')
    target.remove()
  })

  it('reads "cost unknown" for an unpriced thread, never a silent $0', async () => {
    const target = await renderWith({ ...base, fullyPriced: false, providers: ['openai'] })
    expect(target.textContent).toContain('unknown')
    expect(target.textContent).not.toContain('$0.00')
    target.remove()
  })

  it('stays hidden until the thread has a metered turn', async () => {
    const target = await renderWith({ ...base, promptTokens: 0, completionTokens: 0, providers: [] })
    expect(target.querySelector('.cost-footer')).toBeNull()
    target.remove()
  })
})
