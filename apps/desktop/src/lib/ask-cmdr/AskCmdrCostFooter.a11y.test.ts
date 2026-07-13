/**
 * Tier 3 a11y tests for `AskCmdrCostFooter.svelte`, the per-thread cost readout.
 *
 * The footer is a labelled row (token count + estimated cost) that only renders once the
 * thread has a metered turn. The trigger state and the cost command are mocked so it mounts
 * without a backend; a priced thread is used so the footer renders (its a11y surface).
 */

import { describe, it, vi, beforeAll } from 'vitest'
import { mount, flushSync } from 'svelte'
import { _setLocaleForTests } from '$lib/intl/locale'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { ConversationCost } from '$lib/tauri-commands'

const { triggerState, costMock } = vi.hoisted(() => ({
  triggerState: { conversationId: 1, streaming: false },
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

describe('AskCmdrCostFooter a11y', () => {
  it('the cost footer has no a11y violations', async () => {
    costMock.mockResolvedValue({
      promptTokens: 300,
      completionTokens: 70,
      costMicros: 1_230_000,
      fullyPriced: true,
      providers: ['openAi'],
    })
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AskCmdrCostFooter, { target, props: {} })
    flushSync()
    await Promise.resolve()
    flushSync()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
