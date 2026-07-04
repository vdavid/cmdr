/**
 * Tier 3 a11y tests for the generic `Menu` primitive.
 *
 * Axes the OPEN menu (the only state with rendered content — closed renders
 * nothing), rendered inline (`portal` off) so the content lands in the target.
 * Color contrast is tier 1's job; focus traps are tier 2's.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import Menu, { type MenuItem } from './Menu.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const items: MenuItem[] = [
  { value: 'browse', label: 'Browse like a folder' },
  { value: 'open', label: 'Open with default app' },
  { value: 'configure', label: 'Configure…' },
]

describe('Menu a11y', () => {
  it('open menu has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(Menu, {
      target,
      props: {
        open: true,
        onOpenChange: () => {},
        onSelect: () => {},
        items,
        ariaLabel: 'Open archive or bundle',
        defaultHighlightedValue: 'browse',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
