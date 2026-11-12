/**
 * The switcher's "Use Cmdr's fast direct connection" rows: which rows get asked about,
 * and what a pick does. What the switch MEANS is Rust's (`network/smb_direct_switch.rs`);
 * this pins the routing.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { DirectConnectionSwitch } from '#lib/ipc/bindings.js'

const getSmbDirectConnectionEnabled = vi.fn<(volumeId: string) => Promise<boolean | null>>()
const setSmbDirectConnectionEnabled = vi.fn<(volumeId: string, enabled: boolean) => Promise<DirectConnectionSwitch>>()
const connectDirectly = vi.fn((_args: { volumeId: string; shareName: string }) =>
  Promise.resolve({ kind: 'connected' }),
)

vi.mock('#lib/tauri-commands/index.js', () => ({
  getSmbDirectConnectionEnabled: (volumeId: string) => getSmbDirectConnectionEnabled(volumeId),
  setSmbDirectConnectionEnabled: (volumeId: string, enabled: boolean) =>
    setSmbDirectConnectionEnabled(volumeId, enabled),
}))

vi.mock('../network/direct-connect', () => ({
  connectDirectly: (args: { volumeId: string; shareName: string }) => connectDirectly(args),
}))

import { createDirectConnectionSwitches } from './direct-connection-switch.svelte'
import type { VolumeInfo } from '../types'

const osMountShare: VolumeInfo = {
  id: 'smb-testshare',
  name: 'testshare',
  path: '/Volumes/testshare',
  category: 'network',
  fsType: 'smbfs',
  isEjectable: true,
  connectionState: 'os_mount',
}

const directShare: VolumeInfo = { ...osMountShare, id: 'smb-media', name: 'Media', connectionState: 'direct' }

const disk: VolumeInfo = {
  id: 'volumes-backup',
  name: 'Backup',
  path: '/Volumes/Backup',
  category: 'attached_volume',
  isEjectable: true,
}

beforeEach(() => {
  vi.clearAllMocks()
})

describe('createDirectConnectionSwitches', () => {
  it('asks only about SMB share rows, and keeps no switch where Rust has none', async () => {
    getSmbDirectConnectionEnabled.mockImplementation((id) => Promise.resolve(id === 'smb-testshare' ? false : null))
    const switches = createDirectConnectionSwitches()

    await switches.fetchForRows([osMountShare, directShare, disk])

    expect(getSmbDirectConnectionEnabled.mock.calls.map(([id]) => id).sort()).toEqual(['smb-media', 'smb-testshare'])
    expect(switches.valueFor('smb-testshare')).toBe(false)
    expect(switches.valueFor('smb-media')).toBeUndefined()
    expect(switches.valueFor('volumes-backup')).toBeUndefined()
  })

  it('switching off only saves: Rust hands a direct share back itself', async () => {
    setSmbDirectConnectionEnabled.mockResolvedValue('returnedToOsMount')
    const switches = createDirectConnectionSwitches()

    await switches.pick(directShare, [directShare])

    expect(setSmbDirectConnectionEnabled).toHaveBeenCalledWith('smb-media', false)
    expect(connectDirectly).not.toHaveBeenCalled()
    expect(switches.valueFor('smb-media')).toBe(false)
  })

  it('switching on a share on the OS mount connects it directly, through the one connect flow', async () => {
    getSmbDirectConnectionEnabled.mockResolvedValue(false)
    setSmbDirectConnectionEnabled.mockResolvedValue('saved')
    const switches = createDirectConnectionSwitches()
    await switches.fetchForRows([osMountShare])

    await switches.pick(osMountShare, [osMountShare])

    expect(setSmbDirectConnectionEnabled).toHaveBeenCalledWith('smb-testshare', true)
    expect(connectDirectly).toHaveBeenCalledWith({ volumeId: 'smb-testshare', shareName: 'testshare' })
    expect(switches.valueFor('smb-testshare')).toBe(true)
  })

  it('a share that went away keeps its old value and starts no connect', async () => {
    getSmbDirectConnectionEnabled.mockResolvedValue(false)
    setSmbDirectConnectionEnabled.mockResolvedValue('notAnSmbShare')
    const switches = createDirectConnectionSwitches()
    await switches.fetchForRows([osMountShare])

    await switches.pick(osMountShare, [osMountShare])

    expect(connectDirectly).not.toHaveBeenCalled()
    expect(switches.valueFor('smb-testshare')).toBe(false)
  })
})
