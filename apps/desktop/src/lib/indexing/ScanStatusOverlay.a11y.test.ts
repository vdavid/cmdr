/**
 * Tier 3 a11y tests for `ScanStatusOverlay.svelte`.
 *
 * Thin wrapper over `ProgressOverlay`. Index state is stubbed so we can
 * render the overlay in scanning and aggregation modes without touching
 * the real indexer.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import ScanStatusOverlay from './ScanStatusOverlay.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

let scanning = false
let aggregating = false
let aggPhase = 'sorting'

vi.mock('./index-state.svelte', () => ({
  isScanning: () => scanning,
  getEntriesScanned: () => 12000,
  getDirsFound: () => 420,
  isAggregating: () => aggregating,
  getAggregationPhase: () => aggPhase,
  getAggregationCurrent: () => 500,
  getAggregationTotal: () => 1000,
  getAggregationStartedAt: () => Date.now() - 3000,
}))

describe('ScanStatusOverlay a11y', () => {
  it('hidden (no scan, no aggregation) has no a11y violations', async () => {
    scanning = false
    aggregating = false
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ScanStatusOverlay, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('scanning phase has no a11y violations', async () => {
    scanning = true
    aggregating = false
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ScanStatusOverlay, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('aggregating/sorting phase has no a11y violations', async () => {
    scanning = false
    aggregating = true
    aggPhase = 'sorting'
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ScanStatusOverlay, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })
})
