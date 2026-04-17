/**
 * Tier 3 a11y tests for `ProgressBar.svelte`.
 *
 * The bar uses `role="progressbar"` with `aria-valuenow/min/max`. These
 * tests check the ARIA wiring at empty, partial, full progress, and with
 * an explicit aria-label.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import ProgressBar from './ProgressBar.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('ProgressBar a11y', () => {
  it('empty progress with ariaLabel has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ProgressBar, { target, props: { value: 0, ariaLabel: 'Download progress' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('50% progress with ariaLabel has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ProgressBar, { target, props: { value: 0.5, ariaLabel: 'Upload progress' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('complete (100%) with ariaLabel has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ProgressBar, { target, props: { value: 1, ariaLabel: 'Transfer progress' } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('small size with ariaLabel has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ProgressBar, { target, props: { value: 0.25, size: 'sm', ariaLabel: 'Indexing progress' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
