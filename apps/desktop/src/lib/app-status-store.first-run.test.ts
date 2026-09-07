/**
 * First-run facts the app status store has to get right.
 *
 * Two contracts live here:
 *
 *  (a) a fresh install opens both panes on the home folder, and
 *  (b) the `firstRunLayoutApplied` marker survives a round trip to disk.
 *
 * (b) is worth its own test because `doSaveAppStatus` persists only the fields it
 * enumerates one by one: a key missing from that list saves nothing, silently, and the
 * one-shot layout would then fire on every launch.
 *
 * A shared `disk` Map backs a fake `@tauri-apps/plugin-store`. The store's own cached
 * handle is module-scoped, so `vi.resetModules()` plus a fresh dynamic import is what
 * stands in for a relaunch re-reading the file.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { loadAppStatus, loadPaneTabs, hasPersistedPaneState, saveAppStatusNow } from './app-status-store'

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

/** Everything exists, so no walk-up fallback kicks in. */
const alwaysExists = () => Promise.resolve(true)

beforeEach(() => {
  disk.clear()
})

describe('a fresh install', () => {
  it('opens both panes on the home folder', async () => {
    const status = await loadAppStatus(alwaysExists)
    expect(status.leftPath).toBe('~')
    expect(status.rightPath).toBe('~')
  })

  it('starts a single home tab per pane', async () => {
    for (const side of ['left', 'right'] as const) {
      const paneTabs = await loadPaneTabs(side, alwaysExists)
      expect(paneTabs.tabs.map((t) => t.path)).toEqual(['~'])
    }
  })

  it('reports no first-run layout yet, and no persisted pane state', async () => {
    expect((await loadAppStatus(alwaysExists)).firstRunLayoutApplied).toBe(false)
    expect(await hasPersistedPaneState()).toBe(false)
  })
})

describe('hasPersistedPaneState', () => {
  it('counts an empty tab list as a prior install', async () => {
    // Key PRESENCE is the signal, never tab content: a user who closed every tab still
    // has a layout of their own, and the one-shot rule must stay away from it.
    disk.set('leftTabs', { tabs: [], activeTabId: '' })
    expect(await hasPersistedPaneState()).toBe(true)
  })

  it('counts the legacy scalar path key as a prior install', async () => {
    disk.set('leftPath', '~/projects')
    expect(await hasPersistedPaneState()).toBe(true)
  })

  it('counts either side on its own', async () => {
    // Nav-state persists per pane, so someone who only ever moved their right pane has
    // no left keys at all. Checking one side would hand them the layout and lose theirs.
    for (const key of ['leftTabs', 'rightTabs', 'leftPath', 'rightPath']) {
      disk.clear()
      disk.set(key, key.endsWith('Tabs') ? { tabs: [], activeTabId: '' } : '~/projects')
      expect(await hasPersistedPaneState()).toBe(true)
    }
  })

  it('ignores unrelated keys', async () => {
    disk.set('focusedPane', 'right')
    disk.set('askCmdrRailOpen', true)
    expect(await hasPersistedPaneState()).toBe(false)
  })
})

describe('the first-run layout marker', () => {
  it('survives a relaunch once written', async () => {
    await saveAppStatusNow({ firstRunLayoutApplied: true })
    expect(disk.get('firstRunLayoutApplied')).toBe(true)

    vi.resetModules()
    const afterRelaunch = await import('./app-status-store')
    expect((await afterRelaunch.loadAppStatus(alwaysExists)).firstRunLayoutApplied).toBe(true)
  })

  it('reads as unset when the stored value is not a boolean true', async () => {
    disk.set('firstRunLayoutApplied', 'yes')
    expect((await loadAppStatus(alwaysExists)).firstRunLayoutApplied).toBe(false)
  })
})

describe('saveAppStatusNow', () => {
  it('writes without waiting for the debounce', async () => {
    // Startup is followed by things that can quit the app, so the marker can't sit in a
    // 200 ms timer. No fake timers here: the write has to have landed on return.
    await saveAppStatusNow({ leftPath: '~/one', rightPath: '~/two' })
    expect(disk.get('leftPath')).toBe('~/one')
    expect(disk.get('rightPath')).toBe('~/two')
  })
})

/**
 * Restoring a tab that stood on a server.
 *
 * ❗ Launch must not dial: four saved servers waking a Mac would be four Keychain
 * reads and four network waits nobody asked for. So the path is taken as it
 * stands, and the pane dials only when the user activates the tab
 * (`src/lib/file-explorer/navigation/DETAILS.md` § "Restoring a remote path").
 * The failure this catches is quiet and
 * total: probing a remote path answers false for every parent, the walk-up chops
 * the scheme, and the tab reopens on the boot disk's home folder.
 */
describe('a restored server tab', () => {
  const neverExists = vi.fn(() => Promise.resolve(false))

  beforeEach(() => {
    neverExists.mockClear()
  })

  it('keeps its subpath, and nothing probes the server', async () => {
    disk.set('leftTabs', {
      activeTabId: 'tab-1',
      tabs: [
        {
          id: 'tab-1',
          path: 'sftp://ada@nas.local:22/srv/data/photos',
          volumeId: 'sftp-nas-local-22-ada',
          sortBy: 'name',
          sortOrder: 'ascending',
          viewMode: 'full',
          pinned: false,
        },
      ],
    })
    const paneTabs = await loadPaneTabs('left', neverExists)
    expect(paneTabs.tabs[0].path).toBe('sftp://ada@nas.local:22/srv/data/photos')
    expect(paneTabs.tabs[0].volumeId).toBe('sftp-nas-local-22-ada')
    expect(neverExists).not.toHaveBeenCalled()
  })

  it('still walks up a LOCAL tab that went missing', async () => {
    disk.set('leftTabs', {
      activeTabId: 'tab-1',
      tabs: [
        {
          id: 'tab-1',
          path: '/Volumes/gone/photos',
          volumeId: 'ext',
          sortBy: 'name',
          sortOrder: 'ascending',
          viewMode: 'full',
          pinned: false,
        },
      ],
    })
    const paneTabs = await loadPaneTabs('left', neverExists)
    expect(paneTabs.tabs[0].path).toBe('~')
    expect(neverExists).toHaveBeenCalled()
  })

  it('leaves both pane paths on their servers too', async () => {
    disk.set('leftVolumeId', 'sftp-nas-local-22-ada')
    disk.set('leftPath', 'sftp://ada@nas.local:22/srv/data')
    disk.set('rightVolumeId', 'root')
    disk.set('rightPath', '/Users/ada')
    const status = await loadAppStatus(neverExists)
    expect(status.leftPath).toBe('sftp://ada@nas.local:22/srv/data')
    expect(status.rightPath).toBe('~')
  })
})
