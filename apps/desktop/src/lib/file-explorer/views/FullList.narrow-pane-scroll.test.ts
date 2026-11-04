/**
 * A column never pops in or out while the user scrolls. Full shrink-wraps Ext /
 * Size / date to the rows on screen, so scrolling from short extensions to long
 * ones widens Ext; if the narrow-pane fit (`full-list-column-fit.ts`) followed
 * those live widths, a pane near a threshold would drop the date mid-scroll and
 * bring it back on the way up. The fit latches the widths and ignores a
 * visible-range change; a resize, navigation, or setting re-latches.
 *
 * The listing: five folders on top (no extension, Ext at its header floor),
 * then files with a 10-character extension (Ext ~72 px under the 7 px-per-char
 * fake measurer). The viewport shows five rows, so the first screen is folders
 * only.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { dirEntry, fileEntry, mountFullList } from './test-full-list'
import { _setMeasureForTests } from './measure-column-widths'

vi.mock('#lib/tauri-commands/index.js', async () => (await import('./test-file-list-mocks')).tauriCommandsMock())
vi.mock('#lib/icon-cache.js', async () => (await import('./test-file-list-mocks')).iconCacheMock())
vi.mock('#lib/indexing/index-state.svelte.js', async () => (await import('./test-file-list-mocks')).indexStateMock())
vi.mock('#lib/settings/settings-store.js', async () => (await import('./test-file-list-mocks')).settingsStoreMock())
vi.mock('#lib/settings/reactive-settings.svelte.js', async () =>
  (await import('./test-file-list-mocks')).reactiveSettingsMock(),
)

beforeEach(() => {
  _setMeasureForTests((text) => text.length * 7)
})
afterEach(() => {
  _setMeasureForTests(null)
})

const entries = [
  ...['a', 'b', 'c', 'd', 'e'].map((n) => dirEntry({ name: `folder-${n}` })),
  ...Array.from({ length: 20 }, (_, i) => fileEntry({ name: `file-${String(i)}.controller`, size: 2048 })),
]

const ROW_HEIGHT = 20
const SURFACE = '[data-file-list-surface]'

function extTrackWidth(target: HTMLElement): number {
  // `16px 1fr <ext> <size> [<date>]`
  const template = target.querySelector<HTMLElement>('.file-entry')?.style.gridTemplateColumns ?? ''
  return Number.parseInt(template.split(/\s+/)[2], 10)
}

const dateCells = (target: HTMLElement) => target.querySelectorAll('.col-date').length

describe('FullList narrow-pane fit while scrolling', () => {
  it('never drops or restores a column on scroll alone', async () => {
    // A pane wide enough for every column over the folders (Ext at its floor),
    // but not once the long extensions widen Ext.
    const list = await mountFullList({
      entries,
      viewport: { clientHeight: 5 * ROW_HEIGHT, clientWidth: 400, offsetWidth: 400 },
    })
    expect(dateCells(list.target)).toBeGreaterThan(0)
    const extAtTop = extTrackWidth(list.target)

    list.layout.scroll(SURFACE, 10 * ROW_HEIGHT)
    await list.settle(() => list.rowNames().some((n) => n.endsWith('.controller')), 'the files to scroll into view')
    await list.settle(() => extTrackWidth(list.target) > extAtTop, 'Ext to widen over the long extensions')
    // Ext really did widen past what the date needs, yet the date stays.
    expect(dateCells(list.target)).toBeGreaterThan(0)

    list.layout.scroll(SURFACE, 0)
    await list.settle(() => list.rowNames().includes('folder-a'), 'the folders to scroll back')
    expect(dateCells(list.target)).toBeGreaterThan(0)
  })

  it('re-fits on a resize after the scroll', async () => {
    const list = await mountFullList({
      entries,
      viewport: { clientHeight: 5 * ROW_HEIGHT, clientWidth: 400, offsetWidth: 400 },
    })
    list.layout.scroll(SURFACE, 10 * ROW_HEIGHT)
    await list.settle(() => list.rowNames().some((n) => n.endsWith('.controller')), 'the files to scroll into view')
    // A resize is the user asking for a re-fit, so the latched widths refresh.
    list.layout.resize(SURFACE, { clientWidth: 401, offsetWidth: 401 })
    await list.settle(() => dateCells(list.target) === 0, 'the date column to drop on resize')
  })
})
