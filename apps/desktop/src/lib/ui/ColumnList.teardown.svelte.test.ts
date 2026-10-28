/**
 * A row's cells may read their row while they tear down, and must always find their OWN row.
 *
 * Regression anchor: Ark's checkbox reads its props again on the way out, so a cell whose row was
 * gone by then threw (`undefined is not an object (evaluating 'op.status')` in the AI suggestions
 * review, `'$.get(row).blockedReason'` in the bulk rename review) and took the main window down
 * to its crash panel. A block was keyed by its row's key but read its row by INDEX, so a removed
 * block found `undefined` there, or the neighbour that had shifted into its index. Here each
 * cell holds a real `Checkbox` whose `checked` reads the row, as the dialogs' include cells do.
 */

import { afterEach, describe, expect, it } from 'vitest'
import { createRawSnippet, flushSync, mount, unmount } from 'svelte'
import ColumnList from './ColumnList.svelte'
import Checkbox from './Checkbox.svelte'
import type { ColumnListColumn, ColumnListWindowedSource } from './column-list-types'

interface Item {
  id: number
  name: string
}

/** Every read where a cell found something other than the row it was drawn for. */
const wrongRows: string[] = []

/** Each cell's own read of its row, kept past the cell's teardown: what Ark does with `checked`
 *  when it reads it from a reaction that outlives the cell (seen in WebKit, not in this DOM). */
const lateReads: Array<{ own: string; read: () => Item | undefined }> = []

function readEveryCellLate(): void {
  for (const { own, read } of lateReads) {
    const name = read()?.name ?? 'nothing'
    if (name !== own) wrongRows.push(`${own} read ${name}`)
  }
}

const includeCell = createRawSnippet<[{ row: Item }]>((ctx) => ({
  render: () => `<span class="cell"></span>`,
  setup: (element) => {
    const own = ctx().row.name
    lateReads.push({ own, read: () => ctx().row })
    const box = mount(Checkbox, {
      target: element,
      props: {
        get checked() {
          // Throws on a gone row, which Vitest reports as an unhandled error.
          const name = ctx().row.name
          if (name !== own) wrongRows.push(`${own} read ${name}`)
          return true
        },
        ariaLabel: `Include ${own}`,
      },
    })
    return () => {
      void unmount(box)
    }
  },
}))

const columns: ColumnListColumn<Item>[] = [
  { id: 'include', label: 'Include', width: { kind: 'fixed', px: 16 }, cell: includeCell },
]

function items(ids: number[]): Item[] {
  return ids.map((id) => ({ id, name: `item-${String(id)}` }))
}

let cleanup: (() => void) | null = null

afterEach(() => {
  cleanup?.()
  cleanup = null
  wrongRows.length = 0
  lateReads.length = 0
  document.body.innerHTML = ''
})

function mountWith(props: Record<string, unknown>): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(ColumnList<Item>, { target, props: props as never })
  cleanup = () => {
    void unmount(component)
  }
  flushSync()
  return target
}

function arrayList(ids: number[]) {
  const props = $state({
    rows: items(ids),
    rowKey: (row: Item) => row.id,
    columns,
    ariaLabel: 'Things',
    virtualized: false,
  })
  return props
}

describe('a row tearing down', () => {
  it('still finds its own row when the list empties', () => {
    const props = arrayList([1, 2, 3])
    const target = mountWith(props)

    props.rows = []
    flushSync()
    readEveryCellLate()

    expect(target.querySelectorAll('.is-data')).toHaveLength(0)
    expect(wrongRows).toEqual([])
  })

  it('finds its own row, never the neighbour that shifted into its index', () => {
    const props = arrayList([1, 2, 3])
    const target = mountWith(props)

    props.rows = items([2, 3])
    flushSync()
    readEveryCellLate()

    expect(target.querySelectorAll('.is-data')).toHaveLength(2)
    expect(wrongRows).toEqual([])
  })

  it('still finds its row when a windowed source drops the page it came from', () => {
    const held: { rows: Item[] | null } = $state({ rows: items([1, 2]) })
    const source: ColumnListWindowedSource<Item> = {
      get count() {
        return 2
      },
      getRow: (index) => held.rows?.[index],
    }
    const target = mountWith({ rows: source, columns, ariaLabel: 'Things', virtualized: false })
    expect(target.querySelectorAll('.is-data')).toHaveLength(2)

    held.rows = null
    flushSync()
    readEveryCellLate()

    expect(target.querySelectorAll('.is-placeholder')).toHaveLength(2)
    expect(wrongRows).toEqual([])
  })
})
