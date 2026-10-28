/**
 * Component tests for `ColumnList`: the virtual window over a real DOM, the windowed source
 * contract, the single cursor, both semantics, and measured columns reaching the template.
 * The pure math is pinned in `column-list-layout.test.ts`.
 *
 * The test DOM has no layout: the viewport's height and width come from `installLayoutMock`,
 * the row height from a stubbed `getBoundingClientRect` on the probe (fractional, like the
 * real token's 23.6 px, which `offsetHeight` would round), and `@chenglou/pretext` is a
 * 10-px-per-character font.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createRawSnippet, flushSync, mount, tick, unmount } from 'svelte'
import { SvelteMap } from 'svelte/reactivity'
import ColumnList from './ColumnList.svelte'
import type { ColumnListApi, ColumnListColumn, ColumnListWindowedSource } from './column-list-types'
import { installLayoutMock, type LayoutMock } from '#lib/test-layout.js'

vi.mock('@chenglou/pretext', () => ({
  prepareWithSegments: (text: string) => ({ text }),
  measureNaturalWidth: (prepared: { text: string }) => prepared.text.length * 10,
}))

interface Item {
  id: number
  name: string
}

const ROW_PX = 20
const VIEWPORT = '.column-list-viewport'

const nameCell = createRawSnippet<[{ row: Item }]>((ctx) => ({
  render: () => `<span>${ctx().row.name}</span>`,
}))
const idCell = createRawSnippet<[{ row: Item }]>((ctx) => ({
  render: () => `<span>${String(ctx().row.id)}</span>`,
}))

const plainColumns: ColumnListColumn<Item>[] = [
  { id: 'name', label: 'Name', width: { kind: 'share', minPx: 80 }, cell: nameCell },
  { id: 'id', label: 'Id', width: { kind: 'fixed', ch: 6 }, align: 'end', cell: idCell },
]

function items(count: number): Item[] {
  return Array.from({ length: count }, (_, i) => ({ id: i, name: `item-${String(i)}` }))
}

let layout: LayoutMock
let cleanup: (() => void) | null = null

beforeEach(() => {
  layout = installLayoutMock({ [VIEWPORT]: { clientHeight: 200, clientWidth: 600 } })
  // The spy lands on `HTMLElement.prototype`, so `Element.prototype` still holds the real one.
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
    if (this.classList.contains('column-list-probe')) return { height: ROW_PX } as DOMRect
    return Element.prototype.getBoundingClientRect.call(this)
  })
})

afterEach(() => {
  cleanup?.()
  cleanup = null
  vi.restoreAllMocks()
  document.body.innerHTML = ''
})

async function render(props: Record<string, unknown>): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(ColumnList<Item>, {
    target,
    props: { columns: plainColumns, ariaLabel: 'Things', ...props } as never,
  })
  cleanup = () => {
    void unmount(component)
  }
  for (let i = 0; i < 4; i++) {
    await tick()
    await Promise.resolve()
  }
  return target
}

function drawnIndices(target: HTMLElement): number[] {
  return [...target.querySelectorAll<HTMLElement>('.column-list-row[data-index]')].map((el) => Number(el.dataset.index))
}

describe('ColumnList virtual window', () => {
  it('draws only the rows in view plus overscan, with spacers for the rest', async () => {
    const target = await render({ rows: items(10_000) })
    // 200 px / 20 px = 10 rows in view, plus 20 overscan below.
    expect(drawnIndices(target)).toEqual(Array.from({ length: 30 }, (_, i) => i))
    const inner = target.querySelector<HTMLElement>('.column-list-viewport > div')
    expect(inner?.style.paddingTop).toBe('0px')
    expect(inner?.style.paddingBottom).toBe(`${String((10_000 - 30) * ROW_PX)}px`)
  })

  it('moves the window when the viewport scrolls', async () => {
    const target = await render({ rows: items(10_000) })
    layout.scroll(VIEWPORT, 2_000)
    flushSync()
    const drawn = drawnIndices(target)
    expect(drawn[0]).toBe(80)
    expect(drawn.at(-1)).toBe(129)
    expect(target.querySelector<HTMLElement>('.column-list-viewport > div')?.style.paddingTop).toBe(
      `${String(80 * ROW_PX)}px`,
    )
  })

  it('renders a short list whole, without spacers', async () => {
    const target = await render({ rows: items(5) })
    expect(drawnIndices(target)).toEqual([0, 1, 2, 3, 4])
    expect(target.querySelector<HTMLElement>('.column-list-viewport > div')?.style.paddingBottom).toBe('0px')
  })

  it('scrolls a row outside the viewport into view on request', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(ColumnList<Item>, {
      target,
      props: { columns: plainColumns, ariaLabel: 'Things', rows: items(1_000) },
    })
    cleanup = () => {
      void unmount(component)
    }
    await tick()
    ;(component as ColumnListApi).scrollIndexIntoView(500)
    flushSync()
    const viewport = target.querySelector<HTMLElement>(VIEWPORT)
    // Row 500's bottom edge (501 * 20) aligned to the 200 px viewport's bottom.
    expect(viewport?.scrollTop).toBe(501 * ROW_PX - 200)
    expect(drawnIndices(target)).toContain(500)
  })
})

describe('ColumnList content-sized rows', () => {
  it('draws every row with no spacers, however far the viewport is from them', async () => {
    const target = await render({ rows: items(300), virtualized: false })
    expect(drawnIndices(target)).toEqual(Array.from({ length: 300 }, (_, i) => i))
    const inner = target.querySelector<HTMLElement>('.column-list-viewport > div')
    expect(inner?.style.paddingTop).toBe('0px')
    expect(inner?.style.paddingBottom).toBe('0px')

    layout.scroll(VIEWPORT, 4_000)
    flushSync()
    expect(drawnIndices(target)).toHaveLength(300)
  })

  it('makes the row height a floor rather than the height', async () => {
    const target = await render({ rows: items(3), virtualized: false, rowHeight: '40px' })
    const row = target.querySelector<HTMLElement>('.column-list-row.is-data')
    expect(row?.style.height).toBe('')
    expect(row?.style.minHeight).toBe('40px')
    expect(target.querySelector('.column-list')?.classList.contains('is-content-sized')).toBe(true)
  })

  it('scrolls a row into view by its element, since rows have no fixed height', async () => {
    const scrolled: string[] = []
    vi.spyOn(HTMLElement.prototype, 'scrollIntoView').mockImplementation(function (this: HTMLElement) {
      scrolled.push(this.dataset.index ?? '')
    })
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(ColumnList<Item>, {
      target,
      props: { columns: plainColumns, ariaLabel: 'Things', rows: items(300), virtualized: false },
    })
    cleanup = () => {
      void unmount(component)
    }
    await tick()
    ;(component as ColumnListApi).scrollIndexIntoView(250)
    expect(scrolled).toEqual(['250'])
    // The fixed-height arithmetic would have written a scroll offset instead.
    expect(target.querySelector<HTMLElement>(VIEWPORT)?.scrollTop).toBe(0)
  })
})

describe('ColumnList windowed source', () => {
  it('draws placeholders for rows not loaded yet and reports the range on screen', async () => {
    const loaded = new Map(items(5).map((item) => [item.id, item]))
    const ranges: { start: number; end: number }[] = []
    const source: ColumnListWindowedSource<Item> = {
      count: 200_000,
      getRow: (index) => loaded.get(index),
      onRangeChange: (range) => ranges.push(range),
    }
    const target = await render({ rows: source })
    expect(ranges.at(-1)).toEqual({ start: 0, end: 30 })
    expect(target.querySelectorAll('.column-list-row.is-data')).toHaveLength(5)
    const placeholders = target.querySelectorAll('.column-list-row.is-placeholder')
    expect(placeholders).toHaveLength(25)
    expect(placeholders[0].getAttribute('aria-hidden')).toBe('true')
  })

  it('fills a deep scroll of a 60,000-row list once its rows land, with no row left blank', async () => {
    const loaded = new SvelteMap<number, Item>()
    const ranges: { start: number; end: number }[] = []
    const source: ColumnListWindowedSource<Item> = {
      count: 60_000,
      getRow: (index) => loaded.get(index),
      onRangeChange: (range) => ranges.push(range),
    }
    const target = await render({ rows: source })

    // Near the end: row 59,500 at the top, 10 in view, 20 overscan each side.
    layout.scroll(VIEWPORT, 59_500 * ROW_PX)
    flushSync()
    expect(ranges.at(-1)).toEqual({ start: 59_480, end: 59_530 })
    for (let index = 59_480; index < 59_530; index++) loaded.set(index, { id: index, name: `item-${String(index)}` })
    flushSync()

    expect(target.querySelectorAll('.column-list-row.is-placeholder')).toHaveLength(0)
    expect(drawnIndices(target)).toEqual(Array.from({ length: 50 }, (_, i) => 59_480 + i))
    const inner = target.querySelector<HTMLElement>('.column-list-viewport > div')
    expect(inner?.style.paddingTop).toBe(`${String(59_480 * ROW_PX)}px`)
    expect(inner?.style.paddingBottom).toBe(`${String((60_000 - 59_530) * ROW_PX)}px`)
  })
})

describe('ColumnList cursor and pointer', () => {
  it('marks the cursor row as the selected option and reports a click and a right-click', async () => {
    const onCursorChange = vi.fn()
    const onRowClick = vi.fn()
    const onRowContextMenu = vi.fn()
    const rows = items(3)
    const target = await render({ rows, cursorIndex: 1, onCursorChange, onRowClick, onRowContextMenu })
    const options = target.querySelectorAll<HTMLElement>('[role="option"]')
    expect(target.querySelector('[role="listbox"]')?.getAttribute('aria-label')).toBe('Things')
    expect([...options].map((o) => o.getAttribute('aria-selected'))).toEqual(['false', 'true', 'false'])
    expect(options[1].classList.contains('is-under-cursor')).toBe(true)
    expect(options[2].getAttribute('aria-posinset')).toBe('3')
    expect(options[2].getAttribute('aria-setsize')).toBe('3')

    options[2].dispatchEvent(new MouseEvent('mouseenter'))
    // Hover leaves the cursor alone unless the list asks for it.
    expect(onCursorChange).not.toHaveBeenCalled()
    options[2].click()
    const menu = new MouseEvent('contextmenu', { bubbles: true, cancelable: true })
    options[0].dispatchEvent(menu)
    expect(onCursorChange).toHaveBeenCalledWith(2)
    expect(onRowClick).toHaveBeenCalledWith(2)
    expect(onRowContextMenu).toHaveBeenCalledWith(expect.objectContaining({ index: 0, row: rows[0] }))
    expect(menu.defaultPrevented).toBe(true)
  })

  it('moves the cursor on hover with hoverMovesCursor', async () => {
    const onCursorChange = vi.fn()
    const target = await render({ rows: items(3), cursorIndex: 0, onCursorChange, hoverMovesCursor: true })
    target.querySelectorAll<HTMLElement>('[role="option"]')[2].dispatchEvent(new MouseEvent('mouseenter'))
    expect(onCursorChange).toHaveBeenCalledWith(2)
  })
})

/** Mounts a list and hands back its API too, for the methods a parent calls. */
async function mountList(props: Record<string, unknown>): Promise<{ target: HTMLElement; api: ColumnListApi }> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(ColumnList<Item>, {
    target,
    props: { columns: plainColumns, ariaLabel: 'Things', ...props } as never,
  })
  cleanup = () => {
    void unmount(component)
  }
  for (let i = 0; i < 4; i++) {
    await tick()
    await Promise.resolve()
  }
  return { target, api: component as ColumnListApi }
}

function keydown(key: string, modifiers: KeyboardEventInit = {}): KeyboardEvent {
  return new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...modifiers })
}

// The parent decides when a key is the list's (Search's field, a review row's button); the
// list says where it lands, from its own count, headings, and rows on screen.
describe('ColumnList cursorTarget', () => {
  it('pages by the rows on screen, less one, and reaches both ends', async () => {
    // 200 px viewport over 20 px rows: 10 on screen.
    const { api } = await mountList({ rows: items(100) })
    expect(api.visibleRowCount()).toBe(10)
    expect(api.cursorTarget(keydown('PageDown'), 0)).toBe(9)
    expect(api.cursorTarget(keydown('PageUp'), 50)).toBe(41)
    expect(api.cursorTarget(keydown('End'), 3)).toBe(99)
    expect(api.cursorTarget(keydown('Home'), 30)).toBe(0)
  })

  it('wraps a step when asked, and leaves Home and End to a text field', async () => {
    const { api } = await mountList({ rows: items(5) })
    expect(api.cursorTarget(keydown('ArrowDown'), 4)).toBe(4)
    expect(api.cursorTarget(keydown('ArrowDown'), 4, { wrap: true })).toBe(0)
    expect(api.cursorTarget(keydown('Home'), 3, { textField: true })).toBeNull()
  })

  it("steps over group headings, and answers null for a key that isn't a cursor key", async () => {
    const rows: Item[] = [{ id: -1, name: '~/A' }, ...items(2), { id: -2, name: '~/B' }, ...items(2)]
    const { api } = await mountList({
      rows,
      semantics: 'table',
      isGroupHeading: (row: Item) => row.id < 0,
      groupHeading: nameCell,
    })
    expect(api.cursorTarget(keydown('ArrowDown'), 2)).toBe(4)
    expect(api.cursorTarget(keydown('Home'), 5)).toBe(1)
    expect(api.cursorTarget(keydown('ArrowDown', { shiftKey: true }), 2)).toBeNull()
    expect(api.cursorTarget(keydown('Enter'), 2)).toBeNull()
  })

  it('lets the cursor land on a windowed row that is still loading', async () => {
    const source: ColumnListWindowedSource<Item> = {
      count: 50,
      getRow: (index) => (index < 5 ? items(5)[index] : undefined),
    }
    const { api } = await mountList({ rows: source })
    expect(api.cursorTarget(keydown('ArrowDown'), 4)).toBe(5)
  })
})

// A grid is a table that holds focus itself: the Multi-rename preview, a Tab stop whose
// cursor the keys and clicks move, read by VoiceOver through `aria-activedescendant`.
describe('ColumnList grid semantics', () => {
  it('is a focusable grid of rows and gridcells with the cursor row selected', async () => {
    const { target } = await mountList({ rows: items(3), semantics: 'grid', cursorIndex: 1 })
    const grid = target.querySelector<HTMLElement>('[role="grid"]')
    expect(grid?.getAttribute('tabindex')).toBe('0')
    expect(grid?.getAttribute('aria-label')).toBe('Things')
    expect(grid?.getAttribute('aria-rowcount')).toBe('4')
    expect(target.querySelectorAll('[role="columnheader"]')).toHaveLength(2)
    const rows = target.querySelectorAll<HTMLElement>('.column-list-row.is-data')
    expect([...rows].map((row) => row.getAttribute('aria-selected'))).toEqual(['false', 'true', 'false'])
    expect(rows[0].querySelectorAll('[role="gridcell"]')).toHaveLength(2)
    expect(rows[0].hasAttribute('tabindex')).toBe(false)
  })

  it('answers its cursor keys and claims them, and leaves the rest to bubble', async () => {
    const onCursorChange = vi.fn()
    const { target } = await mountList({ rows: items(100), semantics: 'grid', cursorIndex: 3, onCursorChange })
    const grid = target.querySelector<HTMLElement>('[role="grid"]')
    // Above the mount root: Svelte delegates `onkeydown` to the root itself.
    const outside = vi.fn()
    document.body.addEventListener('keydown', outside)

    const down = keydown('ArrowDown')
    grid?.dispatchEvent(down)
    expect(onCursorChange).toHaveBeenLastCalledWith(4)
    expect(down.defaultPrevented).toBe(true)
    grid?.dispatchEvent(keydown('PageDown'))
    expect(onCursorChange).toHaveBeenLastCalledWith(12)
    grid?.dispatchEvent(keydown('End'))
    expect(onCursorChange).toHaveBeenLastCalledWith(99)
    expect(outside).not.toHaveBeenCalled()

    const shifted = keydown('ArrowDown', { shiftKey: true })
    grid?.dispatchEvent(shifted)
    expect(shifted.defaultPrevented).toBe(false)
    expect(outside).toHaveBeenCalledTimes(1)
    expect(onCursorChange).toHaveBeenCalledTimes(3)
    document.body.removeEventListener('keydown', outside)
  })

  it('scrolls the row a key landed on into view', async () => {
    const { target } = await mountList({
      rows: items(1_000),
      semantics: 'grid',
      cursorIndex: 0,
      onCursorChange: vi.fn(),
    })
    target.querySelector<HTMLElement>('[role="grid"]')?.dispatchEvent(keydown('End'))
    flushSync()
    expect(target.querySelector<HTMLElement>(VIEWPORT)?.scrollTop).toBe(1_000 * ROW_PX - 200)
  })

  it('names the cursor row as its active descendant a frame after it takes focus', async () => {
    vi.useFakeTimers()
    try {
      const { target } = await mountList({ rows: items(3), semantics: 'grid', cursorIndex: 2 })
      const grid = target.querySelector<HTMLElement>('[role="grid"]')
      expect(grid?.hasAttribute('aria-activedescendant')).toBe(false)
      grid?.focus()
      flushSync()
      expect(grid?.hasAttribute('aria-activedescendant')).toBe(false)
      vi.advanceTimersByTime(150)
      flushSync()
      const id = grid?.getAttribute('aria-activedescendant') ?? ''
      expect(id).not.toBe('')
      expect(target.querySelector(`#${CSS.escape(id)}`)?.getAttribute('data-index')).toBe('2')
      grid?.blur()
      flushSync()
      expect(grid?.hasAttribute('aria-activedescendant')).toBe(false)
    } finally {
      vi.useRealTimers()
    }
  })
})

// A combobox (Search's query field) names the cursor row by id, so a list given an `id`
// gives every option one, and says which id is safe to name: only a row that's drawn, since
// `aria-activedescendant` pointing at nothing reads as nothing.
describe('ColumnList option ids', () => {
  it('ids the listbox and every option, and names only a drawn row as active', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(ColumnList<Item>, {
      target,
      props: { columns: plainColumns, ariaLabel: 'Things', rows: items(1_000), id: 'results' },
    })
    cleanup = () => {
      void unmount(component)
    }
    for (let i = 0; i < 4; i++) await tick()
    expect(target.querySelector('[role="listbox"]')?.id).toBe('results')
    const options = target.querySelectorAll<HTMLElement>('[role="option"]')
    expect(options[3].id).toBe('results-option-3')

    const api = component as ColumnListApi
    expect(api.activeDescendantId(3)).toBe('results-option-3')
    expect(api.activeDescendantId(900)).toBeUndefined()
    expect(api.activeDescendantId(-1)).toBeUndefined()
  })

  it('leaves options without ids when the list has none', async () => {
    const target = await render({ rows: items(3) })
    expect([...target.querySelectorAll('[role="option"]')].some((o) => o.hasAttribute('id'))).toBe(false)
  })
})

describe('ColumnList table semantics', () => {
  it('renders rows and cells with a row count, and group headings as spanning row headers', async () => {
    const heading = createRawSnippet<[{ row: Item }]>((ctx) => ({ render: () => `<span>${ctx().row.name}</span>` }))
    const rows: Item[] = [{ id: -1, name: '~/Photos' }, ...items(2)]
    const target = await render({
      rows,
      semantics: 'table',
      isGroupHeading: (row: Item) => row.id < 0,
      groupHeading: heading,
    })
    const table = target.querySelector('[role="table"]')
    expect(table?.getAttribute('aria-rowcount')).toBe('4')
    expect(target.querySelectorAll('[role="columnheader"]')).toHaveLength(2)
    expect(target.querySelector('[role="listbox"]')).toBeNull()
    const rowheader = target.querySelector('[role="rowheader"]')
    expect(rowheader?.textContent).toContain('~/Photos')
    expect(rowheader?.getAttribute('aria-colspan')).toBe('2')
    const dataRows = target.querySelectorAll('.column-list-row.is-data')
    expect(dataRows[0].getAttribute('role')).toBe('row')
    expect(dataRows[0].getAttribute('aria-rowindex')).toBe('3')
    expect(dataRows[0].querySelectorAll('[role="cell"]')).toHaveLength(2)
  })
})

describe('ColumnList measured columns', () => {
  it('pins a fit column to its widest cell and splits the rest between share columns', async () => {
    const columns: ColumnListColumn<Item>[] = [
      {
        id: 'name',
        label: 'Name',
        width: { kind: 'share', minPx: 40 },
        demand: ({ row, measure }) => measure.text(row.name),
        emphasis: true,
        cell: nameCell,
      },
      {
        id: 'note',
        label: 'Note',
        width: { kind: 'share', minPx: 40 },
        demand: () => 2_000,
        cell: nameCell,
      },
      {
        id: 'id',
        label: 'Id',
        width: { kind: 'fit', fallback: { kind: 'fixed', ch: 6 } },
        demand: ({ row, measure }) => measure.tabular(String(row.id)),
        cell: idCell,
      },
    ]
    const target = await render({ columns, rows: items(12) })
    const header = target.querySelector<HTMLElement>('.column-list-header')
    // Name: widest "item-10" = 70 + 2 pad. Id: header "Id" (20) beats "11" (20), + 2 pad.
    expect(header?.style.gridTemplateColumns).toBe('72px minmax(40px, 1fr) 22px')
    for (const row of target.querySelectorAll<HTMLElement>('.column-list-row.is-data')) {
      expect(row.style.gridTemplateColumns).toBe(header?.style.gridTemplateColumns)
    }
  })
})
