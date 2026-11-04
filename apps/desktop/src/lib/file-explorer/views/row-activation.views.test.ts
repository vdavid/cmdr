/**
 * Both views open a row through `row-activation.ts`: a mouse on `dblclick`, a
 * touch or pen on two quick taps even when no `dblclick` comes (Chrome's device
 * mode), and each exactly once. A touch held still presses the row as a mouse
 * press does (`../drag/touch-hold.ts`).
 */

import { describe, expect, it, vi } from 'vitest'
import type { FileEntry, SelectPayload } from '../types'
import { TOUCH_HOLD_MS } from '../drag/touch-hold'
import { fileEntry, mountFullList } from './test-full-list'
import { mountBriefList } from './test-brief-list'

vi.mock('#lib/tauri-commands/index.js', async () =>
  (await import('./test-file-list-mocks')).tauriCommandsMock({
    getBriefColumnTextWidths: () => Promise.resolve({ status: 'ok', data: { widths: [], missingCodePoints: [] } }),
  }),
)
vi.mock('#lib/icon-cache.js', async () => (await import('./test-file-list-mocks')).iconCacheMock())
vi.mock('#lib/indexing/index-state.svelte.js', async () => (await import('./test-file-list-mocks')).indexStateMock())
vi.mock('#lib/settings/reactive-settings.svelte.js', async () =>
  (await import('./test-file-list-mocks')).reactiveSettingsMock({
    getBriefColumnWidthMode: () => 'paneWidth',
    getBriefColumnWidthMaxPx: () => 400,
  }),
)
vi.mock('#lib/settings/settings-store.js', async () =>
  (await import('./test-file-list-mocks')).settingsStoreMock({
    'advanced.virtualizationBufferColumns': 2,
  }),
)

const entries = [fileEntry({ name: 'alpha', isDirectory: true }), fileEntry({ name: 'beta.txt' })]

function press(row: HTMLElement, pointerType: string): void {
  // happy-dom has no `PointerEvent` constructor that takes `pointerType`, so set it by hand.
  const event = new MouseEvent('pointerdown', { bubbles: true, button: 0 })
  Object.defineProperty(event, 'pointerType', { value: pointerType })
  row.dispatchEvent(event)
}

function click(row: HTMLElement, detail: number): void {
  row.dispatchEvent(new MouseEvent('click', { bubbles: true, button: 0, detail }))
}

function dblclick(row: HTMLElement): void {
  row.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, button: 0, detail: 2 }))
}

/** The props both views share that these specs set. */
interface RowProps {
  onNavigate?: (entry: FileEntry) => void
  onSelect?: (args: SelectPayload) => void
}

const views = [
  { name: 'FullList', mount: (props: RowProps) => mountFullList({ entries, props }) },
  { name: 'BriefList', mount: (props: RowProps) => mountBriefList({ entries, props }) },
]

describe.each(views)('$name row activation', ({ mount }) => {
  async function setup() {
    const opened: string[] = []
    const list = await mount({ onNavigate: (entry) => opened.push(entry.name) })
    const row = list.rows().find((r) => r.dataset.filename === 'alpha')
    if (!row) throw new Error('no row rendered for alpha')
    return { opened, row }
  }

  it('opens on a mouse double-click, once', async () => {
    const { opened, row } = await setup()
    press(row, 'mouse')
    click(row, 1)
    press(row, 'mouse')
    click(row, 2)
    dblclick(row)
    expect(opened).toEqual(['alpha'])
  })

  it('stays put on a single mouse click', async () => {
    const { opened, row } = await setup()
    press(row, 'mouse')
    click(row, 1)
    expect(opened).toEqual([])
  })

  it('opens on a double tap that brings no dblclick', async () => {
    const { opened, row } = await setup()
    press(row, 'touch')
    click(row, 1)
    press(row, 'touch')
    click(row, 1)
    expect(opened).toEqual(['alpha'])
  })

  it('presses the row on a touch held still, as a mouse press does', async () => {
    const selected: number[] = []
    const list = await mount({ onSelect: ({ index }) => selected.push(index) })
    const row = list.rows().find((r) => r.dataset.filename === 'alpha')
    if (!row) throw new Error('no row rendered for alpha')
    vi.useFakeTimers()
    try {
      row.dispatchEvent(
        new PointerEvent('pointerdown', {
          bubbles: true,
          button: 0,
          pointerId: 9,
          pointerType: 'touch',
          isPrimary: true,
        }),
      )
      vi.advanceTimersByTime(TOUCH_HOLD_MS)
      // Lifted before the drag threshold: the row's mouse-release select.
      document.dispatchEvent(new PointerEvent('pointerup', { pointerId: 9, pointerType: 'touch' }))
      expect(selected).toEqual([0])
    } finally {
      vi.useRealTimers()
    }
  })

  it('opens once on a double tap that also brings a dblclick', async () => {
    const { opened, row } = await setup()
    press(row, 'touch')
    click(row, 1)
    press(row, 'touch')
    click(row, 2)
    dblclick(row)
    expect(opened).toEqual(['alpha'])
  })
})
