/**
 * Tier 3 a11y test for `SearchRowMenu.svelte`.
 *
 * The button carries `aria-label="More actions"` and is `tabindex="-1"` (the row is the
 * keyboard target; the button is a mouse-and-explicit-keyboard affordance reached via
 * the row context-menu IPC). Verify axe-core is happy with the resting and cursor-row
 * variants.
 */
import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SearchRowMenu from './SearchRowMenu.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tooltip/tooltip', () => ({
  tooltip: () => ({ destroy() {} }),
}))

describe('SearchRowMenu a11y', () => {
  it('cursor-row variant renders without axe violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchRowMenu, {
      target,
      props: { isCursorRow: true, onOpen: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('non-cursor variant renders without axe violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchRowMenu, {
      target,
      props: { isCursorRow: false, onOpen: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
