/**
 * The Full view's column header, mounted on its own.
 *
 * The load-bearing part is which TRACKS it emits: the header and the data rows share
 * one `grid-template-columns`, so a header that renders a cell the rows don't (or vice
 * versa) drifts every column to its right. These pin the two conditional tracks (Git,
 * Ext) against the same conditions `FullList` uses to build the template, plus the
 * scrollbar-width compensation that keeps the header aligned from outside the scroller.
 */

import { describe, it, expect } from 'vitest'
import { mount } from 'svelte'
import FullListHeader from './FullListHeader.svelte'
import type { SortColumn } from '../types'

function mountHeader(props: Partial<Parameters<typeof mountHeaderRaw>[0]> = {}) {
  return mountHeaderRaw({
    gridTemplate: '16px 1fr 60px 115px 80px',
    isFocused: true,
    sortBy: 'name',
    sortOrder: 'ascending' as const,
    showExtensionInName: false,
    gitColumnVisible: false,
    skipTransition: false,
    scrollbarWidth: 0,
    ...props,
  })
}

function mountHeaderRaw(props: {
  gridTemplate: string
  isFocused: boolean
  sortBy: SortColumn
  sortOrder: 'ascending' | 'descending'
  showExtensionInName: boolean
  gitColumnVisible: boolean
  skipTransition: boolean
  scrollbarWidth: number
  onSortChange?: (column: SortColumn) => void
}) {
  const target = document.createElement('div')
  document.body.append(target)
  mount(FullListHeader, { target, props })
  return target
}

/** The sort trigger labels, in DOM order. */
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
