/**
 * Tier 3 a11y tests for `SearchInputArea.svelte`.
 *
 * Pattern row + scope row + filter row. Many controls — input, toggles,
 * selects. Tests cover the default state (filters "any"), an expanded
 * size filter (single bound), a "between" size filter (two bounds),
 * and a date filter.
 */

import { describe, it } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'
import { SvelteSet } from 'svelte/reactivity'
import SearchInputArea from './SearchInputArea.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

type Props = ComponentProps<typeof SearchInputArea>

function baseProps(overrides: Partial<Props> = {}): Props {
  return {
    patternInputElement: undefined,
    namePattern: '',
    patternType: 'glob',
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
    onSearch: () => {},
    onTogglePatternType: () => {},
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
        namePattern: '*.jpg',
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
        namePattern: 'report*',
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
      props: baseProps({
        disabled: true,
        namePattern: 'anything',
      }),
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('regex mode + case-sensitive has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchInputArea, {
      target,
      props: baseProps({
        patternType: 'regex',
        caseSensitive: true,
        namePattern: '^report.*\\.md$',
      }),
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
