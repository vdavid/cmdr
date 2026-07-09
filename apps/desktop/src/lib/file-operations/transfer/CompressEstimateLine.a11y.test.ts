/**
 * Tier 3 a11y tests for `CompressEstimateLine.svelte`.
 *
 * Covers the three visible states: a present estimate, the loading affordance
 * while a local scan runs, and the absent state (remote source), which renders
 * nothing and must stay violation-free as an empty mount.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(() => 6),
  setSetting: vi.fn(),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

import CompressEstimateLine from './CompressEstimateLine.svelte'

const estimate = { compressibleBytes: 1_000_000, mediumBytes: 500_000, incompressibleBytes: 250_000 }

describe('CompressEstimateLine a11y', () => {
  it('has no a11y violations with a present estimate', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(CompressEstimateLine, { target, props: { estimate, isScanning: false, sourceIsLocal: true } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('has no a11y violations while loading', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(CompressEstimateLine, { target, props: { estimate: null, isScanning: true, sourceIsLocal: true } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('has no a11y violations when absent (remote source)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(CompressEstimateLine, { target, props: { estimate: null, isScanning: true, sourceIsLocal: false } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
