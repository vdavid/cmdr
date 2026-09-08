/**
 * `BriefList`'s hidden-entry name dim: `.name.is-hidden` lands only on a
 * hidden row that is neither selected nor under the cursor. See
 * `BriefList.svelte`'s `nameIsHiddenDimmed` const, which delegates to
 * `full-list-utils.ts::isHiddenNameDimmed` — the restricted-row precedence is
 * covered there directly (`full-list-utils.test.ts`), not by a mount here.
 */

import { describe, expect, it, vi } from 'vitest'
import { fileEntry } from './test-full-list'
import { mountBriefList } from './test-brief-list'

vi.mock('$lib/tauri-commands', async () =>
  (await import('./test-file-list-mocks')).tauriCommandsMock({
    getBriefColumnTextWidths: () => Promise.resolve({ status: 'ok', data: { widths: [], missingCodePoints: [] } }),
  }),
)
vi.mock('$lib/icon-cache', async () => (await import('./test-file-list-mocks')).iconCacheMock())
vi.mock('$lib/indexing/index-state.svelte', async () => (await import('./test-file-list-mocks')).indexStateMock())
vi.mock('$lib/settings/reactive-settings.svelte', async () =>
  (await import('./test-file-list-mocks')).reactiveSettingsMock({
    getBriefColumnWidthMode: () => 'paneWidth',
    getBriefColumnWidthMaxPx: () => 400,
  }),
)
vi.mock('$lib/settings/settings-store', async () =>
  (await import('./test-file-list-mocks')).settingsStoreMock({
    'advanced.virtualizationBufferColumns': 2,
  }),
)

function nameCellFor(rows: HTMLElement[], filename: string): HTMLElement {
  const row = rows.find((r) => r.dataset.filename === filename)
  if (!row) throw new Error(`no row rendered for ${filename}`)
  const cell = row.querySelector<HTMLElement>('.name')
  if (!cell) throw new Error(`row for ${filename} has no .name`)
  return cell
}

describe('BriefList hidden-entry name dim', () => {
  it('dims the name of a hidden entry that is neither selected nor under cursor', async () => {
    const entries = [fileEntry({ name: 'visible.txt' }), fileEntry({ name: '.hidden.txt', isHidden: true })]
    const list = await mountBriefList({ entries, props: { cursorIndex: -1 } })

    expect(nameCellFor(list.rows(), '.hidden.txt').classList.contains('is-hidden')).toBe(true)
  })

  it('does not dim an ordinary (non-hidden) entry', async () => {
    const entries = [fileEntry({ name: 'visible.txt' })]
    const list = await mountBriefList({ entries, props: { cursorIndex: -1 } })

    expect(nameCellFor(list.rows(), 'visible.txt').classList.contains('is-hidden')).toBe(false)
  })

  it('does not dim a hidden entry that is selected', async () => {
    const entries = [fileEntry({ name: '.hidden.txt', isHidden: true })]
    const list = await mountBriefList({ entries, props: { cursorIndex: -1, selectedIndices: new Set([0]) } })

    expect(nameCellFor(list.rows(), '.hidden.txt').classList.contains('is-hidden')).toBe(false)
  })

  it('does not dim a hidden entry that is under the cursor', async () => {
    const entries = [fileEntry({ name: '.hidden.txt', isHidden: true })]
    const list = await mountBriefList({ entries, props: { cursorIndex: 0 } })

    expect(nameCellFor(list.rows(), '.hidden.txt').classList.contains('is-hidden')).toBe(false)
  })
})
