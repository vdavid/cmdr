/**
 * Tier 3 a11y test for `OnboardingStepShell.svelte`. Just a padded scroll container that
 * renders its children, so the assertion is structural: with a minimal child, axe finds
 * nothing wrong.
 */

import { describe, it, afterEach } from 'vitest'
import { mount, tick, unmount, createRawSnippet } from 'svelte'
import OnboardingStepShell from './OnboardingStepShell.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

let mounted: { target: HTMLElement; instance: ReturnType<typeof mount> } | undefined

afterEach(() => {
  if (mounted) {
    unmount(mounted.instance)
    mounted.target.remove()
    mounted = undefined
  }
})

describe('OnboardingStepShell a11y', () => {
  it('renders children without a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const children = createRawSnippet(() => ({
      render: () => '<p>Test content</p>',
    }))
    const instance = mount(OnboardingStepShell, { target, props: { children } })
    mounted = { target, instance }
    await tick()
    await expectNoA11yViolations(target)
  })
})
