/**
 * What loading a stored tab does with a path on an SMB share.
 *
 * ❗ The store hands it back UNPROBED: an unmounted share has no folder on disk, so a
 * walk here would shorten a path that was right to `/Volumes` before anyone could tell
 * whether the share is a saved place. `file-explorer/pane/initialization.ts` decides,
 * with the saved list in hand. Same fake `@tauri-apps/plugin-store` rig as
 * `app-status-store.snapshot-paths.test.ts`.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { loadPaneTabs } from './app-status-store'
import type { PersistedTab } from './file-explorer/tabs/tab-types'

const disk = vi.hoisted(() => new Map<string, unknown>())

vi.mock('@tauri-apps/plugin-store', () => ({
  load: vi.fn(() =>
    Promise.resolve({
      get: (key: string) => Promise.resolve(disk.get(key)),
      set: (key: string, value: unknown) => {
        disk.set(key, value)
        return Promise.resolve()
      },
      delete: (key: string) => Promise.resolve(disk.delete(key)),
      has: (key: string) => Promise.resolve(disk.has(key)),
      keys: () => Promise.resolve([...disk.keys()]),
      save: () => Promise.resolve(),
    }),
  ),
}))

vi.mock('./settings/store-path', () => ({
  resolveStorePath: (name: string) => Promise.resolve(name),
}))

/** Only the boot disk's own folders exist: the share isn't mounted. */
const onlyBootDisk = (p: string) => Promise.resolve(p === '/' || p === '/Volumes' || p === '~')

function storedTab(overrides: Partial<PersistedTab>): PersistedTab {
  return {
    id: 'tab-1',
    path: '/Volumes/naspi/docs',
    volumeId: 'smb-naspolya-445-naspi',
    sortBy: 'name',
    sortOrder: 'ascending',
    viewMode: 'full',
    pinned: false,
    ...overrides,
  }
}

beforeEach(() => {
  disk.clear()
})

describe('a stored tab on an SMB share', () => {
  it('comes back on its own folder, unprobed', async () => {
    disk.set('leftTabs', { tabs: [storedTab({})], activeTabId: 'tab-1' })

    const paneTabs = await loadPaneTabs('left', onlyBootDisk)

    expect(paneTabs.tabs[0]).toMatchObject({ path: '/Volumes/naspi/docs', volumeId: 'smb-naspolya-445-naspi' })
  })

  it('still walks a local tab whose folder is gone', async () => {
    disk.set('leftTabs', { tabs: [storedTab({ volumeId: 'root', path: '/Volumes/gone/docs' })], activeTabId: 'tab-1' })

    const paneTabs = await loadPaneTabs('left', onlyBootDisk)

    expect(paneTabs.tabs[0]).toMatchObject({ path: '/Volumes', volumeId: 'root' })
  })
})
