/**
 * Tier 3 a11y test for `PathPills.svelte`.
 *
 * The load-bearing rule (search-redesign-plan §3.8): pills are **not** in the
 * keyboard Tab order. Putting them in Tab order would break the row's
 * arrow-down keyboard flow inside virtualized rows. The dialog wires `⌥←` /
 * `⌥→` on the cursor row's path as the keyboard equivalent.
 *
 * This test pins the contract: every pill carries `tabindex="-1"`, so Tab
 * focus traversal walks past them.
 */
import { describe, it, expect } from 'vitest'
import { mount, tick } from 'svelte'
import PathPills from './PathPills.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('PathPills a11y', () => {
  it('marks every pill with tabindex="-1" so Tab skips them', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(PathPills, {
      target,
      props: { path: '/Users/dave/code', onPick: () => {} },
    })
    await tick()
    const pills = Array.from(target.querySelectorAll('.pill'))
    expect(pills.length).toBeGreaterThan(0)
    for (const p of pills) {
      expect(p.getAttribute('tabindex')).toBe('-1')
    }
    target.remove()
  })

  it('renders without axe-core violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(PathPills, {
      target,
      props: { path: '/Users/dave/code', onPick: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
