/**
 * A narrow Full pane drops its date column first, then Ext, so the Name column
 * never collapses to 0 px (the 950 px window with a 25 % split left ~240 px and
 * the fixed tracks ate all of it). Header cells drop with their columns, a
 * dropped Ext folds the extension back into Name (as `showExtensionInName`
 * does, keeping the Ext sort trigger), and a wide pane renders exactly as before.
 *
 * Widths come from a fake measurer (7 px per character), so the fixed tracks are
 * realistic: Ext 28, Size 40, date 107 px at 100 % text size.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { fileEntry, mountFullList } from './test-full-list'
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

const mountAt = (clientWidth: number) =>
  mountFullList({
    entries: [fileEntry({ name: 'report.md', iconId: 'ext:md', size: 2048 })],
    viewport: { clientHeight: 400, clientWidth, offsetWidth: clientWidth },
  })

function headerSortLabels(target: HTMLElement): string[] {
  return [...target.querySelectorAll('.header-row .sortable-header .label')].map((l) => l.textContent)
}

function trackCount(gridTemplate: string): number {
  return gridTemplate.trim().split(/\s+/).length
}

function rowTemplate(target: HTMLElement): string {
  return target.querySelector<HTMLElement>('.file-entry')?.style.gridTemplateColumns ?? ''
}

describe('FullList in a narrow pane', () => {
  it('keeps every column in a wide pane (layout unchanged)', async () => {
    const list = await mountAt(800)
    expect(list.rowNames()).toEqual(['report.md'])
    expect(list.target.querySelectorAll('.col-ext').length).toBe(1)
    expect(list.target.querySelectorAll('.col-date').length).toBe(1)
    expect(list.target.querySelector('.col-name-text')?.textContent).toBe('report')
    expect(list.target.querySelector('.header-date')).toBeTruthy()
    expect(list.target.querySelector('.header-name-ext')).toBeNull()
    expect(headerSortLabels(list.target)).toEqual(['Name', 'Ext', 'Size', 'Modified'])
    // icon 1fr ext size date, mirrored on the header.
    expect(rowTemplate(list.target)).toMatch(/^16px 1fr \d+px \d+px \d+px$/)
    expect(list.target.querySelector<HTMLElement>('.header-row')?.style.gridTemplateColumns).toBe(
      rowTemplate(list.target),
    )
  })

  it('drops the date column (cells and header) first', async () => {
    const list = await mountAt(320)
    await list.settle(() => list.target.querySelectorAll('.col-date').length === 0, 'the date column to drop')
    expect(list.rowNames()).toEqual(['report.md'])
    expect(list.target.querySelectorAll('.col-ext').length).toBe(1)
    expect(list.target.querySelector('.header-date')).toBeNull()
    expect(headerSortLabels(list.target)).toEqual(['Name', 'Ext', 'Size'])
    expect(trackCount(rowTemplate(list.target))).toBe(4)
    expect(list.target.querySelector<HTMLElement>('.header-row')?.style.gridTemplateColumns).toBe(
      rowTemplate(list.target),
    )
  })

  it('then drops Ext, folding the extension into Name and keeping its sort trigger', async () => {
    const list = await mountAt(240)
    await list.settle(() => list.target.querySelectorAll('.col-ext').length === 0, 'the Ext column to drop')
    expect(list.rowNames()).toEqual(['report.md'])
    expect(list.target.querySelectorAll('.col-date').length).toBe(0)
    expect(list.target.querySelector('.col-name-text')?.textContent).toBe('report.md')
    expect(list.target.querySelector('.header-name-ext')).toBeTruthy()
    expect(headerSortLabels(list.target)).toEqual(['Name', 'Ext', 'Size'])
    expect(rowTemplate(list.target)).toMatch(/^16px 1fr \d+px$/)
  })

  it('stops naming the dropped date in the row’s accessible name', async () => {
    const wide = await mountAt(800)
    expect(wide.rows()[0].getAttribute('aria-label')).toMatch(/modified/i)
    wide.target.remove()

    const narrow = await mountAt(240)
    await narrow.settle(() => narrow.target.querySelectorAll('.col-date').length === 0, 'the date column to drop')
    expect(narrow.rows()[0].getAttribute('aria-label')).not.toMatch(/modified/i)
  })

  it('brings the columns back when the pane widens', async () => {
    const list = await mountAt(240)
    await list.settle(() => list.target.querySelectorAll('.col-date').length === 0, 'the date column to drop')
    list.layout.resize('[data-file-list-surface]', { clientWidth: 800, offsetWidth: 800 })
    await list.settle(() => list.target.querySelectorAll('.col-date').length === 1, 'the date column to return')
    expect(list.target.querySelectorAll('.col-ext').length).toBe(1)
  })
})
