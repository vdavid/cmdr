/**
 * Tier 3 a11y test for the `StepOptional.svelte` stub (M2). M4 ships the real toggles
 * and expands this coverage to default + one-off-toggle states.
 */

import { describe, it, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import StepOptional from './StepOptional.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

let mounted: { target: HTMLElement; instance: ReturnType<typeof mount> } | undefined

afterEach(() => {
  if (mounted) {
    unmount(mounted.instance)
    mounted.target.remove()
    mounted = undefined
  }
})

describe('StepOptional a11y', () => {
  it('default stub state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(StepOptional, { target, props: {} })
    mounted = { target, instance }
    await tick()
    await expectNoA11yViolations(target)
  })
})
