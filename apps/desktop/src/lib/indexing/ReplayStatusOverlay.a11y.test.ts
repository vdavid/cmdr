/**
 * Tier 3 a11y tests for `ReplayStatusOverlay.svelte`.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import ReplayStatusOverlay from './ReplayStatusOverlay.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

let replaying = false

vi.mock('./index-state.svelte', () => ({
  isReplaying: () => replaying,
  getReplayEventsProcessed: () => 5000,
  getReplayEstimatedTotal: () => 10000,
  getReplayStartedAt: () => Date.now() - 5000,
  isScanning: () => false,
  isAggregating: () => false,
}))

describe('ReplayStatusOverlay a11y', () => {
  it('hidden (no replay) has no a11y violations', async () => {
    replaying = false
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ReplayStatusOverlay, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })
})
