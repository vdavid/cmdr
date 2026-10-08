/** Component tests for the per-thread cost footer: the honest free / estimate / unknown
 * miss-path, and hidden when the thread has no metered turn. */

import { describe, it, expect, vi, afterEach, beforeEach, beforeAll } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'
import { _setLocaleForTests } from '$lib/intl/locale'
import type { ConversationCost } from '$lib/tauri-commands'

const { costMock } = vi.hoisted(() => ({
  costMock: vi.fn<(id: number) => Promise<ConversationCost>>(),
}))

// The REAL `$state` (its module imports only types), so a thread switch re-runs the footer's
// effect the way it does in the app. The trigger module itself pulls in the whole rail.
vi.mock('./ask-cmdr-trigger.svelte', async () => ({
  askCmdrState: (await import('./ask-cmdr-state.svelte')).askCmdrState,
}))
vi.mock('$lib/tauri-commands', () => ({ askCmdrConversationCost: (id: number) => costMock(id) }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

import AskCmdrCostFooter from './AskCmdrCostFooter.svelte'
import { askCmdrState as triggerState } from './ask-cmdr-state.svelte'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
beforeEach(() => {
  vi.clearAllMocks()
  triggerState.conversationId = 1
  triggerState.streaming = false
})

// Unmounted after each test: the state is shared and reactive, so a footer left mounted would
// refetch on the next test's thread switch and take its mocked answers.
const mounted: ReturnType<typeof mount>[] = []
afterEach(() => {
  for (const instance of mounted.splice(0)) void unmount(instance)
})

function mountFooter(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted.push(mount(AskCmdrCostFooter, { target, props: {} }))
  return target
}

async function renderWith(cost: ConversationCost): Promise<HTMLElement> {
  costMock.mockResolvedValue(cost)
  const target = mountFooter()
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
    const target = await renderWith({ ...base, costMicros: 1_230_000, fullyPriced: true, providers: ['openAi'] })
    expect(target.textContent).toContain('about')
    expect(target.textContent).toContain('$1.23')
    target.remove()
  })

  it('reads "cost unknown" for an unpriced thread, never a silent $0', async () => {
    const target = await renderWith({ ...base, fullyPriced: false, providers: ['openAi'] })
    expect(target.textContent).toContain('unknown')
    expect(target.textContent).not.toContain('$0.00')
    target.remove()
  })

  it("drops the previous thread's cost the moment the user switches threads", async () => {
    const target = await renderWith({ ...base, costMicros: 1_230_000, providers: ['openAi'] })
    expect(target.textContent).toContain('$1.23')

    // Thread 2's read is still in flight: the footer must not keep showing thread 1's total.
    costMock.mockReturnValue(new Promise<ConversationCost>(() => {}))
    triggerState.conversationId = 2
    flushSync()

    expect(target.querySelector('.cost-footer')).toBeNull()
    target.remove()
  })

  it("never lets a slow read for the previous thread overwrite the current thread's cost", async () => {
    let releaseFirst: ((cost: ConversationCost) => void) | undefined
    costMock.mockReturnValueOnce(
      new Promise<ConversationCost>((resolve) => {
        releaseFirst = resolve
      }),
    )
    costMock.mockResolvedValue({ ...base, costMicros: 4_560_000, providers: ['openAi'] })
    const target = mountFooter()
    flushSync()

    triggerState.conversationId = 2
    flushSync()
    await Promise.resolve()
    flushSync()
    releaseFirst?.({ ...base, costMicros: 1_230_000, providers: ['openAi'] })
    await Promise.resolve()
    flushSync()

    expect(target.textContent).toContain('$4.56')
    expect(target.textContent).not.toContain('$1.23')
    target.remove()
  })

  it('stays hidden until the thread has a metered turn', async () => {
    const target = await renderWith({ ...base, promptTokens: 0, completionTokens: 0, providers: [] })
    expect(target.querySelector('.cost-footer')).toBeNull()
    target.remove()
  })
})
