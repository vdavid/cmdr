import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { VolumeInfo } from '../types'
import type { NavigateIntent, NavigateResult } from './navigate'

const { resolvePathVolumeSpy } = vi.hoisted(() => ({
    resolvePathVolumeSpy: vi.fn<() => Promise<{ volume: { id: string } | null }>>(),
}))

vi.mock('$lib/tauri-commands', () => ({ resolvePathVolume: resolvePathVolumeSpy }))
vi.mock('$lib/logging/logger', () => ({
    getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { createVolumeSelection, type VolumeSelectionDeps } from './volume-selection'

function vol(over: Partial<VolumeInfo>): VolumeInfo {
    return { id: 'v', name: 'V', path: '/v', category: 'device', ...over } as unknown as VolumeInfo
}

function setup(volumes: VolumeInfo[]) {
    const navigate = vi.fn<(intent: NavigateIntent) => NavigateResult>(
        () => ({ status: 'started' }) as unknown as NavigateResult,
    )
    const deps: VolumeSelectionDeps = { getVolumes: () => volumes, navigate }
    return { ops: createVolumeSelection(deps), navigate }
}

describe('createVolumeSelection', () => {
    beforeEach(() => vi.clearAllMocks())

    it('selectVolumeByName("Network") navigates to the virtual network volume', async () => {
        const { ops, navigate } = setup([])
        const ok = await ops.selectVolumeByName('left', 'Network')
        expect(ok).toBe(true)
        expect(navigate).toHaveBeenCalledWith({
            pane: 'left',
            to: { selectVolume: { volumeId: 'network', path: 'smb://' } },
            source: 'user',
        })
    })

    it('selectVolumeByName for a real volume opens it at its root', async () => {
        const { ops, navigate } = setup([vol({ id: 'usb', name: 'USB', path: '/Volumes/USB' })])
        const ok = await ops.selectVolumeByName('right', 'USB')
        expect(ok).toBe(true)
        expect(navigate).toHaveBeenCalledWith({
            pane: 'right',
            to: { selectVolume: { volumeId: 'usb', path: '/Volumes/USB' } },
            source: 'user',
        })
    })

    it('selectVolumeByName for a favorite navigates to its path on the containing volume', async () => {
        resolvePathVolumeSpy.mockResolvedValue({ volume: { id: 'root' } })
        const { ops, navigate } = setup([
            vol({ id: 'fav', name: 'Docs', path: '/Users/me/Docs', category: 'favorite' }),
        ])
        const ok = await ops.selectVolumeByName('left', 'Docs')
        expect(ok).toBe(true)
        expect(resolvePathVolumeSpy).toHaveBeenCalledWith('/Users/me/Docs')
        expect(navigate).toHaveBeenCalledWith({
            pane: 'left',
            to: { selectVolume: { volumeId: 'root', path: '/Users/me/Docs' } },
            source: 'user',
        })
    })

    it('selectVolumeByName returns false and does not navigate when the name is unknown', async () => {
        const { ops, navigate } = setup([vol({ name: 'USB' })])
        const ok = await ops.selectVolumeByName('left', 'Nope')
        expect(ok).toBe(false)
        expect(navigate).not.toHaveBeenCalled()
    })

    it('selectVolumeByIndex out of range returns false', async () => {
        const { ops, navigate } = setup([vol({})])
        expect(await ops.selectVolumeByIndex('left', 5)).toBe(false)
        expect(navigate).not.toHaveBeenCalled()
    })
})
