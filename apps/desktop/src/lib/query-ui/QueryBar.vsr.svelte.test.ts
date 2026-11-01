// @vitest-environment jsdom
/**
 * Speech-level expectations for the Search / Selection query field, through
 * `@guidepup/virtual-screen-reader`: the field is a combobox over the result list, and ↓
 * speaks the cursor result through `aria-activedescendant`.
 *
 * It models the ARIA spec over jsdom, not WebKit or VoiceOver, so it guards OUR markup. The
 * harness stands in for `QueryDialog` (whose wiring `QueryDialog.svelte.test.ts` pins): the
 * field, a `ColumnList` with an id, and a keydown handler moving the cursor the way the
 * dialog's does. What VoiceOver really says is a listening check (V6 in David's guide).
 */

import { describe, it, expect, afterEach, beforeAll } from 'vitest'
import { createRawSnippet, flushSync, mount, unmount } from 'svelte'
import { virtual } from '@guidepup/virtual-screen-reader'
import QueryBar from './QueryBar.svelte'
import ColumnList from '#lib/ui/ColumnList.svelte'
import type { ColumnListApi, ColumnListColumn } from '#lib/ui/column-list-types.js'
import { untilFocusNamesARow } from '#lib/test-frames.js'
import { pressAndHear } from '#lib/test-vsr.js'

interface Row {
  name: string
}

const nameCell = createRawSnippet<[{ row: Row }]>((ctx) => ({ render: () => `<span>${ctx().row.name}</span>` }))
const columns: ColumnListColumn<Row>[] = [
  { id: 'name', label: 'Name', width: { kind: 'share', minPx: 80 }, cell: nameCell },
]
const ROWS: Row[] = [{ name: 'photo.jpg' }, { name: 'b.jpg' }, { name: 'c.jpg' }]

const mounted: ReturnType<typeof mount>[] = []

beforeAll(() => {
  Element.prototype.scrollIntoView = () => {}
})

afterEach(async () => {
  for (const component of mounted.splice(0)) void unmount(component)
  await virtual.stop()
  document.body.innerHTML = ''
})

async function setup(): Promise<HTMLInputElement> {
  const state = $state({ cursor: 0, list: undefined as ColumnListApi | undefined })
  const target = document.createElement('div')
  document.body.appendChild(target)
  const listTarget = document.createElement('div')
  mounted.push(
    mount(QueryBar, {
      target,
      props: {
        inputElement: undefined,
        query: '*.jpg',
        mode: 'filename',
        disabled: false,
        aiHighlight: false,
        runHintCopy: 'Press Enter to search',
        listboxId: 'results',
        listExpanded: true,
        get activeOptionId() {
          return state.list?.activeDescendantId(state.cursor)
        },
        onInput: () => {},
        onRun: () => {},
        onToggleRecent: () => {},
        recentTriggerLabel: 'All recent searches',
        recentTriggerTooltip: 'Show all recent searches',
      },
    }),
  )
  document.body.appendChild(listTarget)
  const list = mount(ColumnList<Row>, {
    target: listTarget,
    props: {
      columns,
      rows: ROWS,
      ariaLabel: 'Results',
      id: 'results',
      get cursorIndex() {
        return state.cursor
      },
    },
  })
  mounted.push(list)
  state.list = list
  const input = target.querySelector('input') as HTMLInputElement
  // The dialog's ↓: move the cursor, focus stays in the field.
  input.addEventListener('keydown', (event) => {
    if (event.key === 'ArrowDown') state.cursor = Math.min(state.cursor + 1, ROWS.length - 1)
  })
  flushSync()
  await virtual.start({ container: document.body })
  return input
}

describe('QueryBar speech', () => {
  it('speaks the cursor result as ↓ moves it, focus staying in the field', async () => {
    const input = await setup()
    input.focus()
    await untilFocusNamesARow()
    await pressAndHear(
      'ArrowDown',
      'combobox, Filename search pattern, *.jpg, has popup listbox, expanded, active descendant b.jpg, autocomplete in list, 1 control',
    )
    expect(document.activeElement).toBe(input)
  })
})
