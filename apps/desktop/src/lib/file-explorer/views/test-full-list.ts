/**
 * Mounting a real `FullList` in a unit test, over a surface that has a size.
 *
 * A layout-less DOM measures every element at zero, and `FullList` hands its
 * scroll surface's `clientHeight` straight to the virtual-window math — so
 * without a viewport it renders ZERO rows, silently, and a spec goes green while
 * asserting on an empty DOM. `mountFullList` supplies that viewport, a
 * `getFileRange` that serves the range it's asked for, and the twelve required
 * props, so a spec says what it's actually about.
 *
 * A spec pairs this with the module stand-ins from `test-file-list-mocks.ts`
 * (which must stay literal `vi.mock` calls — that's the only form Vitest hoists):
 *
 *     import { dirEntry, mountFullList } from './test-full-list'
 *
 *     vi.mock('$lib/tauri-commands', async () => (await import('./test-file-list-mocks')).tauriCommandsMock())
 *     vi.mock('$lib/icon-cache', async () => (await import('./test-file-list-mocks')).iconCacheMock())
 *     vi.mock('$lib/indexing/index-state.svelte', async () =>
 *       (await import('./test-file-list-mocks')).indexStateMock({ getWalkedGround: () => ['/root/a'] }),
 *     )
 *     vi.mock('$lib/settings/reactive-settings.svelte', async () =>
 *       (await import('./test-file-list-mocks')).reactiveSettingsMock(),
 *     )
 *     vi.mock('$lib/settings/settings-store', async () =>
 *       (await import('./test-file-list-mocks')).settingsStoreMock())
 *
 *     const list = await mountFullList({ entries: [dirEntry({ name: 'src' })] })
 *     expect(list.rowNames()).toEqual(['src'])
 *
 * ⚠️ Assert that the rows you expect are ON SCREEN before asserting that
 * something isn't on them. A negative assertion over an empty list passes for the
 * wrong reason, which is the failure mode this harness exists to close.
 *
 * The viewport mechanics are generic and live in `$lib/test-layout`.
 */

import { mount, tick, type ComponentProps } from 'svelte'
import { vi } from 'vitest'
import { installLayoutMock, type LayoutBox, type LayoutMock } from '$lib/test-layout'
import FullList from './FullList.svelte'
import type { FileEntry } from '../types'

// ============================================================================
// Entry fixtures
// ============================================================================

const BASE_ENTRY: FileEntry = {
  name: 'file.txt',
  path: '/root/file.txt',
  isDirectory: false,
  isSymlink: false,
  size: 1024,
  modifiedAt: 1710000000,
  iconId: 'ext:txt',
  permissions: 420,
  owner: 'test',
  group: 'staff',
  extendedMetadataLoaded: false,
}

/** A file row. `path` defaults to `/root/<name>`. */
export function fileEntry(overrides: Partial<FileEntry> & { name: string }): FileEntry {
  return { ...BASE_ENTRY, path: `/root/${overrides.name}`, ...overrides }
}

/**
 * A directory row carrying an exact, complete recursive size, which is the state
 * a folder is in once it has been indexed — so its size cell renders a number and
 * the hourglass on top is decided purely by whether that number is in flux.
 */
export function dirEntry(overrides: Partial<FileEntry> & { name: string }): FileEntry {
  return {
    ...BASE_ENTRY,
    isDirectory: true,
    iconId: 'folder',
    size: undefined,
    recursiveSize: 4096,
    recursiveFileCount: 3,
    recursiveDirCount: 1,
    recursiveSizeComplete: true,
    path: `/root/${overrides.name}`,
    ...overrides,
  }
}

// ============================================================================
// Mounting
// ============================================================================

type FullListProps = ComponentProps<typeof FullList>

/** The default surface: 400 px of rows, with an overlay (zero-width) scrollbar. */
const DEFAULT_VIEWPORT: LayoutBox = { clientHeight: 400, clientWidth: 800, offsetWidth: 800 }

export interface MountFullListOptions {
  /** Rows the backend listing holds. `totalCount` follows unless overridden. */
  entries?: FileEntry[]
  /** The scroll surface's measured box. Height ÷ row height sets what renders. */
  viewport?: LayoutBox
  props?: Partial<FullListProps>
}

export interface MountedFullList {
  target: HTMLElement
  /** Resize or scroll the surface and let the component react. */
  layout: LayoutMock
  rows: () => HTMLElement[]
  rowNames: () => string[]
  /** The rendered rows showing the size-updating hourglass, by filename. */
  hourglassRowNames: () => string[]
  /**
   * Drives effects and IPC promises until `until` holds, throwing `reason` if it
   * never does. Use it after a `layout.resize` / `layout.scroll`, and phrase
   * `reason` as a noun phrase: it completes "timed out waiting for …".
   */
  settle: (until: () => boolean, reason: string) => Promise<void>
}

/** One round of "let Svelte's effects run and pending promises resolve". */
async function flush(): Promise<void> {
  await tick()
  await new Promise((resolve) => setTimeout(resolve, 0))
}

/**
 * Flushes until `until` holds, and THROWS when it never does.
 *
 * ❌ Not a fixed number of rounds. The number of hops between a mount and the
 * first painted row is an implementation detail (an effect, an IPC promise, the
 * re-render it triggers), so a guessed count silently renders fewer rows — or
 * none — the day one is added, which puts every spec built on this straight back
 * into passing over an empty DOM. The bound below is a runaway backstop, far
 * above the real cost; it's the condition that decides.
 */
async function settleUntil(until: () => boolean, reason: string): Promise<void> {
  for (let round = 0; round < 50; round++) {
    if (until()) return
    await flush()
  }
  throw new Error(`mountFullList: timed out waiting for ${reason}`)
}

/** Mounts a `FullList` over a measured surface and waits for its first fetch. */
export async function mountFullList(options: MountFullListOptions = {}): Promise<MountedFullList> {
  const entries = options.entries ?? []
  const { getFileRange } = await import('$lib/tauri-commands')
  vi.mocked(getFileRange).mockImplementation((_listingId, start: number, count: number) =>
    Promise.resolve(entries.slice(start, start + count)),
  )

  const layout = installLayoutMock({ '[data-file-list-surface]': options.viewport ?? DEFAULT_VIEWPORT })

  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(FullList, {
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

  // The FIRST BACKEND ENTRY has to be on screen before the spec asserts anything,
  // or it's about to run over an empty DOM. ❌ Not "some row rendered": the `..`
  // row is synthetic and paints immediately, so on a `hasParent` listing that
  // condition holds a full IPC round-trip before any real entry arrives. At mount
  // the window starts at index 0, so `entries[0]` is always in it (given a
  // viewport tall enough for two rows). An empty listing has nothing to wait for —
  // the empty-state branch renders instead — so it only needs the effects to run.
  if (entries.length > 0) {
    const firstName = entries[0].name
    await settleUntil(
      () => rowNames().includes(firstName),
      `the first entry (${firstName}) to render — is the viewport tall enough for it?`,
    )
  } else {
    await flush()
  }

  return {
    target,
    layout,
    rows,
    rowNames,
    hourglassRowNames: () =>
      rows()
        .filter((row) => row.querySelector('.size-updating') !== null)
        .map((row) => row.dataset.filename ?? ''),
    settle: settleUntil,
  }
}
