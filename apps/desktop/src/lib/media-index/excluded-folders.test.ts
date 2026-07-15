import { describe, expect, it, vi, beforeEach } from 'vitest'
// Static import of the module under test (satisfies `custom/no-isolated-tests`); the
// `vi.hoisted` mocks below are hoisted above it, so they apply before it loads.
import * as prefs from './excluded-folders'

// `vi.hoisted` so the mock factories can close over these before the static import runs.
const { store, getSetting, setSetting, setExcludedFolder } = vi.hoisted(() => {
  const store = new Map<string, unknown>()
  return {
    store,
    getSetting: vi.fn((id: string): unknown => store.get(id) ?? []),
    setSetting: vi.fn((id: string, value: unknown) => store.set(id, value)),
    setExcludedFolder: vi.fn<(folder: string, excluded: boolean) => Promise<void>>(),
  }
})

vi.mock('$lib/settings', () => ({
  getSetting: (id: string) => getSetting(id),
  setSetting: (id: string, value: unknown) => setSetting(id, value),
}))

vi.mock('$lib/tauri-commands', () => ({
  mediaIndexSetExcludedFolder: (folder: string, excluded: boolean) => setExcludedFolder(folder, excluded),
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

describe('excluded-folders', () => {
  beforeEach(() => {
    store.clear()
    vi.clearAllMocks()
    setExcludedFolder.mockResolvedValue()
  })

  it('excluding a folder persists the path and live-applies via IPC', async () => {
    await prefs.setFolderExcluded('/Users/me/Documents/IDs', true)
    expect(store.get('mediaIndex.excludedFolders')).toEqual(['/Users/me/Documents/IDs'])
    expect(setExcludedFolder).toHaveBeenCalledWith('/Users/me/Documents/IDs', true)
  })

  it('un-excluding removes the path and live-applies', async () => {
    store.set('mediaIndex.excludedFolders', ['/a', '/b'])
    await prefs.setFolderExcluded('/a', false)
    expect(store.get('mediaIndex.excludedFolders')).toEqual(['/b'])
    expect(setExcludedFolder).toHaveBeenCalledWith('/a', false)
  })

  it('is idempotent: excluding an already-excluded folder keeps a single entry', async () => {
    store.set('mediaIndex.excludedFolders', ['/a'])
    await prefs.setFolderExcluded('/a', true)
    expect(store.get('mediaIndex.excludedFolders')).toEqual(['/a'])
  })

  it('rolls the persisted exclusion back when the IPC call rejects', async () => {
    setExcludedFolder.mockRejectedValueOnce(new Error('backend down'))
    await expect(prefs.setFolderExcluded('/a', true)).rejects.toThrow('backend down')
    // The optimistic write was reverted so the store and backend stay in agreement.
    expect(store.get('mediaIndex.excludedFolders')).toEqual([])
  })
})
