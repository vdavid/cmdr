/**
 * When `listing.showExtensionInName` is ON, the Ext DATA column is gone (the full
 * filename lives in the Name column), but sort-by-extension must stay clickable.
 * The single Name-column header splits into two `SortableHeader` triggers inside a
 * `.header-name-ext` row: "Name" (fills, left) and "Ext" (right), each showing its
 * caret when active. Both sit inside the `1fr` Name track, so the Ext trigger costs
 * the pane no column width.
 *
 * This pins that the header renders both triggers AND that no Ext data cell is
 * emitted, so the renderer stays in lockstep with `measure-column-widths.ts`
 * (which returns `ext: 0` in this mode). The "no Ext cell" half only means
 * anything over rows that are actually on screen, which is what
 * `mountFullList` (`test-full-list.ts`) is for.
 */

import { describe, it, expect, vi } from 'vitest'
import { fileEntry, mountFullList } from './test-full-list'

vi.mock('$lib/tauri-commands', async () => (await import('./test-file-list-mocks')).tauriCommandsMock())
vi.mock('$lib/icon-cache', async () => (await import('./test-file-list-mocks')).iconCacheMock())
vi.mock('$lib/indexing/index-state.svelte', async () => (await import('./test-file-list-mocks')).indexStateMock())
vi.mock('$lib/settings/settings-store', async () => (await import('./test-file-list-mocks')).settingsStoreMock())

// The flag under test.
vi.mock('$lib/settings/reactive-settings.svelte', async () =>
  (await import('./test-file-list-mocks')).reactiveSettingsMock({ getShowExtensionInName: () => true }),
)

const mountPopulated = () =>
  mountFullList({
    entries: [fileEntry({ name: 'report.md', iconId: 'ext:md', size: 2048 })],
    props: { sortBy: 'extension' },
  })

describe('FullList combined Name+Ext header (showExtensionInName)', () => {
  it('renders the combined header with both Name and Ext sort triggers', async () => {
    const { target } = await mountPopulated()
    const combined = target.querySelector('.header-name-ext')
    expect(combined).toBeTruthy()
    const labels = [...(combined?.querySelectorAll('.sortable-header .label') ?? [])].map((l) => l.textContent)
    expect(labels).toEqual(['Name', 'Ext'])
    // Both triggers are real buttons (keyboard- and mouse-operable).
    expect(combined?.querySelectorAll('button.sortable-header').length).toBe(2)
  })

  it('shows the active caret on the Ext trigger when sorting by extension', async () => {
    const { target } = await mountPopulated()
    const combined = target.querySelector('.header-name-ext')
    const extBtn = combined?.querySelectorAll('.sortable-header')[1]
    expect(extBtn?.classList.contains('is-active')).toBe(true)
    // The caret span is only shown (not `.invisible`) on the active column.
    expect(extBtn?.querySelector('.sort-indicator:not(.invisible)')).toBeTruthy()
  })

  it('emits no Ext data cell (full filename rides in the Name column)', async () => {
    const list = await mountPopulated()
    // The row is on screen, so the absent Ext cell is a real absence.
    expect(list.rowNames()).toEqual(['report.md'])
    expect(list.target.querySelectorAll('.col-ext').length).toBe(0)
  })
})
