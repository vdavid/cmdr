/**
 * The module stand-ins a file-list spec needs, as factories its `vi.mock` calls
 * spread in.
 *
 * `FullList` reaches for five modules that have no meaning outside the app: IPC,
 * the icon cache, live index state, and the two settings layers. Getting one of
 * them incomplete fails QUIETLY — `fetchVisibleRange` swallows every throw, so a
 * missing export empties the list, and a missing numeric setting turns the fetch
 * range into `NaN` — leaving a green spec asserting on an empty DOM. Hence one
 * complete set here instead of a hand-rolled pile per spec.
 *
 * ⚠️ This file must NOT import a component. A `vi.mock` factory that reaches
 * back into a module the component itself imports deadlocks the run: the spec's
 * top-level import starts loading the component, the component asks for the
 * mocked module, and the factory waits on the import that's already in flight.
 * Mounting lives in `test-full-list.ts` for exactly that reason.
 *
 * Usage, and what a spec then does with a mounted list: `test-full-list.ts`.
 */

import { vi } from 'vitest'
import { writable } from 'svelte/store'
import type { FileEntry } from '../types'

/**
 * `$lib/tauri-commands`. `getFileRange` starts empty; `mountFullList` replaces it
 * with one that serves the requested `(start, count)` slice.
 */
export function tauriCommandsMock(overrides: Record<string, unknown> = {}) {
  return {
    getFileRange: vi.fn(() => Promise.resolve([] as FileEntry[])),
    // An ARRAY, positionally matching the requested paths — `syncParentDirStats`
    // reads `results[0]`, so an object here silently leaves the `..` row blank.
    getDirStatsBatch: vi.fn(() => Promise.resolve([null])),
    enrichTags: vi.fn(() => Promise.resolve()),
    ...overrides,
  }
}

/** `$lib/icon-cache`. Every export the listing path touches, all inert. */
export function iconCacheMock(overrides: Record<string, unknown> = {}) {
  return {
    getCachedIcon: () => undefined,
    getCachedCustomFolderIcon: () => undefined,
    iconCacheVersion: writable(0),
    iconCacheCleared: writable(0),
    prefetchIcons: vi.fn(),
    prefetchCustomFolderIcons: vi.fn(),
    ...overrides,
  }
}

/**
 * `$lib/indexing/index-state.svelte`: nothing is moving on any volume. Override
 * `getWalkedGround` / `isVolumeAggregating` to put a drive under a walker.
 */
export function indexStateMock(overrides: Record<string, unknown> = {}) {
  return {
    isVolumeScanning: () => false,
    isVolumeAggregating: () => false,
    getWalkedGround: () => [],
    ...overrides,
  }
}

/** `$lib/settings/reactive-settings.svelte` at its shipped defaults. */
export function reactiveSettingsMock(overrides: Record<string, unknown> = {}) {
  return {
    getRowHeight: () => 20,
    getIconSize: () => 16,
    getIsCompactDensity: () => false,
    getIsCmdrGold: () => false,
    getUseAppIconsForDocuments: () => true,
    formatDateTime: (t: number | undefined) => (t ? '2025-03-14 10:30' : ''),
    formattedDate: (t: number | undefined) =>
      t
        ? { text: '2025-03-14 10:30', segments: [{ text: '2025-03-14 10:30', ageClass: null }] }
        : { text: '', segments: [] },
    formatFileSize: (n: number) => `${String(n)} B`,
    getSizeDisplayMode: () => 'smart',
    getSizeMismatchWarning: () => false,
    getStripedRows: () => false,
    getShowExtensionInName: () => false,
    getShowTags: () => false,
    getFileSizeUnit: () => 'bytes',
    getFileSizeFormat: () => 'binary',
    ...overrides,
  }
}

/**
 * `$lib/settings/settings-store`. Both buffer sizes MUST be numbers: the fetch
 * range is arithmetic on them, and an absent one makes it `NaN`, which fetches
 * nothing and leaves the list empty with no complaint.
 */
export function settingsStoreMock(values: Record<string, unknown> = {}) {
  const settings: Record<string, unknown> = {
    'advanced.virtualizationBufferRows': 20,
    'advanced.prefetchBufferSize': 0,
    ...values,
  }
  return { getSetting: (key: string) => settings[key] }
}
