/** Component tests for the per-thread cost footer: the honest free / estimate / unknown
 * miss-path, a new chat's zero line and baseline gauge, and the mid-turn re-read. */

import { describe, it, expect, vi, afterEach, beforeEach, beforeAll } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'
import { _setLocaleForTests } from '#lib/intl/locale.js'
import type { ConversationCost, SlotStatus } from '#lib/tauri-commands/index.js'

const { costMock } = vi.hoisted(() => ({
  costMock: vi.fn<(id: number) => Promise<ConversationCost>>(),
}))

// The REAL `$state` (its module imports only types), so a thread switch re-runs the footer's
// effect the way it does in the app. The trigger module itself pulls in the whole rail.
vi.mock('./ask-cmdr-trigger.svelte', async () => ({
  askCmdrState: (await import('./ask-cmdr-state.svelte')).askCmdrState,
}))
vi.mock('#lib/tauri-commands/index.js', () => ({ askCmdrConversationCost: (id: number) => costMock(id) }))
// A plain object: each test sets it before mounting, so the footer needn't react to it here.
const slot = vi.hoisted(() => ({
  slotStatusState: { status: null as SlotStatus | null },
}))
vi.mock('./ask-cmdr-slot-status.svelte', () => slot)
vi.mock('#lib/logging/logger.js', () => ({
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
  triggerState.contextUsage = null
  slot.slotStatusState.status = {
    readiness: 'ready',
    nextCall: { provider: 'openAi', priced: false },
    baseline: { estimatedTokens: 6_300, budgetTokens: 16_000 },
  }
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
  cacheReadTokens: 0,
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

    expect(target.textContent).not.toContain('$1.23')
    expect(target.textContent).toContain('0 tokens')
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

  it('shows a brand-new chat as zero tokens, priced the way its first call will be, with the baseline gauge', () => {
    triggerState.conversationId = null
    slot.slotStatusState.status = {
      readiness: 'ready',
      nextCall: { provider: 'anthropic', priced: true },
      baseline: { estimatedTokens: 6_300, budgetTokens: 16_000 },
    }
    const target = mountFooter()
    flushSync()

    expect(target.textContent).toContain('0 tokens')
    expect(target.textContent).toContain('$0')
    expect(costMock).not.toHaveBeenCalled()
    const meter = target.querySelector('[role="meter"]')
    expect(meter?.getAttribute('aria-valuenow')).toBe('39')
    target.remove()
  })

  it('reads "cost unknown" for a new chat on an unpriced model, never a silent $0', () => {
    triggerState.conversationId = null
    const target = mountFooter()
    flushSync()

    expect(target.textContent).toContain('0 tokens')
    expect(target.textContent).toContain('unknown')
    target.remove()
  })

  it('shows only the token count while AI is off, with no gauge', () => {
    triggerState.conversationId = null
    slot.slotStatusState.status = { readiness: 'off', nextCall: null, baseline: null }
    const target = mountFooter()
    flushSync()

    expect(target.querySelector('.cost-footer')?.textContent.trim()).toBe('0 tokens')
    expect(target.querySelector('[role="meter"]')).toBeNull()
    target.remove()
  })

  it('never lets an older read of the same thread land last and show a lower total', async () => {
    let releaseFirst: ((cost: ConversationCost) => void) | undefined
    costMock.mockReturnValueOnce(
      new Promise<ConversationCost>((resolve) => {
        releaseFirst = resolve
      }),
    )
    costMock.mockResolvedValue({ ...base, costMicros: 2_000_000, providers: ['openAi'] })
    triggerState.streaming = true
    const target = mountFooter()
    flushSync()

    // A second metered call re-reads the same thread; its (newer) answer lands first.
    triggerState.costRevision += 1
    flushSync()
    await Promise.resolve()
    flushSync()
    releaseFirst?.({ ...base, costMicros: 1_000_000, providers: ['openAi'] })
    await Promise.resolve()
    flushSync()

    expect(target.textContent).toContain('$2.00')
    expect(target.textContent).not.toContain('$1.00')
    target.remove()
  })

  it('re-reads the meter after each metered call while the turn is still running', async () => {
    triggerState.streaming = true
    const target = await renderWith({ ...base, costMicros: 1_000_000, providers: ['openAi'] })
    expect(target.textContent).toContain('$1.00')

    costMock.mockResolvedValue({ ...base, costMicros: 2_000_000, providers: ['openAi'] })
    triggerState.costRevision += 1
    flushSync()
    await Promise.resolve()
    flushSync()

    expect(target.textContent).toContain('$2.00')
    target.remove()
  })
})
