/**
 * `BriefList` keeps the previous rows painted through a cold context change
 * (navigation, sort) until the forced fetch lands. These pin that those
 * retained rows are never handed out as the entry under the cursor: their
 * indices belong to the old listing, so Enter or a rename would hit the wrong
 * file.
 */

import { describe, expect, it, vi } from 'vitest'
import { tick, type ComponentProps } from 'svelte'
import { getFileRange } from '$lib/tauri-commands'
import { fileEntry } from './test-full-list'
import { mountBriefList } from './test-brief-list'
import type BriefList from './BriefList.svelte'

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

type Props = ComponentProps<typeof BriefList>

describe('BriefList retained rows', () => {
  it.each<[string, (props: Partial<Props>) => void]>([
    ['a navigation', (props) => (props.listingId = 'listing-2')],
    ['an explicit refresh or sort', (props) => (props.cacheGeneration = 1)],
  ])('are painted but not actionable after %s, until the new rows land', async (_label, change) => {
    const live = $state<Partial<Props>>({})
    const list = await mountBriefList({
      entries: [fileEntry({ name: 'a.txt' }), fileEntry({ name: 'b.txt' })],
      liveProps: live,
    })
    expect(list.component.getEntryAt(0)?.name).toBe('a.txt')

    let release: (rows: ReturnType<typeof fileEntry>[]) => void = () => {}
    vi.mocked(getFileRange).mockImplementation(
      () =>
        new Promise((resolve) => {
          release = resolve
        }),
    )
    change(live)
    await tick()

    expect(list.rowNames()).toEqual(['a.txt', 'b.txt'])
    expect(list.component.getEntryAt(0)).toBeUndefined()
    expect(list.component.indexOfEntry(fileEntry({ name: 'a.txt' }).path)).toBeUndefined()

    release([fileEntry({ name: 'new.txt' })])
    await list.settle(() => list.rowNames().includes('new.txt'), 'the replacement rows')

    expect(list.component.getEntryAt(0)?.name).toBe('new.txt')
  })

  // A jump from `/root` straight to `/root/b/sub` once painted the new `..`
  // (`/root/b`) over the old rows, which hold `/root/b` too: a duplicate key.
  it('keep the parent row they were fetched with, so no path repeats', async () => {
    const live = $state<Partial<Props>>({})
    const list = await mountBriefList({
      entries: [fileEntry({ name: 'a' }), fileEntry({ name: 'b' })],
      liveProps: live,
    })
    vi.mocked(getFileRange).mockImplementation(() => new Promise(() => {}))

    live.listingId = 'listing-2'
    live.hasParent = true
    live.parentPath = '/root/b'
    live.currentPath = '/root/b/sub'
    live.totalCount = 3
    await tick()

    expect(list.rowNames()).toEqual(['a', 'b'])
  })
})
