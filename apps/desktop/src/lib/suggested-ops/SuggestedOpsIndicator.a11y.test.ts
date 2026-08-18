/**
 * The status-corner indicator: a11y, and the one rule that keeps the corner honest.
 *
 * It hides at zero rather than showing an empty badge. The corner is reserved for work in
 * progress, so a control that is always there for a feature with nothing to say is noise, and
 * a "0" reads as a broken counter rather than as calm.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const { openMock, badge } = vi.hoisted(() => ({
  openMock: vi.fn<() => Promise<void>>(),
  badge: { state: { pendingGroupCount: 0, pendingOpCount: 0 } },
}))

vi.mock('./suggested-ops-badge.svelte', () => ({ suggestedOpsBadge: badge.state }))
vi.mock('./suggested-ops-trigger.svelte', () => ({ openSuggestedOps: openMock }))

const SuggestedOpsIndicator = (await import('./SuggestedOpsIndicator.svelte')).default

function mountIndicator(): HTMLElement {
  const host = document.createElement('div')
  document.body.appendChild(host)
  mount(SuggestedOpsIndicator, { target: host })
  flushSync()
  return host
}

beforeEach(() => {
  vi.clearAllMocks()
  document.body.innerHTML = ''
  badge.state.pendingGroupCount = 0
  badge.state.pendingOpCount = 0
})

describe('the indicator', () => {
  it('renders nothing when nothing is waiting', () => {
    const host = mountIndicator()

    expect(host.querySelector('button')).toBeNull()
  })

  it('shows the count once something is waiting', () => {
    badge.state.pendingGroupCount = 3
    badge.state.pendingOpCount = 61
    const host = mountIndicator()

    expect(host.querySelector('button')).not.toBeNull()
    expect(host.textContent).toContain('3')
  })

  it('carries an accessible name, since the glyph and number alone say nothing', () => {
    badge.state.pendingGroupCount = 2
    const host = mountIndicator()

    const button = host.querySelector('button')
    expect((button?.getAttribute('aria-label') ?? '').length).toBeGreaterThan(0)
  })

  it('opens the review when clicked', () => {
    badge.state.pendingGroupCount = 1
    const host = mountIndicator()

    host.querySelector('button')?.click()

    expect(openMock).toHaveBeenCalledTimes(1)
  })

  it('has no a11y violations', async () => {
    badge.state.pendingGroupCount = 4
    const host = mountIndicator()

    await expectNoA11yViolations(host)
  })
})
