/**
 * The Full view's column header, mounted on its own.
 *
 * The load-bearing part is which TRACKS it emits: the header and the data rows share
 * one `grid-template-columns`, so a header that renders a cell the rows don't (or vice
 * versa) drifts every column to its right. These pin the two conditional tracks (Git,
 * Ext) against the same conditions `FullList` uses to build the template, plus the
 * scrollbar-width compensation that keeps the header aligned from outside the scroller.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import FullListHeader from './FullListHeader.svelte'
import type { SortColumn } from '../types'
import type { DateColumn } from '../displayed-date'
import type { MenuAnchor } from '#lib/tauri-commands/index.js'
import { movePaneChromeFocus } from '../pane/pane-chrome'

function mountHeader(props: Partial<Parameters<typeof mountHeaderRaw>[0]> = {}) {
  return mountHeaderRaw({
    gridTemplate: '16px 1fr 60px 115px 80px',
    isFocused: true,
    sortBy: 'name',
    sortOrder: 'ascending' as const,
    dateColumn: 'modified' as const,
    dateFallbackNote: undefined,
    showExtensionInName: false,
    showDateColumn: true,
    gitColumnVisible: false,
    skipTransition: false,
    scrollbarWidth: 0,
    ...props,
  })
}

function mountHeaderRaw(props: {
  gridTemplate: string
  isFocused: boolean
  sortBy: SortColumn | null
  sortOrder: 'ascending' | 'descending'
  dateColumn: DateColumn
  dateFallbackNote: string | undefined
  showExtensionInName: boolean
  showDateColumn: boolean
  gitColumnVisible: boolean
  skipTransition: boolean
  scrollbarWidth: number
  clearsSortLabel?: string
  onSortChange?: (column: SortColumn) => void
  onSortMenu?: (anchor: MenuAnchor | null) => Promise<void>
}) {
  const target = document.createElement('div')
  document.body.append(target)
  mount(FullListHeader, { target, props })
  return target
}

/** The column labels, in DOM order. Covers both sort triggers and static labels. */
function labels(target: HTMLElement): (string | null)[] {
  return [...target.querySelectorAll('.sortable-header .label')].map((el) => el.textContent)
}

describe('columns', () => {
  it('renders the default four sort triggers', () => {
    expect(labels(mountHeader())).toEqual(['Name', 'Ext', 'Size', 'Modified'])
  })

  it('adds a Git cell only when the column is on', () => {
    expect(mountHeader().querySelector('.header-git')).toBeNull()
    expect(mountHeader({ gitColumnVisible: true }).querySelector('.header-git')).toBeTruthy()
  })

  it('folds Ext into the Name track when the extension rides in the name', () => {
    const target = mountHeader({ showExtensionInName: true })

    // Same four labels, but Name and Ext now share the single `1fr` track, so the
    // Ext trigger costs the pane no column width.
    expect(labels(target)).toEqual(['Name', 'Ext', 'Size', 'Modified'])
    expect(target.querySelectorAll('.header-name-ext .sortable-header')).toHaveLength(2)
  })

  it('keeps Ext as its own track otherwise', () => {
    const target = mountHeader({ showExtensionInName: false })

    expect(target.querySelector('.header-name-ext')).toBeNull()
  })

  it('drops the date cell, chevron included, with its column', () => {
    const target = mountHeader({ showDateColumn: false, gridTemplate: '16px 1fr 28px 115px' })

    expect(labels(target)).toEqual(['Name', 'Ext', 'Size'])
    expect(target.querySelector('.header-date')).toBeNull()
  })

  it('mirrors the grid template it was handed', () => {
    const target = mountHeader({ gridTemplate: '16px 1fr 28px 115px 80px' })

    expect(target.querySelector('.header-row')?.getAttribute('style')).toContain(
      'grid-template-columns: 16px 1fr 28px 115px 80px',
    )
  })
})

describe('sorting', () => {
  it('marks the sorted column active and leaves the rest alone', () => {
    const target = mountHeader({ sortBy: 'size' as SortColumn })
    const active = [...target.querySelectorAll('.sortable-header')].filter((el) => el.classList.contains('is-active'))

    expect(active).toHaveLength(1)
    expect(active[0].querySelector('.label')?.textContent).toBe('Size')
  })

  it('reports the clicked column', () => {
    const clicked: SortColumn[] = []
    const target = mountHeader({
      onSortChange: (column: SortColumn) => {
        clicked.push(column)
      },
    })

    target.querySelectorAll<HTMLButtonElement>('button.sortable-header').forEach((button) => {
      button.click()
    })

    expect(clicked).toEqual(['name', 'extension', 'size', 'modified'])
  })

  it('stays clickable with no handler wired', () => {
    const target = mountHeader()

    expect(() => {
      target.querySelector<HTMLButtonElement>('button.sortable-header')?.click()
    }).not.toThrow()
  })
})

/**
 * A pane whose rows do NOT follow its sort (`sortable={false}`, today the
 * search-results snapshot pane) must not claim one. The labels and their column
 * tracks stay; the button semantics, the active column, and the direction arrow
 * all go, because a click here would do nothing and the arrow would name an order
 * the rows are not in.
 */
/**
 * A pane in NO column's order: the search-results pane before its header is ever
 * clicked, showing the search engine's ranked rows. Every header still sorts,
 * because a click is what puts the pane in a column's order.
 */
describe('a pane in no column order', () => {
  it('keeps the four column labels', () => {
    expect(labels(mountHeader({ sortBy: null }))).toEqual(['Name', 'Ext', 'Size', 'Modified'])
  })

  it('keeps every header clickable, because a click is what sorts the pane', () => {
    expect(mountHeader({ sortBy: null }).querySelectorAll('button.sortable-header')).toHaveLength(4)
  })

  it('lights no column and draws no direction arrow', () => {
    const target = mountHeader({ sortBy: null })

    expect(target.querySelector('.sortable-header.is-active')).toBeNull()
    expect(target.querySelector('.sort-indicator:not(.invisible)')).toBeNull()
  })

  it('still folds Ext into the Name track when the extension rides in the name', () => {
    const target = mountHeader({ sortBy: null, showExtensionInName: true })

    expect(labels(target)).toEqual(['Name', 'Ext', 'Size', 'Modified'])
    expect(target.querySelectorAll('.header-name-ext .sortable-header')).toHaveLength(2)
  })
})

describe('transition suppression', () => {
  it('drops the width transition for the first paint after a navigation', () => {
    expect(mountHeader({ skipTransition: true }).querySelector('.header-row.no-transition')).toBeTruthy()
    expect(mountHeader({ skipTransition: false }).querySelector('.header-row.no-transition')).toBeNull()
  })
})

/**
 * The header renders OUTSIDE the rows' scroll container (so the scrollbar starts below
 * it), which means it no longer loses the scrollbar's width the way the rows do. It has
 * to re-add that width on the right, or every column drifts under "Always show scroll
 * bars". Overlay scrollbars report 0 and must add nothing.
 */
describe('scrollbar-width compensation', () => {
  /**
   * The width reaches the stylesheet's `padding-right: calc(…)` as a custom property,
   * so this reads the property rather than the resolved padding: jsdom computes no
   * cascade, and its CSS parser drops a `calc()` that contains a `var()` outright.
   */
  function scrollbarVar(target: HTMLElement): string {
    return target.querySelector<HTMLElement>('.header-row')?.style.getPropertyValue('--spacing-scrollbar-width') ?? ''
  }

  it('adds nothing for overlay scrollbars', () => {
    expect(scrollbarVar(mountHeader({ scrollbarWidth: 0 }))).toBe('0px')
  })

  it('reserves the scrollbar width when the scroller has a classic scrollbar', () => {
    expect(scrollbarVar(mountHeader({ scrollbarWidth: 15 }))).toBe('15px')
  })
})

/** The date header's button (the last sort trigger). */
function dateHeader(target: HTMLElement): HTMLButtonElement {
  const buttons = target.querySelectorAll<HTMLButtonElement>('button.sortable-header')
  return buttons[buttons.length - 1]
}

/**
 * The date header sorts by the date the pane ASKED for, so a click flips that sort's
 * order, even in a fallback where the cells show Modified.
 */
describe('date header click', () => {
  it.each<[SortColumn, DateColumn, SortColumn]>([
    ['created', 'created', 'created'],
    ['added', 'added', 'added'],
    // A fallback: Added asked for, Modified shown. The click still flips Added.
    ['added', 'modified', 'added'],
    ['name', 'modified', 'modified'],
    ['size', 'modified', 'modified'],
  ])('sorts a pane asking for %s (showing %s) by %s', (sortBy, dateColumn, expected) => {
    const clicked: SortColumn[] = []
    const target = mountHeader({ sortBy, dateColumn, onSortChange: (column: SortColumn) => clicked.push(column) })

    dateHeader(target).click()

    expect(clicked).toEqual([expected])
  })
})

/** A header whose Sort by menu stays open until the test closes it, recording each open's anchor. */
function mountWithMenu(props: Partial<Parameters<typeof mountHeaderRaw>[0]> = {}) {
  const opens: (MenuAnchor | null)[] = []
  let close: () => void = () => {}
  const target = mountHeader({
    onSortMenu: (anchor: MenuAnchor | null) => {
      opens.push(anchor)
      // The popup blocks in Rust until it closes.
      return new Promise<void>((resolve) => {
        close = resolve
      })
    },
    ...props,
  })
  const chevron = target.querySelector<HTMLButtonElement>('.date-sort-chevron')
  if (!chevron) throw new Error('no chevron')
  chevron.getBoundingClientRect = () => new DOMRect(300, 10, 12, 18)
  return {
    target,
    chevron,
    opens,
    close: () => {
      close()
    },
  }
}

describe('the date Sort by menu', () => {
  it('puts a menu button in the date track, named for what it does', () => {
    const { chevron } = mountWithMenu()

    expect(chevron.closest('.header-date')).toBeTruthy()
    expect(chevron.getAttribute('aria-label')).toBe('Choose a date to sort by')
    expect(chevron.getAttribute('aria-haspopup')).toBe('menu')
    expect(chevron.getAttribute('aria-expanded')).toBe('false')
    // ❌ No roving tabindex in pane chrome: WebKit would make the button mouse-focusable.
    expect(chevron.hasAttribute('tabindex')).toBe(false)
  })

  it('keeps the four sort triggers as the only sort buttons', () => {
    expect(mountWithMenu({ sortBy: null }).target.querySelectorAll('button.sortable-header')).toHaveLength(4)
  })

  it('opens the menu anchored under itself on a click', () => {
    const { chevron, opens } = mountWithMenu()

    chevron.click()

    expect(opens).toEqual([{ x: 300, y: 28 }])
  })

  it.each(['Enter', ' '])('opens on %j without a click, keeping focus for the next press', (key) => {
    const { target, chevron, opens } = mountWithMenu()
    let clicks = 0
    target.addEventListener('click', () => {
      clicks++
    })
    chevron.focus()

    const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true })
    chevron.dispatchEvent(event)

    expect(opens).toEqual([{ x: 300, y: 28 }])
    expect(event.defaultPrevented).toBe(true)
    expect(clicks).toBe(0)
    expect(document.activeElement).toBe(chevron)
  })

  it('says the menu is open until it closes', async () => {
    const { chevron, close } = mountWithMenu()

    chevron.click()
    await tick()
    expect(chevron.getAttribute('aria-expanded')).toBe('true')

    close()
    await vi.waitFor(() => {
      expect(chevron.getAttribute('aria-expanded')).toBe('false')
    })
  })

  it('opens the same menu at the pointer on a right-click anywhere on the header', () => {
    const { target, opens } = mountWithMenu()
    const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true })

    target.querySelector('.label')?.dispatchEvent(event)

    expect(opens).toEqual([null])
    expect(event.defaultPrevented).toBe(true)
  })

  // The chevron sits left of the date header, so → reaches it from Size and goes on to the date.
  it('joins the header toolbar’s arrow-key walk between Size and the date header', () => {
    const scope = document.createElement('div')
    scope.setAttribute('data-pane-chrome-scope', '')
    document.body.append(scope)
    const { target, chevron } = mountWithMenu()
    scope.append(target)
    const size = [...target.querySelectorAll<HTMLButtonElement>('button')].find((b) => b.textContent.includes('Size'))
    size?.focus()

    const pressRight = (): boolean => {
      const event = new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true })
      Object.defineProperty(event, 'target', { value: document.activeElement })
      return movePaneChromeFocus(event)
    }

    expect(pressRight()).toBe(true)
    expect(document.activeElement).toBe(chevron)
    expect(pressRight()).toBe(true)
    expect(document.activeElement).toBe(dateHeader(target))
  })
})
