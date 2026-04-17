/**
 * Tier 3 a11y tests for `SortableHeader.svelte`.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import SortableHeader from './SortableHeader.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('SortableHeader a11y', () => {
  it('active ascending has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SortableHeader, {
      target,
      props: {
        column: 'name',
        label: 'Name',
        currentSortColumn: 'name',
        currentSortOrder: 'ascending',
        onClick: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('inactive right-aligned has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SortableHeader, {
      target,
      props: {
        column: 'size',
        label: 'Size',
        currentSortColumn: 'name',
        currentSortOrder: 'ascending',
        align: 'right',
        onClick: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
