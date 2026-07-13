import { describe, expect, it, vi, beforeEach } from 'vitest'
// Static import of the module under test (satisfies `custom/no-isolated-tests`); the
// `vi.hoisted` mocks below are hoisted above it, so they apply before it loads.
import * as prefs from './network-volume-prefs'

// `vi.hoisted` so the mock factories can close over these before the static import runs.
const { store, getSetting, setSetting, setNetworkVolumeEnabled, setAlwaysIndexVolume } = vi.hoisted(() => {
  const store = new Map<string, unknown>()
  return {
    store,
    getSetting: vi.fn((id: string): unknown => store.get(id) ?? []),
    setSetting: vi.fn((id: string, value: unknown) => store.set(id, value)),
    setNetworkVolumeEnabled: vi.fn<(volumeId: string, enabled: boolean) => Promise<void>>(),
    setAlwaysIndexVolume: vi.fn<(volumeId: string, always: boolean) => Promise<void>>(),
  }
})

vi.mock('$lib/settings', () => ({
  getSetting: (id: string) => getSetting(id),
  setSetting: (id: string, value: unknown) => setSetting(id, value),
}))

vi.mock('$lib/tauri-commands', () => ({
  mediaIndexSetNetworkVolumeEnabled: (v: string, e: boolean) => setNetworkVolumeEnabled(v, e),
  mediaIndexSetAlwaysIndexVolume: (v: string, a: boolean) => setAlwaysIndexVolume(v, a),
}))

describe('network-volume-prefs', () => {
  beforeEach(() => {
    store.clear()
    vi.clearAllMocks()
    setNetworkVolumeEnabled.mockResolvedValue()
    setAlwaysIndexVolume.mockResolvedValue()
  })

  it('opting a volume in persists the id and live-applies via IPC', async () => {
    await prefs.setNetworkVolumeOptedIn('smb-1', true)
    expect(store.get('mediaIndex.networkVolumes')).toEqual(['smb-1'])
    expect(setNetworkVolumeEnabled).toHaveBeenCalledWith('smb-1', true)
    expect(prefs.isNetworkVolumeOptedIn('smb-1')).toBe(true)
  })

  it('opting out removes the id and live-applies', async () => {
    store.set('mediaIndex.networkVolumes', ['smb-1', 'smb-2'])
    await prefs.setNetworkVolumeOptedIn('smb-1', false)
    expect(store.get('mediaIndex.networkVolumes')).toEqual(['smb-2'])
    expect(setNetworkVolumeEnabled).toHaveBeenCalledWith('smb-1', false)
    expect(prefs.isNetworkVolumeOptedIn('smb-1')).toBe(false)
  })

  it('is idempotent: opting in an already-opted-in volume keeps a single entry', async () => {
    store.set('mediaIndex.networkVolumes', ['smb-1'])
    await prefs.setNetworkVolumeOptedIn('smb-1', true)
    expect(store.get('mediaIndex.networkVolumes')).toEqual(['smb-1'])
  })

  it('rolls the persisted opt-in back when the IPC call rejects', async () => {
    setNetworkVolumeEnabled.mockRejectedValueOnce(new Error('backend down'))
    await expect(prefs.setNetworkVolumeOptedIn('smb-1', true)).rejects.toThrow('backend down')
    // The optimistic write was reverted so the store and backend stay in agreement.
    expect(store.get('mediaIndex.networkVolumes')).toEqual([])
  })

  it('always-index override persists and live-applies independently of the opt-in', async () => {
    await prefs.setVolumeAlwaysIndexed('smb-1', true)
    expect(store.get('mediaIndex.alwaysIndexVolumes')).toEqual(['smb-1'])
    expect(setAlwaysIndexVolume).toHaveBeenCalledWith('smb-1', true)
    expect(prefs.isVolumeAlwaysIndexed('smb-1')).toBe(true)
    // The opt-in array is untouched.
    expect(prefs.getNetworkOptInVolumes()).toEqual([])
  })

  it('rolls the always-index override back on IPC failure', async () => {
    store.set('mediaIndex.alwaysIndexVolumes', ['smb-9'])
    setAlwaysIndexVolume.mockRejectedValueOnce(new Error('nope'))
    await expect(prefs.setVolumeAlwaysIndexed('smb-1', true)).rejects.toThrow('nope')
    expect(store.get('mediaIndex.alwaysIndexVolumes')).toEqual(['smb-9'])
  })
})
