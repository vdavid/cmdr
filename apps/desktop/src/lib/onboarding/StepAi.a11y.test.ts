/**
 * Tier 3 a11y test for the `StepAi.svelte` stub (M2). M3 ships the real provider
 * picker + per-provider setup and expands this coverage to one test per banner branch
 * and one per radio state.
 */

import { describe, it, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import StepAi from './StepAi.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

let mounted: { target: HTMLElement; instance: ReturnType<typeof mount> } | undefined

afterEach(() => {
  if (mounted) {
    unmount(mounted.instance)
    mounted.target.remove()
    mounted = undefined
  }
})

describe('StepAi a11y', () => {
  it('default stub state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(StepAi, { target, props: {} })
    mounted = { target, instance }
    await tick()
    await expectNoA11yViolations(target)
  })
})
