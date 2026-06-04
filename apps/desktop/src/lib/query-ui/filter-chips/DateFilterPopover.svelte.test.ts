/**
 * Behavior tests for `DateFilterPopover.svelte`.
 *
 * Pins:
 *   - Clicking a preset cell while comparator = `any` auto-promotes the comparator to `after`,
 *     writes the preset's resolved date, and schedules a search.
 *   - Clicking a preset cell with a non-`any` comparator only writes the value (no promotion).
 *   - Clicking the Custom… cell flags custom mode (the inline `<input type="date">` appears) and
 *     clears the lower bound.
 *   - Clicking a comparator cell writes the comparator and schedules a search.
 *   - `between` mode renders the upper-bound preset column; other comparators don't.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import DateFilterPopover from './DateFilterPopover.svelte'

type Mounted = { component: ReturnType<typeof mount>; target: HTMLElement }

function mountPopover(overrides: Record<string, unknown> = {}): Mounted & {
  setDateFilter: ReturnType<typeof vi.fn>
  setDateValue: ReturnType<typeof vi.fn>
  setDateValueMax: ReturnType<typeof vi.fn>
  scheduleSearch: ReturnType<typeof vi.fn>
} {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const anchor = document.createElement('button')
  anchor.textContent = 'Modified'
  target.appendChild(anchor)
  const setDateFilter = vi.fn()
  const setDateValue = vi.fn()
  const setDateValueMax = vi.fn()
  const scheduleSearch = vi.fn()
  const component = mount(DateFilterPopover, {
    target,
    props: {
      anchor,
      open: true,
      onClose: () => {},
      dateFilter: 'any',
      dateValue: '',
      dateValueMax: '',
      setDateFilter,
      setDateValue,
      setDateValueMax,
      onInput: () => () => {},
      scheduleSearch,
      ...overrides,
    },
  })
  return { component, target, setDateFilter, setDateValue, setDateValueMax, scheduleSearch }
}

function cleanup(mounted: Mounted): void {
  void unmount(mounted.component)
  mounted.target.remove()
  document.querySelectorAll('.filter-chip-popover').forEach((el) => {
    el.remove()
  })
}

function lowerValueCells(): HTMLButtonElement[] {
  const cols = document.querySelectorAll('[role="radiogroup"][aria-label="Date value"]')
  expect(cols.length).toBeGreaterThan(0)
  return Array.from(cols[0].querySelectorAll('button'))
}

describe('DateFilterPopover', () => {
  it('preset click with comparator `any` promotes to `after`, writes the value, and searches', async () => {
    const m = mountPopover()
    await tick()
    const cells = lowerValueCells()
    cells[0].click() // first dynamic preset ("today …")
    await tick()
    expect(m.setDateFilter).toHaveBeenCalledWith('after')
    expect(m.setDateValue).toHaveBeenCalledTimes(1)
    expect(m.scheduleSearch).toHaveBeenCalled()
    cleanup(m)
  })

  it('preset click with comparator `before` writes the value without promotion', async () => {
    const m = mountPopover({ dateFilter: 'before' })
    await tick()
    lowerValueCells()[1].click() // "yesterday …"
    await tick()
    expect(m.setDateFilter).not.toHaveBeenCalled()
    expect(m.setDateValue).toHaveBeenCalledTimes(1)
    cleanup(m)
  })

  it('Custom… click flags custom mode, clears the bound, and shows the inline date input', async () => {
    const m = mountPopover({ dateFilter: 'after' })
    await tick()
    const cells = lowerValueCells()
    const customCell = cells[cells.length - 1]
    expect(document.querySelector('input[type="date"]')).toBeNull()
    customCell.click()
    await tick()
    expect(m.setDateValue).toHaveBeenCalledWith('')
    expect(document.querySelector('input[type="date"]')).not.toBeNull()
    cleanup(m)
  })

  it('comparator click writes the comparator and schedules a search', async () => {
    const m = mountPopover()
    await tick()
    const comparatorCol = document.querySelector('[role="radiogroup"][aria-label="Comparator"]')
    if (!comparatorCol) throw new Error('Comparator column not found')
    const betweenCell = Array.from(comparatorCol.querySelectorAll('button')).find(
      (b) => b.textContent.trim() === 'between',
    )
    if (!betweenCell) throw new Error('between comparator cell not found')
    betweenCell.click()
    await tick()
    expect(m.setDateFilter).toHaveBeenCalledWith('between')
    expect(m.scheduleSearch).toHaveBeenCalled()
    cleanup(m)
  })

  it('`between` renders the upper-bound column; `after` does not', async () => {
    const between = mountPopover({ dateFilter: 'between', dateValue: '2020-01-02', dateValueMax: '2021-03-04' })
    await tick()
    expect(document.querySelector('[role="radiogroup"][aria-label="Maximum date value"]')).not.toBeNull()
    const grid = document.querySelector('.date-grid')
    expect(grid?.classList.contains('has-upper')).toBe(true)
    cleanup(between)

    const after = mountPopover({ dateFilter: 'after' })
    await tick()
    expect(document.querySelector('[role="radiogroup"][aria-label="Maximum date value"]')).toBeNull()
    const grid2 = document.querySelector('.date-grid')
    expect(grid2?.classList.contains('has-upper')).toBe(false)
    cleanup(after)
  })
})
