/**
 * Tier-3 a11y tests for `SearchFilterChips.svelte` (M3).
 *
 * Covers the chip strip in default and configured states plus the open popovers (size, modified,
 * scope). Popovers are dialogs (`role="dialog"`); the chip is `aria-haspopup="dialog"` with an
 * `aria-expanded` reflecting the open state.
 *
 * Tier 3 = jsdom + axe-core for structural a11y. Color contrast is checked at design time by
 * `scripts/check-a11y-contrast/` (tier 1). Full-page focus-trap and Escape-return checks live in
 * Playwright (tier 2).
 */

import { describe, it } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'
import { SvelteSet } from 'svelte/reactivity'
import SearchFilterChips from './SearchFilterChips.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

type Props = ComponentProps<typeof SearchFilterChips>

function baseProps(overrides: Partial<Props> = {}): Props {
  return {
    caseSensitive: false,
    scope: '',
    excludeSystemDirs: true,
    searchableFolder: { path: '/Users/test', disabled: false, disabledReason: '' },
    sizeFilter: 'any',
    sizeValue: '',
    sizeUnit: 'MB',
    sizeValueMax: '',
    sizeUnitMax: 'MB',
    dateFilter: 'any',
    dateValue: '',
    dateValueMax: '',
    systemDirExcludeTooltip: 'Excluded: <code>node_modules</code>, <code>.git</code>',
    highlightedFields: new SvelteSet<string>(),
    disabled: false,
    onInput: () => () => {},
    onToggleCaseSensitive: () => {},
    onToggleExcludeSystemDirs: () => {},
    onSetScope: () => {},
    scheduleSearch: () => {},
    mode: 'filename',
    query: '',
    aiPattern: null,
    onFocusBar: () => {},
    ...overrides,
  }
}

describe('SearchFilterChips a11y', () => {
  it('default state (no filters configured) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchFilterChips, { target, props: baseProps() })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('configured chips (size, date, scope) have no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchFilterChips, {
      target,
      props: baseProps({
        sizeFilter: 'between',
        sizeValue: '10',
        sizeUnit: 'MB',
        sizeValueMax: '500',
        sizeUnitMax: 'MB',
        dateFilter: 'between',
        dateValue: '2026-01-01',
        dateValueMax: '2026-03-31',
        scope: '~/Documents, !node_modules',
      }),
    })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('disabled state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchFilterChips, { target, props: baseProps({ disabled: true }) })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('open size popover has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchFilterChips, { target, props: baseProps({ sizeFilter: 'gte', sizeValue: '100' }) })
    await tick()
    const sizeChip = Array.from(target.querySelectorAll<HTMLButtonElement>('.filter-chip')).find((c) =>
      c.textContent?.trim().startsWith('Size'),
    )
    sizeChip?.click()
    await tick()
    // The popover renders alongside the chip strip; pass the document body to cover both subtrees.
    await expectNoA11yViolations(document.body)
    target.remove()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })

  it('open scope popover has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchFilterChips, { target, props: baseProps() })
    await tick()
    const scopeChip = Array.from(target.querySelectorAll<HTMLButtonElement>('.filter-chip')).find((c) =>
      c.textContent?.trim().startsWith('Search in'),
    )
    scopeChip?.click()
    await tick()
    await expectNoA11yViolations(document.body)
    target.remove()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })
})
