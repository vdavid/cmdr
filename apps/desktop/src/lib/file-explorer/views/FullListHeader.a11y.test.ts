/**
 * Tier 3 a11y tests for `FullListHeader.svelte`.
 *
 * The Full view's column header. It mounts with no Tauri, settings, or
 * icon-cache dependency, so unlike its `FullList` host it needs no stubs.
 *
 * Every branch that changes the DOM gets a case, because each one adds or moves a
 * focusable sort trigger: the default four columns, the `showExtensionInName` split
 * (two triggers sharing the Name track), the optional Git cell, and an unfocused
 * pane (`SortableHeader` styles its caret off `isFocused`).
 *
 * Deliberately NOT asserted here: the header carries no `role`. It sits inside
 * `FullList`'s `role="listbox"` scroll container, and `role="toolbar"` would violate
 * `aria-required-children` on the listbox. The composed header-inside-listbox tree is
 * what `FullList.a11y.test.ts` covers.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import FullListHeader from './FullListHeader.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { SortColumn, SortOrder } from '../types'

interface HeaderProps {
  gridTemplate: string
  isFocused: boolean
  sortBy: SortColumn
  sortOrder: SortOrder
  showExtensionInName: boolean
  gitColumnVisible: boolean
  skipTransition: boolean
  scrollbarWidth: number
  onSortChange?: (column: SortColumn) => void
}

async function mountHeader(overrides: Partial<HeaderProps> = {}): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(FullListHeader, {
    target,
    props: {
      gridTemplate: '16px 1fr 60px 115px 80px',
      isFocused: true,
      sortBy: 'name',
      sortOrder: 'ascending',
      showExtensionInName: false,
      gitColumnVisible: false,
      skipTransition: false,
      scrollbarWidth: 0,
      onSortChange: vi.fn(),
      ...overrides,
    } satisfies HeaderProps,
  })
  await tick()
  return target
}

describe('FullListHeader a11y', () => {
  it('default four columns have no a11y violations', async () => {
    await expectNoA11yViolations(await mountHeader())
  })

  it('the split Name+Ext header has no a11y violations', async () => {
    // Two sort triggers share the single `1fr` Name track here, so both must
    // still be reachable and named.
    await expectNoA11yViolations(await mountHeader({ showExtensionInName: true, sortBy: 'extension' }))
  })

  it('the optional Git column has no a11y violations', async () => {
    // The Git cell is a label, not a trigger: it carries a `title` and no role.
    await expectNoA11yViolations(await mountHeader({ gitColumnVisible: true }))
  })

  it('an unfocused pane has no a11y violations', async () => {
    await expectNoA11yViolations(await mountHeader({ isFocused: false, sortBy: 'modified', sortOrder: 'descending' }))
  })

  it('every branch at once has no a11y violations', async () => {
    await expectNoA11yViolations(
      await mountHeader({ showExtensionInName: true, gitColumnVisible: true, skipTransition: true, sortBy: 'size' }),
    )
  })
})
