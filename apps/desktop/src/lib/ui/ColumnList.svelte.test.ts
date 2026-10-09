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
import ColumnList from './ColumnList.svelte'
import type { ColumnListColumn, ColumnListWindowedSource } from './column-list-types'
import { installLayoutMock, type LayoutMock } from '$lib/test-layout'

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
  const real = HTMLElement.prototype.getBoundingClientRect
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
    if (this.classList.contains('column-list-probe')) return { height: ROW_PX } as DOMRect
    return real.call(this)
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
    component.scrollIndexIntoView(500)
    flushSync()
    const viewport = target.querySelector<HTMLElement>(VIEWPORT)
    // Row 500's bottom edge (501 * 20) aligned to the 200 px viewport's bottom.
    expect(viewport?.scrollTop).toBe(501 * ROW_PX - 200)
    expect(drawnIndices(target)).toContain(500)
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
})

describe('ColumnList cursor and pointer', () => {
  it('marks the cursor row as the selected option and reports hover, click, and right-click', async () => {
    const onHover = vi.fn()
    const onRowClick = vi.fn()
    const onRowContextMenu = vi.fn()
    const rows = items(3)
    const target = await render({ rows, cursorIndex: 1, onHover, onRowClick, onRowContextMenu })
    const options = target.querySelectorAll<HTMLElement>('[role="option"]')
    expect(target.querySelector('[role="listbox"]')?.getAttribute('aria-label')).toBe('Things')
    expect([...options].map((o) => o.getAttribute('aria-selected'))).toEqual(['false', 'true', 'false'])
    expect(options[1].classList.contains('is-under-cursor')).toBe(true)
    expect(options[2].getAttribute('aria-posinset')).toBe('3')
    expect(options[2].getAttribute('aria-setsize')).toBe('3')

    options[2].dispatchEvent(new MouseEvent('mouseenter'))
    options[2].click()
    const menu = new MouseEvent('contextmenu', { bubbles: true, cancelable: true })
    options[0].dispatchEvent(menu)
    expect(onHover).toHaveBeenCalledWith(2)
    expect(onRowClick).toHaveBeenCalledWith(2)
    expect(onRowContextMenu).toHaveBeenCalledWith(expect.objectContaining({ index: 0, row: rows[0] }))
    expect(menu.defaultPrevented).toBe(true)
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
