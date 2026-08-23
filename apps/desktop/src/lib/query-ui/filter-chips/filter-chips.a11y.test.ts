/**
 * Tier 3 a11y tests for the filter chip strip and its three popovers.
 *
 * One file per component would cost about four times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, props, and
 * assertions; `makeProps` stays inside its block because all three popovers define
 * a different one. Nothing here mocks a module, so the blocks share only the
 * document cleanup.
 */

import { describe, it, expect, afterEach } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'
import { SvelteSet } from 'svelte/reactivity'
import axe from 'axe-core'
import DateFilterPopover from './DateFilterPopover.svelte'
import SearchFilterChips from './FilterChips.svelte'
import ScopeFilterPopover from './ScopeFilterPopover.svelte'
import SizeFilterPopover from './SizeFilterPopover.svelte'
import { searchQueryState } from '$lib/search/search-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

// Several blocks audit the whole `document.body` (popovers portal out of their
// container), and axe resolves ARIA id references document-wide. Clearing between
// tests keeps each audit looking at its own render only.
afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * Tier-3 a11y tests for `FilterChips.svelte`.
 *
 * Covers the chip strip in default and configured states plus the open popovers (size, modified,
 * scope). Popovers are dialogs (`role="dialog"`); the chip is `aria-haspopup="dialog"` with an
 * `aria-expanded` reflecting the open state.
 *
 * Tier 3 = jsdom + axe-core for structural a11y. Color contrast is checked at design time by
 * `scripts/check-a11y-contrast/` (tier 1). Full-page focus-trap and Escape-return checks live in
 * Playwright (tier 2).
 */
describe('SearchFilterChips a11y', () => {
  type Props = ComponentProps<typeof SearchFilterChips>

  // Reuse Search's own core state instance so the a11y rendering matches what Search ships
  // (no separate test-only state to drift from the real wiring).
  const testState = searchQueryState

  function baseProps(overrides: Partial<Props> = {}): Props {
    return {
      filterState: testState,
      caseSensitive: false,
      scope: '',
      excludeSystemDirs: true,
      scopePresets: { currentFolder: '/Users/test', currentFolderUnavailableReason: '', volumeRoot: '/' },
      defaultScope: { path: '/Users/test', label: 'Current folder' },
      sizeFilter: 'any',
      sizeValue: '',
      sizeUnit: 'MB',
      sizeValueMax: '',
      sizeUnitMax: 'MB',
      dateFilter: 'any',
      dateValue: '',
      dateValueMax: '',
      typeFilter: 'both',
      systemDirExcludeTooltip: 'Excluded: <code>node_modules</code>, <code>.git</code>',
      highlightedFields: new SvelteSet<string>(),
      disabled: false,
      onInput: () => () => {},
      onToggleCaseSensitive: () => {},
      onToggleExcludeSystemDirs: () => {},
      onSetScope: () => {},
      onClearAiPattern: () => {},
      scheduleSearch: () => {},
      mode: 'filename',
      query: '',
      aiPattern: null,
      onFocusBar: () => {},
      ...overrides,
    }
  }

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

  it('type toggle set to Folders has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchFilterChips, { target, props: baseProps({ typeFilter: 'folder' }) })
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
    const sizeChip = Array.from(target.querySelectorAll<HTMLButtonElement>('.chip-filter')).find((c) =>
      c.textContent.trim().startsWith('Size'),
    )
    sizeChip?.click()
    await tick()
    // The popover renders alongside the chip strip; pass the document body to cover both subtrees.
    await expectNoA11yViolations(document.body)
    target.remove()
    document.querySelectorAll('.ui-popover').forEach((el) => {
      el.remove()
    })
  })

  it('open scope popover has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchFilterChips, { target, props: baseProps() })
    await tick()
    const scopeChip = Array.from(target.querySelectorAll<HTMLButtonElement>('.chip-filter')).find((c) =>
      c.textContent.trim().startsWith('Search in'),
    )
    scopeChip?.click()
    await tick()
    await expectNoA11yViolations(document.body)
    target.remove()
    document.querySelectorAll('.ui-popover').forEach((el) => {
      el.remove()
    })
  })
})

/**
 * Tier-3 a11y tests for `DateFilterPopover.svelte`.
 *
 * Covers the closed state (renders nothing) and the open state in `between` mode with custom
 * (non-preset) bounds, which renders every column plus both inline `<input type="date">` custom
 * cells. The anchor is provided as a real button in the test DOM so the popover shell has
 * something to position against.
 */
describe('DateFilterPopover a11y', () => {
  function makeProps(overrides: Record<string, unknown> = {}) {
    const anchor = document.createElement('button')
    anchor.textContent = 'Modified'
    return {
      anchor,
      open: false,
      onClose: () => {},
      dateFilter: 'any' as const,
      dateValue: '',
      dateValueMax: '',
      setDateFilter: () => {},
      setDateValue: () => {},
      setDateValueMax: () => {},
      onInput: () => () => {},
      scheduleSearch: () => {},
      ...overrides,
    }
  }

  it('closed (open=false) renders nothing and has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const props = makeProps()
    target.appendChild(props.anchor)
    mount(DateFilterPopover, { target, props })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('open in preset mode (after) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const props = makeProps({ open: true, dateFilter: 'after' as const })
    target.appendChild(props.anchor)
    mount(DateFilterPopover, { target, props })
    await tick()
    await expectNoA11yViolations(document.body)
    target.remove()
    document.querySelectorAll('.ui-popover').forEach((el) => {
      el.remove()
    })
  })

  // The Custom… cell renders its `<input type="date">` INSIDE the cell button (one click
  // selects + focuses — see `filter-chips/CLAUDE.md` § "Chip-side behavior"). Axe's
  // `nested-interactive` rule flags that structural nesting; we disable that one rule for
  // this state and let every other rule run, mirroring the `SearchResults` block of
  // `../dialog.a11y.test.ts`.
  it('open in between mode with custom bounds has no a11y violations (nested-interactive intentionally disabled)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    // 2020-01-02 / 2021-03-04 match no dynamic preset, so both Custom cells render their inline inputs.
    const props = makeProps({
      open: true,
      dateFilter: 'between' as const,
      dateValue: '2020-01-02',
      dateValueMax: '2021-03-04',
    })
    target.appendChild(props.anchor)
    mount(DateFilterPopover, { target, props })
    await tick()
    const out = await axe.run(document.body, {
      runOnly: {
        type: 'tag',
        values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa', 'best-practice'],
      },
      rules: {
        'color-contrast': { enabled: false },
        region: { enabled: false },
        // Intentional: the custom date input lives inside the Custom cell button.
        // See block comment above.
        'nested-interactive': { enabled: false },
      },
    })
    expect(out.violations).toEqual([])
    target.remove()
    document.querySelectorAll('.ui-popover').forEach((el) => {
      el.remove()
    })
  })
})

/**
 * Tier-3 a11y tests for `ScopeFilterPopover.svelte`.
 *
 * Covers the closed state (renders nothing) and the open state with a populated scope, both
 * toggles, and an enabled "Use current folder" footer button. The anchor is provided as a real
 * button in the test DOM so the popover shell has something to position against.
 */
describe('ScopeFilterPopover a11y', () => {
  function makeProps(overrides: Record<string, unknown> = {}) {
    const anchor = document.createElement('button')
    anchor.textContent = 'Search in'
    return {
      anchor,
      open: false,
      onClose: () => {},
      scope: '',
      excludeSystemDirs: true,
      caseSensitive: false,
      scopePresets: { currentFolder: '/Users/test', currentFolderUnavailableReason: '', volumeRoot: '/' },
      defaultScopePath: '/Users/test',
      systemDirExcludeTooltip: 'Excludes system folders',
      onInput: () => () => {},
      onSetScope: () => {},
      onToggleCaseSensitive: () => {},
      onToggleExcludeSystemDirs: () => {},
      scheduleSearch: () => {},
      ...overrides,
    }
  }

  it('closed (open=false) renders nothing and has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const props = makeProps()
    target.appendChild(props.anchor)
    mount(ScopeFilterPopover, { target, props })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('open with scope text and both toggles has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const props = makeProps({
      open: true,
      scope: '/Users/test/Documents\n!/Users/test/Documents/archive',
      caseSensitive: true,
    })
    target.appendChild(props.anchor)
    mount(ScopeFilterPopover, { target, props })
    await tick()
    await expectNoA11yViolations(document.body)
    target.remove()
    document.querySelectorAll('.ui-popover').forEach((el) => {
      el.remove()
    })
  })
})

/**
 * Tier-3 a11y tests for `SizeFilterPopover.svelte`.
 *
 * Covers the closed state (renders nothing) and the open state in `between` mode, which renders
 * every column (comparator, lower value + unit, upper value + unit). The anchor is provided as a
 * real button in the test DOM so the popover shell has something to position against.
 */
describe('SizeFilterPopover a11y', () => {
  function makeProps(overrides: Record<string, unknown> = {}) {
    const anchor = document.createElement('button')
    anchor.textContent = 'Size'
    return {
      anchor,
      open: false,
      onClose: () => {},
      sizeFilter: 'any' as const,
      sizeValue: '',
      sizeUnit: 'MB' as const,
      sizeValueMax: '',
      sizeUnitMax: 'MB' as const,
      setSizeFilter: () => {},
      setSizeValue: () => {},
      setSizeUnit: () => {},
      setSizeValueMax: () => {},
      setSizeUnitMax: () => {},
      onInput: () => () => {},
      scheduleSearch: () => {},
      ...overrides,
    }
  }

  it('closed (open=false) renders nothing and has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const props = makeProps()
    target.appendChild(props.anchor)
    mount(SizeFilterPopover, { target, props })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('open in between mode (all columns) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const props = makeProps({
      open: true,
      sizeFilter: 'between' as const,
      sizeValue: '5',
      sizeUnit: 'MB' as const,
      sizeValueMax: '10',
      sizeUnitMax: 'GB' as const,
    })
    target.appendChild(props.anchor)
    mount(SizeFilterPopover, { target, props })
    await tick()
    await expectNoA11yViolations(document.body)
    target.remove()
    document.querySelectorAll('.ui-popover').forEach((el) => {
      el.remove()
    })
  })
})
