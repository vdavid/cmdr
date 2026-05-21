/**
 * Tier 3 a11y tests for `SearchInputArea.svelte`.
 *
 * Post-M2, this component owns the scope row + filter row only. The pattern input moved into
 * `SearchBar.svelte` and the AI prompt input was absorbed into the same bar (with the chip row
 * carrying the mode discriminator). M3 will further refactor these rows into filter chips with
 * popovers.
 *
 * Covers default state (filters "any"), an expanded size filter (single bound), "between" size
 * filter (two bounds), a date filter, and the disabled state.
 */

import { describe, it } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'
import { SvelteSet } from 'svelte/reactivity'
import SearchInputArea from './SearchInputArea.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

type Props = ComponentProps<typeof SearchInputArea>

function baseProps(overrides: Partial<Props> = {}): Props {
  return {
    caseSensitive: false,
    scope: '',
    excludeSystemDirs: true,
    currentFolderPath: '/Users/test',
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
    onSelect: () => () => {},
    onToggleCaseSensitive: () => {},
    onToggleExcludeSystemDirs: () => {},
    onSetScope: () => {},
    scheduleSearch: () => {},
    ...overrides,
  }
}

describe('SearchInputArea a11y', () => {
  it('default (all filters "any") has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchInputArea, { target, props: baseProps() })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('size filter "gte" (one bound) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchInputArea, {
      target,
      props: baseProps({
        sizeFilter: 'gte',
        sizeValue: '10',
        sizeUnit: 'MB',
      }),
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('size filter "between" (two bounds) + date filter has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchInputArea, {
      target,
      props: baseProps({
        scope: '~/Documents, !node_modules',
        sizeFilter: 'between',
        sizeValue: '1',
        sizeUnit: 'MB',
        sizeValueMax: '50',
        sizeUnitMax: 'MB',
        dateFilter: 'between',
        dateValue: '2026-01-01',
        dateValueMax: '2026-03-31',
      }),
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchInputArea, {
      target,
      props: baseProps({ disabled: true }),
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('case-sensitive toggle on has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchInputArea, {
      target,
      props: baseProps({ caseSensitive: true }),
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
