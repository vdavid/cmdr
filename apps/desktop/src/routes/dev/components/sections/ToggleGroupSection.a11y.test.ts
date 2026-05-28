/**
 * Tier 3 a11y test for the ToggleGroup catalog section. Mirrors the convention
 * used for `lib/settings/sections/*.a11y.test.ts`: mount the section in jsdom,
 * tick once, and let axe-core audit the rendered tree. Catches regressions in
 * the example configurations (badge/hint/tooltip wiring) without needing the
 * full app.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import ToggleGroupSection from './ToggleGroupSection.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('ToggleGroupSection a11y', () => {
  it('renders without a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ToggleGroupSection, { target })
    await tick()
    await expectNoA11yViolations(target)
  })
})
