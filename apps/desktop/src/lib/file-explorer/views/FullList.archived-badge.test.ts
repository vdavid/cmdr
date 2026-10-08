/**
 * The "archived" glyph beside a file in cold storage (`FileEntry.inColdStorage`,
 * an S3 object in Glacier), in both list views. Only that file carries it: a
 * file that reads on demand shows nothing extra.
 */

import { describe, expect, it, vi } from 'vitest'
import { fileEntry, mountFullList } from './test-full-list'
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

function archivedGlyphOf(rows: HTMLElement[], filename: string): HTMLElement | null {
  const row = rows.find((r) => r.dataset.filename === filename)
  if (!row) throw new Error(`no row rendered for ${filename}`)
  return row.querySelector<HTMLElement>('.status-glyph[aria-label="Archived"]')
}

const entries = [fileEntry({ name: 'holiday-2019.tar', inColdStorage: true }), fileEntry({ name: 'notes.txt' })]

describe('the archived glyph', () => {
  it('marks a file in cold storage in the Full view, and only that file', async () => {
    const list = await mountFullList({ entries, props: { cursorIndex: -1 } })

    expect(archivedGlyphOf(list.rows(), 'holiday-2019.tar')).not.toBeNull()
    expect(archivedGlyphOf(list.rows(), 'notes.txt')).toBeNull()
  })

  it('marks a file in cold storage in the Brief view, and only that file', async () => {
    const list = await mountBriefList({ entries, props: { cursorIndex: -1 } })

    expect(archivedGlyphOf(list.rows(), 'holiday-2019.tar')).not.toBeNull()
    expect(archivedGlyphOf(list.rows(), 'notes.txt')).toBeNull()
  })
})
