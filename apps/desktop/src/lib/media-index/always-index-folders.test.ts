import { describe, expect, it, vi, beforeEach } from 'vitest'
// Static import of the module under test (satisfies `custom/no-isolated-tests`); the
// `vi.hoisted` mocks below are hoisted above it, so they apply before it loads.
import * as prefs from './always-index-folders'

// `vi.hoisted` so the mock factories can close over these before the static import runs.
const { store, getSetting, setSetting, setAlwaysIndexFolder } = vi.hoisted(() => {
  const store = new Map<string, unknown>()
  return {
    store,
    getSetting: vi.fn((id: string): unknown => store.get(id) ?? []),
    setSetting: vi.fn((id: string, value: unknown) => store.set(id, value)),
    setAlwaysIndexFolder: vi.fn<(folder: string, always: boolean) => Promise<void>>(),
  }
})

vi.mock('$lib/settings', () => ({
  getSetting: (id: string) => getSetting(id),
  setSetting: (id: string, value: unknown) => setSetting(id, value),
}))

vi.mock('$lib/tauri-commands', () => ({
  mediaIndexSetAlwaysIndexFolder: (folder: string, always: boolean) => setAlwaysIndexFolder(folder, always),
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

describe('always-index-folders', () => {
  beforeEach(() => {
    store.clear()
    vi.clearAllMocks()
    setAlwaysIndexFolder.mockResolvedValue()
  })

  it('starts empty: a fresh install has chosen no folder', () => {
    expect(prefs.getChosenFolders()).toEqual([])
    expect(prefs.isFolderChosen('/Users/me/Photos')).toBe(false)
  })

  it('choosing a folder persists the path and live-applies via IPC', async () => {
    // The IPC half is what kicks the indexing pass backend-side, so it can't be skipped.
    await prefs.setFolderChosen('/Users/me/Photos', true)
    expect(store.get('mediaIndex.alwaysIndexFolders')).toEqual(['/Users/me/Photos'])
    expect(setAlwaysIndexFolder).toHaveBeenCalledWith('/Users/me/Photos', true)
    expect(prefs.isFolderChosen('/Users/me/Photos')).toBe(true)
  })

  it('removing a folder drops the path and live-applies', async () => {
    store.set('mediaIndex.alwaysIndexFolders', ['/a', '/b'])
    await prefs.setFolderChosen('/a', false)
    expect(store.get('mediaIndex.alwaysIndexFolders')).toEqual(['/b'])
    expect(setAlwaysIndexFolder).toHaveBeenCalledWith('/a', false)
  })

  it('is idempotent: re-choosing an already-chosen folder keeps a single entry', async () => {
    store.set('mediaIndex.alwaysIndexFolders', ['/a'])
    await prefs.setFolderChosen('/a', true)
    expect(store.get('mediaIndex.alwaysIndexFolders')).toEqual(['/a'])
  })

  it('rolls the persisted choice back when the IPC call rejects', async () => {
    setAlwaysIndexFolder.mockRejectedValueOnce(new Error('backend down'))
    await expect(prefs.setFolderChosen('/a', true)).rejects.toThrow('backend down')
    // The optimistic write was reverted so the store and backend stay in agreement.
    expect(store.get('mediaIndex.alwaysIndexFolders')).toEqual([])
  })
})
