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

  // A toggle that lands while an earlier one is still in flight must survive the earlier one's rollback.
  it('rolls back only its own change when an earlier toggle fails after a later one landed', async () => {
    let rejectFirst: (err: Error) => void = () => {}
    setNetworkVolumeEnabled.mockImplementationOnce(
      () =>
        new Promise<void>((_, reject) => {
          rejectFirst = reject
        }),
    )
    const first = prefs.setNetworkVolumeOptedIn('smb-1', true)
    await prefs.setNetworkVolumeOptedIn('smb-2', true)

    rejectFirst(new Error('backend down'))
    await expect(first).rejects.toThrow('backend down')

    expect(store.get('mediaIndex.networkVolumes')).toEqual(['smb-2'])
  })

  it('leaves an entry that was already there when re-adding it fails', async () => {
    store.set('mediaIndex.alwaysIndexVolumes', ['smb-1'])
    setAlwaysIndexVolume.mockRejectedValueOnce(new Error('nope'))
    await expect(prefs.setVolumeAlwaysIndexed('smb-1', true)).rejects.toThrow('nope')
    expect(store.get('mediaIndex.alwaysIndexVolumes')).toEqual(['smb-1'])
  })

  /**
   * ❗ A server that moved gives its place a new id (an SMB share at its first mount at the new address), and a
   * per-volume choice keyed by the old id would quietly switch off: the share's photos stop enriching.
   */
  describe('followVolumeMove', () => {
    it('carries the opt-in and the always-index choice to the new id, both persisted and live', async () => {
      store.set('mediaIndex.networkVolumes', ['smb-other', 'smb-old'])
      store.set('mediaIndex.alwaysIndexVolumes', ['smb-old'])

      await prefs.followVolumeMove('smb-old', 'smb-new')

      expect(store.get('mediaIndex.networkVolumes')).toEqual(['smb-other', 'smb-new'])
      expect(store.get('mediaIndex.alwaysIndexVolumes')).toEqual(['smb-new'])
      expect(setNetworkVolumeEnabled).toHaveBeenCalledWith('smb-new', true)
      expect(setNetworkVolumeEnabled).toHaveBeenCalledWith('smb-old', false)
      expect(setAlwaysIndexVolume).toHaveBeenCalledWith('smb-new', true)
      expect(setAlwaysIndexVolume).toHaveBeenCalledWith('smb-old', false)
    })

    it('touches nothing for a volume with no choice, or a move that kept its id', async () => {
      store.set('mediaIndex.networkVolumes', ['smb-other'])

      await prefs.followVolumeMove('smb-old', 'smb-new')
      await prefs.followVolumeMove('smb-other', 'smb-other')

      expect(store.get('mediaIndex.networkVolumes')).toEqual(['smb-other'])
      expect(setNetworkVolumeEnabled).not.toHaveBeenCalled()
      expect(setAlwaysIndexVolume).not.toHaveBeenCalled()
    })
  })
})
