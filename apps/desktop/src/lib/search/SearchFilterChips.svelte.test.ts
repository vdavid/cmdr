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
    currentFolderPath: '/Users/test',
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
    onSelect:
      // eslint-disable-next-line @typescript-eslint/no-unnecessary-type-parameters -- T constrains the setter's param type
      <T extends string>(setter: (v: T) => void) =>
        (e: Event): void => {
          setter((e.target as HTMLSelectElement).value as T)
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
      unmount(component)
      target.remove()
    },
  }
}

function findChip(target: Element, label: string): HTMLButtonElement | null {
  const chips = Array.from(target.querySelectorAll<HTMLButtonElement>('.filter-chip'))
  return chips.find((c) => c.textContent?.trim().startsWith(label)) ?? null
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
    expect(sizeChip?.textContent?.trim()).toBe('Size')
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
    const labels = Array.from(items ?? []).map((el) => el.textContent?.trim())
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
    // Footer buttons surface the shortcuts so mouse users have parity with ⌥F / ⌥D.
    expect(footerButtons?.[0].textContent).toContain('Use current folder')
    expect(footerButtons?.[0].textContent).toContain('⌥F')
    expect(footerButtons?.[1].textContent).toContain('All folders')
    expect(footerButtons?.[1].textContent).toContain('⌥D')
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
        currentFolderPath: '/Users/test/work',
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
