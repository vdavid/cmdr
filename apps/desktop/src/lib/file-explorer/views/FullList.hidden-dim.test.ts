/**
 * `FullList`'s hidden-entry name dim: `.col-name-text.is-hidden` /
 * `.col-ext.is-hidden` land only on a hidden row that is neither selected nor
 * under the cursor. See `FullList.svelte`'s `rowIsHiddenDimmed` const, which
 * delegates to `full-list-utils.ts::isHiddenRowDimmed` — the restricted-row
 * precedence is covered there directly (`full-list-utils.test.ts`), not by a
 * mount here, matching how `pickSizeDisplay`'s own restricted branch is
 * tested.
 */

import { describe, expect, it, vi } from 'vitest'
import { dirEntry, fileEntry, mountFullList } from './test-full-list'

vi.mock('$lib/tauri-commands', async () => (await import('./test-file-list-mocks')).tauriCommandsMock())
vi.mock('$lib/icon-cache', async () => (await import('./test-file-list-mocks')).iconCacheMock())
vi.mock('$lib/indexing/index-state.svelte', async () => (await import('./test-file-list-mocks')).indexStateMock())
vi.mock('$lib/settings/reactive-settings.svelte', async () =>
  (await import('./test-file-list-mocks')).reactiveSettingsMock(),
)
vi.mock('$lib/settings/settings-store', async () => (await import('./test-file-list-mocks')).settingsStoreMock())

function nameCellFor(rows: HTMLElement[], filename: string): HTMLElement {
  const row = rows.find((r) => r.dataset.filename === filename)
  if (!row) throw new Error(`no row rendered for ${filename}`)
  const cell = row.querySelector<HTMLElement>('.col-name-text')
  if (!cell) throw new Error(`row for ${filename} has no .col-name-text`)
  return cell
}

function iconWrapperFor(rows: HTMLElement[], filename: string): HTMLElement {
  const row = rows.find((r) => r.dataset.filename === filename)
  if (!row) throw new Error(`no row rendered for ${filename}`)
  const wrapper = row.querySelector<HTMLElement>('.icon-wrapper')
  if (!wrapper) throw new Error(`row for ${filename} has no .icon-wrapper`)
  return wrapper
}

describe('FullList hidden-entry name dim', () => {
  it('dims the name of a hidden entry that is neither selected nor under cursor', async () => {
    const entries = [fileEntry({ name: 'visible.txt' }), fileEntry({ name: '.hidden.txt', isHidden: true })]
    const list = await mountFullList({ entries, props: { cursorIndex: -1 } })

    expect(nameCellFor(list.rows(), '.hidden.txt').classList.contains('is-hidden')).toBe(true)
  })

  it('does not dim an ordinary (non-hidden) entry', async () => {
    const entries = [fileEntry({ name: 'visible.txt' })]
    const list = await mountFullList({ entries, props: { cursorIndex: -1 } })

    expect(nameCellFor(list.rows(), 'visible.txt').classList.contains('is-hidden')).toBe(false)
  })

  it('does not dim a hidden entry that is selected', async () => {
    const entries = [fileEntry({ name: '.hidden.txt', isHidden: true })]
    const list = await mountFullList({ entries, props: { cursorIndex: -1, selectedIndices: new Set([0]) } })

    expect(nameCellFor(list.rows(), '.hidden.txt').classList.contains('is-hidden')).toBe(false)
  })

  it('does not dim a hidden entry that is under the cursor', async () => {
    const entries = [fileEntry({ name: '.hidden.txt', isHidden: true })]
    const list = await mountFullList({ entries, props: { cursorIndex: 0 } })

    expect(nameCellFor(list.rows(), '.hidden.txt').classList.contains('is-hidden')).toBe(false)
  })

  it('dims the Ext column too, when the extension is split out', async () => {
    const entries = [fileEntry({ name: '.hidden.env', isHidden: true })]
    const list = await mountFullList({ entries, props: { cursorIndex: -1 } })
    const row = list.rows().find((r) => r.dataset.filename === '.hidden.env')
    const extCell = row?.querySelector<HTMLElement>('.col-ext')

    expect(extCell?.classList.contains('is-hidden')).toBe(true)
  })

  it('leaves a hidden directory row available for a size cell (sanity: dir rows still render)', async () => {
    const entries = [dirEntry({ name: '.Trash', isHidden: true })]
    const list = await mountFullList({ entries, props: { cursorIndex: -1 } })

    expect(nameCellFor(list.rows(), '.Trash').classList.contains('is-hidden')).toBe(true)
  })
})

describe('FullList hidden-entry icon dim', () => {
  it('dims the icon of a hidden entry, alongside its name', async () => {
    const entries = [fileEntry({ name: 'visible.txt' }), fileEntry({ name: '.hidden.txt', isHidden: true })]
    const list = await mountFullList({ entries, props: { cursorIndex: -1 } })

    expect(iconWrapperFor(list.rows(), '.hidden.txt').classList.contains('is-dimmed')).toBe(true)
  })

  it("does not dim an ordinary entry's icon", async () => {
    const entries = [fileEntry({ name: 'visible.txt' })]
    const list = await mountFullList({ entries, props: { cursorIndex: -1 } })

    expect(iconWrapperFor(list.rows(), 'visible.txt').classList.contains('is-dimmed')).toBe(false)
  })

  it('does not dim the icon of a hidden entry under the cursor', async () => {
    const entries = [fileEntry({ name: '.hidden.txt', isHidden: true })]
    const list = await mountFullList({ entries, props: { cursorIndex: 0 } })

    expect(iconWrapperFor(list.rows(), '.hidden.txt').classList.contains('is-dimmed')).toBe(false)
  })

  it('dims a hidden directory icon too', async () => {
    const entries = [dirEntry({ name: '.Trash', isHidden: true })]
    const list = await mountFullList({ entries, props: { cursorIndex: -1 } })

    expect(iconWrapperFor(list.rows(), '.Trash').classList.contains('is-dimmed')).toBe(true)
  })
})
