/**
 * `ColumnList` handed two rows that share a `rowKey`.
 *
 * Svelte throws on a repeated `{#each}` key, and in a production build the throw names
 * neither the list nor the key, so the dialog around the list closes with a report
 * nobody can trace (ERR-XVTB2). The list draws every row it was given and says in the
 * log which list it was and which key repeated.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createRawSnippet, mount, tick, unmount } from 'svelte'
import ColumnList from './ColumnList.svelte'
import type { ColumnListColumn } from './column-list-types'
import { installLayoutMock } from '#lib/test-layout.js'

const logError = vi.hoisted(() => vi.fn())

vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: logError }),
}))

vi.mock('@chenglou/pretext', () => ({
  prepareWithSegments: (text: string) => ({ text }),
  measureNaturalWidth: (prepared: { text: string }) => prepared.text.length * 10,
}))

interface Item {
  path: string
  name: string
}

const nameCell = createRawSnippet<[{ row: Item }]>((ctx) => ({
  render: () => `<span class="cell-name">${ctx().row.name}</span>`,
}))

const columns: ColumnListColumn<Item>[] = [
  { id: 'name', label: 'Name', width: { kind: 'share', minPx: 80 }, cell: nameCell },
]

let cleanup: (() => void) | null = null

beforeEach(() => {
  logError.mockClear()
  installLayoutMock({ '.column-list-viewport': { clientHeight: 200, clientWidth: 600 } })
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
    if (this.classList.contains('column-list-probe')) return { height: 20 } as DOMRect
    return Element.prototype.getBoundingClientRect.call(this)
  })
})

afterEach(() => {
  cleanup?.()
  cleanup = null
  vi.restoreAllMocks()
  document.body.innerHTML = ''
})

async function render(rows: Item[]): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(ColumnList<Item>, {
    target,
    props: { columns, ariaLabel: 'Things', rows, rowKey: (row: Item) => row.path } as never,
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

function drawnNames(target: HTMLElement): string[] {
  return [...target.querySelectorAll<HTMLElement>('.column-list-row[data-index] .cell-name')].map(
    (el) => el.textContent,
  )
}

describe('ColumnList with a repeated row key', () => {
  it('draws every row and logs the list and the key', async () => {
    const target = await render([
      { path: '/test/a.txt', name: 'first a' },
      { path: '/test/b.txt', name: 'b' },
      { path: '/test/a.txt', name: 'second a' },
    ])

    expect(drawnNames(target)).toEqual(['first a', 'b', 'second a'])
    expect(logError).toHaveBeenCalledTimes(1)
    expect(logError.mock.calls[0][1]).toMatchObject({ list: 'Things', input: '/test/a.txt' })
  })

  it('says nothing when every key is its own', async () => {
    const target = await render([
      { path: '/test/a.txt', name: 'a' },
      { path: '/test/b.txt', name: 'b' },
    ])

    expect(drawnNames(target)).toEqual(['a', 'b'])
    expect(logError).not.toHaveBeenCalled()
  })
})
