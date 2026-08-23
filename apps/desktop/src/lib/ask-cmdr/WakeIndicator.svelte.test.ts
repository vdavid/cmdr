/**
 * The rendered wake indicator: which state puts what in the corner, and what pressing it does.
 *
 * The gate itself (which states are allowed to render at all) is `wake-indicator.svelte.test.ts`'s
 * subject. This file is about what the user sees and touches once a state has been allowed
 * through, so the state module is stubbed and only the markup is real.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount } from 'svelte'
import type { WakeIndicatorMode } from './wake-indicator.svelte'
import WakeIndicator from './WakeIndicator.svelte'

const { indicator } = vi.hoisted(() => ({
  indicator: {
    mode: 'silent',
    state: { thinkingIn: null as number | null, readiness: 'ready', proactive: true },
    openWakeThread: vi.fn<() => Promise<void>>(() => Promise.resolve()),
    stopWake: vi.fn<() => Promise<void>>(() => Promise.resolve()),
  },
}))

vi.mock('./wake-indicator.svelte', () => ({
  wakeIndicator: indicator.state,
  wakeIndicatorMode: () => indicator.mode,
  openWakeThread: indicator.openWakeThread,
  stopWake: indicator.stopWake,
}))

function mountIndicator(mode: WakeIndicatorMode): HTMLElement {
  indicator.mode = mode
  const host = document.createElement('div')
  document.body.appendChild(host)
  mount(WakeIndicator, { target: host })
  flushSync()
  return host
}

beforeEach(() => {
  vi.clearAllMocks()
  document.body.innerHTML = ''
})

describe('the wake indicator', () => {
  it('puts nothing in the corner when it has nothing to say', () => {
    const host = mountIndicator('silent')

    expect(host.querySelector('.wake-indicator')).toBeNull()
  })

  it('offers a way in AND a way out while a wake is thinking', () => {
    // Both, always: a background turn spending the user's money that can only be watched is not
    // cancelable, which `docs/design-principles.md` rules out.
    const host = mountIndicator('thinking')

    expect(host.querySelectorAll('button')).toHaveLength(2)
  })

  it('opens the wake thread from the first button and stops the wake from the second', () => {
    const host = mountIndicator('thinking')
    const [open, stop] = [...host.querySelectorAll('button')]

    open.click()
    expect(indicator.openWakeThread).toHaveBeenCalledTimes(1)
    expect(indicator.stopWake).not.toHaveBeenCalled()

    stop.click()
    expect(indicator.stopWake).toHaveBeenCalledTimes(1)
  })

  it('names both buttons, since neither carries visible text', () => {
    const host = mountIndicator('thinking')

    for (const button of host.querySelectorAll('button')) {
      expect((button.getAttribute('aria-label') ?? '').length).toBeGreaterThan(0)
    }
  })

  it('shows one button for a gap, and says which gap it is', () => {
    const fda = mountIndicator('needsFullDiskAccess')
    const fdaLabel = fda.querySelector('button')?.getAttribute('aria-label') ?? ''
    document.body.innerHTML = ''
    const key = mountIndicator('needsApiKey')
    const keyLabel = key.querySelector('button')?.getAttribute('aria-label') ?? ''

    expect(key.querySelectorAll('button')).toHaveLength(1)
    expect(fdaLabel.length).toBeGreaterThan(0)
    // ❌ The two gaps must not share a label: they send the user to different screens, and a
    // shared sentence would send half of them to the wrong one.
    expect(fdaLabel).not.toBe(keyLabel)
  })
})
