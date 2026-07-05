/**
 * Tier 3 a11y tests for the presentational `Menu` primitive.
 *
 * The menu renders its content whenever mounted (the caller gates it with `{#if}`),
 * so mounting it is enough to axe the open state. It portals to `document.body`, so
 * axe the whole body. Color contrast is tier 1's job; focus traps are tier 2's.
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
        items,
        onSelect: () => {},
        onClose: () => {},
        ariaLabel: 'Open archive or bundle',
        anchorPoint: { x: 100, y: 100 },
        highlightedValue: 'browse',
      },
    })
    await tick()
    await expectNoA11yViolations(document.body)
  })
})
