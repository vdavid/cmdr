/**
 * Tier 3 a11y for `WakeIndicator.svelte`.
 *
 * The corner is glyph-only, so the whole burden falls on the accessible names: two real buttons
 * in the thinking state, one in each gap state, each with a name that says what pressing it does.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
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
  document.body.innerHTML = ''
})

describe('WakeIndicator a11y', () => {
  it('has no violations while a wake is thinking', async () => {
    const host = mountIndicator('thinking')

    expect(host.querySelectorAll('button')).toHaveLength(2)
    await expectNoA11yViolations(host)
  })

  it('has no violations while it reports a missing key', async () => {
    const host = mountIndicator('needsApiKey')

    await expectNoA11yViolations(host)
  })

  it('has no violations while it reports missing disk access', async () => {
    const host = mountIndicator('needsFullDiskAccess')

    await expectNoA11yViolations(host)
  })

  it('has nothing to violate when it is silent', async () => {
    const host = mountIndicator('silent')

    await expectNoA11yViolations(host)
  })
})
