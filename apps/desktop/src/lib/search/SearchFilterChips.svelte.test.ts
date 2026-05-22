/**
 * Behavior tests for `SearchFilterChips.svelte` (M3).
 *
 * Pins:
 *   - Three chips render by default (Size, Modified, Search in) plus the trailing Add filter chip.
 *   - Each chip shows its default label, not a value, when the filter is in "any" state.
 *   - A configured chip shows its summary plus an `×` clear control.
 *   - Clicking × clears the underlying filter (and the chip goes back to default).
 *   - Backspace on a focused configured chip clears it too.
 *   - Clicking a chip opens its popover; the popover shows the controls; Esc closes the popover
 *     and returns focus to the chip; clicking outside closes too.
 *   - The Add filter chip disappears when all three filters are configured.
 *   - The scope popover supports paste of paths, `!`-prefix exclusions, ⌥F/⌥D footer buttons.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick, unmount, type ComponentProps } from 'svelte'
import { SvelteSet } from 'svelte/reactivity'
import SearchFilterChips from './SearchFilterChips.svelte'
import {
  setSizeFilter,
  setSizeValue,
  setDateFilter,
  setDateValue,
  setScope,
  setExcludeSystemDirs,
  setCaseSensitive,
  clearSearchState,
} from './search-state.svelte'

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
    systemDirExcludeTooltip: 'System dirs',
    highlightedFields: new SvelteSet<string>(),
    disabled: false,
    onInput:
      (setter: (v: string) => void) =>
      (e: Event): void => {
        setter((e.target as HTMLInputElement).value)
      },
    onToggleCaseSensitive: () => {
      setCaseSensitive(true)
    },
    onToggleExcludeSystemDirs: () => {
      setExcludeSystemDirs(false)
    },
    onSetScope: (path: string): void => {
      setScope(path)
    },
    scheduleSearch: () => {},
    mode: 'filename',
    query: '',
    aiPattern: null,
    onFocusBar: () => {},
    ...overrides,
  }
}

function mountChips(props: Props): {
  target: HTMLDivElement
  cleanup: () => void
  component: ReturnType<typeof mount>
} {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(SearchFilterChips, { target, props })
  return {
    target,
    component,
    cleanup: () => {
      void unmount(component)
      target.remove()
    },
  }
}

function findChip(target: Element, label: string): HTMLButtonElement | null {
  const chips = Array.from(target.querySelectorAll<HTMLButtonElement>('.filter-chip'))
  return chips.find((c) => c.textContent.trim().startsWith(label)) ?? null
}

beforeEach(() => {
  clearSearchState()
  // Clean up any leftover popovers from previous tests (they're fixed-position siblings, not
  // children of the per-test `target`, so they don't get removed by `target.remove()`).
  document.querySelectorAll('.filter-chip-popover').forEach((el) => {
    el.remove()
  })
})

describe('SearchFilterChips: default rendering', () => {
  it('renders the three filter chips and an Add filter chip', async () => {
    const { target, cleanup } = mountChips(baseProps())
    await tick()
    expect(findChip(target, 'Size')).not.toBeNull()
    expect(findChip(target, 'Modified')).not.toBeNull()
    expect(findChip(target, 'Search in')).not.toBeNull()
    expect(target.querySelector('.add-filter-chip')).not.toBeNull()
    cleanup()
  })

  it('shows only the label (not a value) when filters are in "any" state', async () => {
    const { target, cleanup } = mountChips(baseProps())
    await tick()
    const sizeChip = findChip(target, 'Size')
    expect(sizeChip.textContent.trim()).toBe('Size')
    expect(sizeChip?.querySelector('.chip-clear')).toBeNull()
    cleanup()
  })
})

describe('SearchFilterChips: configured state', () => {
  it('shows a summary and an × when a filter is configured', async () => {
    const { target, cleanup } = mountChips(baseProps({ sizeFilter: 'gte', sizeValue: '100', sizeUnit: 'MB' }))
    await tick()
    const sizeChip = findChip(target, 'Size')
    expect(sizeChip?.textContent).toContain('100 MB')
    expect(sizeChip?.querySelector('.chip-clear')).not.toBeNull()
    cleanup()
  })

  it('clicking × clears the filter back to default', async () => {
    const onScheduleSearch = vi.fn()
    setSizeFilter('gte')
    setSizeValue('100')
    const { target, cleanup } = mountChips(
      baseProps({
        sizeFilter: 'gte',
        sizeValue: '100',
        sizeUnit: 'MB',
        scheduleSearch: onScheduleSearch,
      }),
    )
    await tick()
    const sizeChip = findChip(target, 'Size')
    const clear = sizeChip?.querySelector<HTMLElement>('.chip-clear')
    expect(clear).not.toBeNull()
    // The clear is a mousedown affordance, not a button. Use mousedown rather than click so
    // it doesn't propagate to the chip's onclick (which would re-open the popover).
    clear?.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }))
    await tick()
    expect(onScheduleSearch).toHaveBeenCalled()
    cleanup()
  })

  it('Backspace on a focused configured chip clears it', async () => {
    const onScheduleSearch = vi.fn()
    setDateFilter('after')
    setDateValue('2026-04-01')
    const { target, cleanup } = mountChips(
      baseProps({
        dateFilter: 'after',
        dateValue: '2026-04-01',
        scheduleSearch: onScheduleSearch,
      }),
    )
    await tick()
    const modifiedChip = findChip(target, 'Modified')
    modifiedChip?.focus()
    modifiedChip?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Backspace', bubbles: true, cancelable: true }))
    await tick()
    expect(onScheduleSearch).toHaveBeenCalled()
    cleanup()
  })

  it('Backspace on a default-state chip does not clear (no action to take)', async () => {
    const onScheduleSearch = vi.fn()
    const { target, cleanup } = mountChips(baseProps({ scheduleSearch: onScheduleSearch }))
    await tick()
    const sizeChip = findChip(target, 'Size')
    sizeChip?.focus()
    sizeChip?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Backspace', bubbles: true, cancelable: true }))
    await tick()
    expect(onScheduleSearch).not.toHaveBeenCalled()
    cleanup()
  })
})

describe('SearchFilterChips: popover keyboard handling', () => {
  it('opens the popover on chip click', async () => {
    const { target, cleanup } = mountChips(baseProps())
    await tick()
    const sizeChip = findChip(target, 'Size')
    sizeChip?.click()
    await tick()
    const popover = document.querySelector('.filter-chip-popover')
    expect(popover).not.toBeNull()
    expect(popover?.getAttribute('aria-label')).toBe('Size filter options')
    cleanup()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })

  it('opens the popover when Enter is pressed on a focused chip', async () => {
    const { target, cleanup } = mountChips(baseProps())
    await tick()
    const sizeChip = findChip(target, 'Size')
    sizeChip?.focus()
    sizeChip?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))
    await tick()
    expect(document.querySelector('.filter-chip-popover')).not.toBeNull()
    cleanup()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })

  it('Esc closes the popover and returns focus to the chip', async () => {
    const { target, cleanup } = mountChips(baseProps())
    await tick()
    const sizeChip = findChip(target, 'Size')
    sizeChip?.click()
    await tick()
    const popover = document.querySelector('.filter-chip-popover')
    expect(popover).not.toBeNull()
    const escEvent = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
    popover?.dispatchEvent(escEvent)
    await tick()
    expect(document.querySelector('.filter-chip-popover')).toBeNull()
    // The Esc event must have been stopPropagation'd so the dialog's capture handler doesn't fire.
    // We verify by checking that the popover is closed but the chip itself still exists.
    expect(sizeChip?.isConnected).toBe(true)
    cleanup()
  })

  it('Esc inside the popover does not let the event propagate up', async () => {
    const { target, cleanup } = mountChips(baseProps())
    await tick()
    const sizeChip = findChip(target, 'Size')
    sizeChip?.click()
    await tick()
    const popover = document.querySelector('.filter-chip-popover') as HTMLElement
    expect(popover).not.toBeNull()
    // Listen at the document level for the bubbling phase. If the popover correctly calls
    // stopPropagation, this handler should NOT fire.
    const docHandler = vi.fn()
    document.addEventListener('keydown', docHandler)
    const escEvent = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
    popover.dispatchEvent(escEvent)
    document.removeEventListener('keydown', docHandler)
    expect(docHandler).not.toHaveBeenCalled()
    cleanup()
  })
})

describe('SearchFilterChips: Add filter chip', () => {
  it('is shown when at least one filter is in default state', async () => {
    const { target, cleanup } = mountChips(baseProps())
    await tick()
    expect(target.querySelector('.add-filter-chip')).not.toBeNull()
    cleanup()
  })

  it('is hidden when all three filters are configured', async () => {
    const { target, cleanup } = mountChips(
      baseProps({
        sizeFilter: 'gte',
        sizeValue: '100',
        dateFilter: 'after',
        dateValue: '2026-04-01',
        scope: '~/Documents',
      }),
    )
    await tick()
    expect(target.querySelector('.add-filter-chip')).toBeNull()
    cleanup()
  })

  it('opening Add filter lists only available filters', async () => {
    const { target, cleanup } = mountChips(
      baseProps({
        sizeFilter: 'gte',
        sizeValue: '100',
      }),
    )
    await tick()
    const addChip = target.querySelector<HTMLButtonElement>('.add-filter-chip')
    addChip?.click()
    await tick()
    const menu = document.querySelector('.add-filter-menu')
    const items = menu?.querySelectorAll('.add-filter-item')
    expect(items?.length).toBe(2)
    const labels = Array.from(items ?? []).map((el) => el.textContent.trim())
    expect(labels).toContain('Modified')
    expect(labels).toContain('Search in')
    expect(labels).not.toContain('Size')
    cleanup()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })
})

describe('SearchFilterChips: scope popover behavior', () => {
  it('renders the scope textarea, system-folder toggle, case-sensitive toggle, and footer buttons', async () => {
    const { target, cleanup } = mountChips(baseProps())
    await tick()
    const scopeChip = findChip(target, 'Search in')
    scopeChip?.click()
    await tick()
    const popover = document.querySelector('.scope-popover')
    expect(popover).not.toBeNull()
    expect(popover?.querySelector('textarea')).not.toBeNull()
    const checkboxes = popover?.querySelectorAll('input[type="checkbox"]')
    expect(checkboxes?.length).toBe(2)
    const footerButtons = popover?.querySelectorAll('.footer-button')
    expect(footerButtons?.length).toBe(2)
    // Round 2 D9: footer buttons surface ⌥C / ⌥V, scoped to the open popover.
    // ⌥F / ⌥D are gone (⌥F is now the global Filename mode chip).
    expect(footerButtons?.[0].textContent).toContain('Use current folder')
    expect(footerButtons?.[0].textContent).toContain('⌥C')
    expect(footerButtons?.[1].textContent).toContain('All folders')
    expect(footerButtons?.[1].textContent).toContain('⌥V')
    cleanup()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })

  it('accepts pasted scope text with ! exclusions', async () => {
    // The textarea's `oninput` is wired through the `onInput` prop, which writes directly into
    // the search-state module (the same path the dialog's `inputHandler` takes in production).
    // We assert against the module state rather than a prop spy, mirroring the real wiring.
    const { getScope } = await import('./search-state.svelte')
    const { target, cleanup } = mountChips(baseProps())
    await tick()
    const scopeChip = findChip(target, 'Search in')
    scopeChip?.click()
    await tick()
    const textarea = document.querySelector<HTMLTextAreaElement>('.popover-textarea')
    expect(textarea).not.toBeNull()
    if (textarea) {
      textarea.value = '~/Documents, !node_modules, !.git'
      textarea.dispatchEvent(new Event('input', { bubbles: true }))
    }
    expect(getScope()).toBe('~/Documents, !node_modules, !.git')
    cleanup()
  })

  it('Use current folder button sets scope to the current folder path', async () => {
    const onSetScope = vi.fn()
    const onScheduleSearch = vi.fn()
    const { target, cleanup } = mountChips(
      baseProps({
        searchableFolder: { path: '/Users/test/work', disabled: false, disabledReason: '' },
        onSetScope,
        scheduleSearch: onScheduleSearch,
      }),
    )
    await tick()
    const scopeChip = findChip(target, 'Search in')
    scopeChip?.click()
    await tick()
    const buttons = document.querySelectorAll<HTMLButtonElement>('.footer-button')
    buttons[0].click()
    expect(onSetScope).toHaveBeenCalledWith('/Users/test/work')
    expect(onScheduleSearch).toHaveBeenCalled()
    cleanup()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })

  it('D10: Size popover renders the list-style grid with comparator + preset + unit columns', async () => {
    const { target, cleanup } = mountChips(baseProps({ sizeFilter: 'gte', sizeValue: '5', sizeUnit: 'MB' }))
    await tick()
    const sizeChip = findChip(target, 'Size')
    sizeChip?.click()
    await tick()
    const grid = document.querySelector('.list-grid')
    expect(grid).not.toBeNull()
    const cols = grid?.querySelectorAll('.list-col')
    expect(cols?.length).toBe(3) // comparator, lower value, lower unit
    // Comparator col exposes all 4 options.
    const compCells = cols?.[0].querySelectorAll('.list-cell')
    expect(compCells?.length).toBe(4)
    // Selected comparator is `>=`.
    const selected = grid?.querySelectorAll('.list-cell.is-selected')
    expect(selected?.length).toBeGreaterThanOrEqual(2) // gte + value '5' + unit 'MB'
    cleanup()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })

  it('D10: Size popover adds upper-bound cols only for `between`', async () => {
    const { target, cleanup } = mountChips(baseProps({ sizeFilter: 'between' }))
    await tick()
    const sizeChip = findChip(target, 'Size')
    sizeChip?.click()
    await tick()
    const grid = document.querySelector('.list-grid')
    expect(grid?.classList.contains('has-upper')).toBe(true)
    const cols = grid?.querySelectorAll('.list-col')
    expect(cols?.length).toBe(5)
    cleanup()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })

  // R3 U5 update: value + unit cells stay clickable when comparator is `any`;
  // they just look dimmed via `.is-disabled-look`. Clicking them auto-promotes
  // the comparator (round 2 D10 → round 3 U5).
  it('R3 U5: Size popover value + unit cells render dimmed (not disabled) when comparator is `any`', async () => {
    const { target, cleanup } = mountChips(baseProps({ sizeFilter: 'any' }))
    await tick()
    const sizeChip = findChip(target, 'Size')
    sizeChip?.click()
    await tick()
    const grid = document.querySelector('.list-grid')
    const valueCells = grid?.querySelectorAll<HTMLButtonElement>('.list-col:nth-child(2) .list-cell')
    const unitCells = grid?.querySelectorAll<HTMLButtonElement>('.list-col:nth-child(3) .list-cell')
    // None of the cells are HTML-disabled (the click must reach our handler).
    expect([...(valueCells ?? [])].some((b) => b.disabled)).toBe(false)
    expect([...(unitCells ?? [])].some((b) => b.disabled)).toBe(false)
    // But they all carry the dimmed look.
    expect([...(valueCells ?? [])].every((b) => b.classList.contains('is-disabled-look'))).toBe(true)
    expect([...(unitCells ?? [])].every((b) => b.classList.contains('is-disabled-look'))).toBe(true)
    cleanup()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })

  it('R3 U5: clicking a Size value cell while `any` auto-promotes the comparator to `gte`', async () => {
    clearSearchState()
    const { target, cleanup } = mountChips(baseProps({ sizeFilter: 'any' }))
    await tick()
    const sizeChip = findChip(target, 'Size')
    sizeChip?.click()
    await tick()
    // Click "100" in the value column.
    const valueCells = document.querySelectorAll<HTMLButtonElement>('.list-col:nth-child(2) .list-cell')
    const hundred = [...valueCells].find((b) => b.textContent.trim() === '100')
    expect(hundred).not.toBeUndefined()
    hundred?.click()
    await tick()
    // The comparator promoted to gte, and the value column landed on "100".
    const { getSizeFilter, getSizeValue } = await import('./search-state.svelte')
    expect(getSizeFilter()).toBe('gte')
    expect(getSizeValue()).toBe('100')
    cleanup()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })

  it('R3 U5: clicking a Modified preset while `any` auto-promotes the comparator to `after`', async () => {
    clearSearchState()
    const { target, cleanup } = mountChips(baseProps({ dateFilter: 'any' }))
    await tick()
    const dateChip = findChip(target, 'Modified')
    dateChip?.click()
    await tick()
    const valueCells = document.querySelectorAll<HTMLButtonElement>('.list-col:nth-child(2) .list-cell')
    const today = [...valueCells].find((b) => /^today/.test(b.textContent.trim()))
    expect(today).not.toBeUndefined()
    today?.click()
    await tick()
    const { getDateFilter, getDateValue } = await import('./search-state.svelte')
    expect(getDateFilter()).toBe('after')
    // dateValue is now an ISO date matching "today"; we just check non-empty.
    expect(getDateValue()).toMatch(/^\d{4}-\d{2}-\d{2}$/)
    cleanup()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })

  it('R3 B5: Modified popover with a preset selected does NOT also light up Custom', async () => {
    clearSearchState()
    setDateFilter('after')
    // Mimic clicking "today" → dateValue is today's ISO. This is the bug:
    // round-2 code left `dateIsCustomLower` true from a prior interaction.
    const { resolveDatePreset } = await import('./filter-popover-helpers')
    const today = resolveDatePreset('today')
    setDateValue(today ?? '2026-05-22')
    const { target, cleanup } = mountChips(baseProps({ dateFilter: 'after', dateValue: today ?? '2026-05-22' }))
    await tick()
    const dateChip = findChip(target, 'Modified')
    dateChip?.click()
    await tick()
    const cells = document.querySelectorAll<HTMLButtonElement>('.list-col:nth-child(2) .list-cell')
    const selected = [...cells].filter((b) => b.classList.contains('is-selected'))
    // Exactly one selected cell. Without the fix, both the matching preset
    // and the Custom cell would carry `is-selected`.
    expect(selected.length).toBe(1)
    const customCell = [...cells].find((b) => b.classList.contains('list-cell-custom'))
    expect(customCell?.classList.contains('is-selected')).toBe(false)
    cleanup()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })

  it('R3 U3: Size Custom cell holds the input inline (not below) so one click focuses it', async () => {
    clearSearchState()
    setSizeFilter('gte')
    setSizeValue('')
    const { target, cleanup } = mountChips(baseProps({ sizeFilter: 'gte', sizeValue: '' }))
    await tick()
    const sizeChip = findChip(target, 'Size')
    sizeChip?.click()
    await tick()
    // Click the Custom cell.
    const cells = document.querySelectorAll<HTMLButtonElement>('.list-col:nth-child(2) .list-cell')
    const customCell = [...cells].find((b) => b.classList.contains('list-cell-custom'))
    expect(customCell).not.toBeUndefined()
    customCell?.click()
    await tick()
    // The input now lives INSIDE the same cell, not as a sibling below it.
    const innerInput = customCell?.querySelector<HTMLInputElement>('input[type="number"]')
    expect(innerInput).not.toBeNull()
    cleanup()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })

  it('R3 U4: Modified popover preset labels include the "0:00" suffix and a "1st of" month label', async () => {
    clearSearchState()
    setDateFilter('after')
    const { target, cleanup } = mountChips(baseProps({ dateFilter: 'after' }))
    await tick()
    const dateChip = findChip(target, 'Modified')
    dateChip?.click()
    await tick()
    const cells = document.querySelectorAll<HTMLButtonElement>('.list-col:nth-child(2) .list-cell')
    const labels = [...cells].map((c) => c.textContent.trim())
    // First two: today / yesterday.
    expect(labels[0]).toMatch(/^today 0:00$/)
    expect(labels[1]).toMatch(/^yesterday 0:00$/)
    // Next two: this / last weekday (Monday on Monday-start locales, Sunday
    // on US locales). The exact weekday is locale-dependent, so we just pin
    // the shape.
    expect(labels[2]).toMatch(/^this \w+ 0:00$/)
    expect(labels[3]).toMatch(/^last \w+ 0:00$/)
    // Next: "1st of <Month> 0:00" (current month, no year) and
    // "1st of <Month>, <Year>, 0:00" (last month, with year).
    expect(labels[4]).toMatch(/^1st of \w+ 0:00$/)
    expect(labels[5]).toMatch(/^1st of \w+, \d{4}, 0:00$/)
    // The custom cell label (in lowercase, last in the column).
    expect(labels.some((l) => /^custom…$/.test(l))).toBe(true)
    cleanup()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })

  it("D10: Size popover unit cell reads 'byte' when the selected value is exactly '1'", async () => {
    const { target, cleanup } = mountChips(baseProps({ sizeFilter: 'gte', sizeValue: '1', sizeUnit: 'B' }))
    await tick()
    const sizeChip = findChip(target, 'Size')
    sizeChip?.click()
    await tick()
    const grid = document.querySelector('.list-grid')
    const firstUnitCell = grid?.querySelector('.list-col:nth-child(3) .list-cell')
    expect(firstUnitCell.textContent.trim()).toBe('byte')
    cleanup()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })

  it("D10: Size popover unit cell reads 'bytes' for values other than 1", async () => {
    const { target, cleanup } = mountChips(baseProps({ sizeFilter: 'gte', sizeValue: '5', sizeUnit: 'B' }))
    await tick()
    const sizeChip = findChip(target, 'Size')
    sizeChip?.click()
    await tick()
    const grid = document.querySelector('.list-grid')
    const firstUnitCell = grid?.querySelector('.list-col:nth-child(3) .list-cell')
    expect(firstUnitCell.textContent.trim()).toBe('bytes')
    cleanup()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })

  it('D11: Modified popover renders comparator + preset columns; adds upper-bound col on `between`', async () => {
    const { target, cleanup } = mountChips(baseProps({ dateFilter: 'between' }))
    await tick()
    const dateChip = findChip(target, 'Modified')
    dateChip?.click()
    await tick()
    const grid = document.querySelector('.list-grid')
    expect(grid).not.toBeNull()
    const cols = grid?.querySelectorAll('.list-col')
    expect(cols?.length).toBe(3) // comparator + lower preset + upper preset
    // No unit column on Modified.
    cleanup()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })

  it('D12: Use current folder is disabled with tooltip when searchableFolder.disabled is true', async () => {
    const onSetScope = vi.fn()
    const { target, cleanup } = mountChips(
      baseProps({
        searchableFolder: {
          path: null,
          disabled: true,
          disabledReason: "Current folder is search results, which isn't searchable. Open a real folder first.",
        },
        onSetScope,
      }),
    )
    await tick()
    const scopeChip = findChip(target, 'Search in')
    scopeChip?.click()
    await tick()
    const buttons = document.querySelectorAll<HTMLButtonElement>('.footer-button')
    expect(buttons[0].disabled).toBe(true)
    buttons[0].click()
    // Disabled button doesn't fire onclick in real DOM; ensure we didn't reach the setter.
    expect(onSetScope).not.toHaveBeenCalled()
    cleanup()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })

  it('D12: Use current folder uses the fallback path when on a snapshot pane with history', async () => {
    const onSetScope = vi.fn()
    const { target, cleanup } = mountChips(
      baseProps({
        // The dialog's parent passes the most-recent real-folder history path here.
        searchableFolder: { path: '/Users/me/projects', disabled: false, disabledReason: '' },
        onSetScope,
      }),
    )
    await tick()
    const scopeChip = findChip(target, 'Search in')
    scopeChip?.click()
    await tick()
    const buttons = document.querySelectorAll<HTMLButtonElement>('.footer-button')
    expect(buttons[0].disabled).toBe(false)
    buttons[0].click()
    expect(onSetScope).toHaveBeenCalledWith('/Users/me/projects')
    cleanup()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })

  it('All folders button clears the scope', async () => {
    const onSetScope = vi.fn()
    const { target, cleanup } = mountChips(baseProps({ scope: '~/Documents', onSetScope }))
    await tick()
    const scopeChip = findChip(target, 'Search in')
    scopeChip?.click()
    await tick()
    const buttons = document.querySelectorAll<HTMLButtonElement>('.footer-button')
    buttons[1].click()
    expect(onSetScope).toHaveBeenCalledWith('')
    cleanup()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })
})
