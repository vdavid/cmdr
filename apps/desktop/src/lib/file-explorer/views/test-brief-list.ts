/**
 * Mounting a real `BriefList` in a unit test, over a surface that has a size.
 *
 * `BriefList`'s twin: see `test-full-list.ts` for the full rationale (a
 * layout-less DOM measures `clientHeight` / `clientWidth` at zero, so without a
 * measured `[data-file-list-surface]` the virtual window renders ZERO columns
 * and a spec silently asserts on an empty DOM). `mountBriefList` supplies that
 * surface, a `getFileRange` serving the requested range, and the required
 * props.
 *
 * Pairs with the same `test-file-list-mocks.ts` stand-ins `test-full-list.ts`
 * uses, plus `getBriefColumnTextWidths` (Brief's own column-width IPC, which
 * `full-list-utils`'s siblings don't need):
 *
 *     import { fileEntry, mountBriefList } from './test-brief-list'
 *
 *     vi.mock('$lib/tauri-commands', async () =>
 *       (await import('./test-file-list-mocks')).tauriCommandsMock({
 *         getBriefColumnTextWidths: () =>
 *           Promise.resolve({ status: 'ok', data: { widths: [], missingCodePoints: [] } }),
 *       }),
 *     )
 *     vi.mock('$lib/icon-cache', async () => (await import('./test-file-list-mocks')).iconCacheMock())
 *     vi.mock('$lib/indexing/index-state.svelte', async () =>
 *       (await import('./test-file-list-mocks')).indexStateMock())
 *     vi.mock('$lib/settings/reactive-settings.svelte', async () =>
 *       (await import('./test-file-list-mocks')).reactiveSettingsMock({
 *         getBriefColumnWidthMode: () => 'paneWidth',
 *         getBriefColumnWidthMaxPx: () => 400,
 *       }))
 *     vi.mock('$lib/settings/settings-store', async () =>
 *       (await import('./test-file-list-mocks')).settingsStoreMock({
 *         'advanced.virtualizationBufferColumns': 2,
 *       }))
 *
 *     const list = await mountBriefList({ entries: [fileEntry({ name: 'a.txt' })] })
 *     expect(list.rowNames()).toEqual(['a.txt'])
 *
 * ⚠️ Same rule as `mountFullList`: assert the rows you expect are ON SCREEN
 * before asserting anything isn't on them.
 */

import { mount, tick, type ComponentProps } from 'svelte'
import { vi } from 'vitest'
import { installLayoutMock, type LayoutBox, type LayoutMock } from '$lib/test-layout'
import BriefList from './BriefList.svelte'
import type { FileEntry } from '../types'

type BriefListProps = ComponentProps<typeof BriefList>

/** The default surface: enough rows for a handful of items in one column. */
const DEFAULT_VIEWPORT: LayoutBox = { clientHeight: 400, clientWidth: 800 }

export interface MountBriefListOptions {
  /** Rows the backend listing holds. `totalCount` follows unless overridden. */
  entries?: FileEntry[]
  /** The scroll surface's measured box. Height ÷ row height sets what renders. */
  viewport?: LayoutBox
  props?: Partial<BriefListProps>
}

export interface MountedBriefList {
  target: HTMLElement
  layout: LayoutMock
  rows: () => HTMLElement[]
  rowNames: () => string[]
  settle: (until: () => boolean, reason: string) => Promise<void>
}

async function flush(): Promise<void> {
  await tick()
  await new Promise((resolve) => setTimeout(resolve, 0))
}

/** See `test-full-list.ts::settleUntil` for why this is condition-based, not a fixed round count. */
async function settleUntil(until: () => boolean, reason: string): Promise<void> {
  for (let round = 0; round < 50; round++) {
    if (until()) return
    await flush()
  }
  throw new Error(`mountBriefList: timed out waiting for ${reason}`)
}

/** Mounts a `BriefList` over a measured surface and waits for its first fetch. */
export async function mountBriefList(options: MountBriefListOptions = {}): Promise<MountedBriefList> {
  const entries = options.entries ?? []
  const { getFileRange } = await import('$lib/tauri-commands')
  vi.mocked(getFileRange).mockImplementation((_listingId, start: number, count: number) =>
    Promise.resolve(entries.slice(start, start + count)),
  )

  const layout = installLayoutMock({ '[data-file-list-surface]': options.viewport ?? DEFAULT_VIEWPORT })

  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(BriefList, {
    target,
    props: {
      listingId: 'listing-1',
      volumeId: 'root',
      totalCount: entries.length,
      includeHidden: false,
      cursorIndex: 0,
      isFocused: true,
      hasParent: false,
      parentPath: '',
      currentPath: '/root',
      sortBy: 'name',
      sortOrder: 'ascending',
      onSelect: () => {},
      onNavigate: () => {},
      ...options.props,
    },
  })

  const rows = () => [...target.querySelectorAll<HTMLElement>('.file-entry')]
  const rowNames = () => rows().map((row) => row.dataset.filename ?? '')

  if (entries.length > 0) {
    const firstName = entries[0].name
    await settleUntil(
      () => rowNames().includes(firstName),
      `the first entry (${firstName}) to render — is the viewport tall enough for it?`,
    )
  } else {
    await flush()
  }

  return { target, layout, rows, rowNames, settle: settleUntil }
}
