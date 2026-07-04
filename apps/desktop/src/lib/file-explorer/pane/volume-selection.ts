/**
 * Volume selection by index / name for a pane — the MCP `select_volume` tool and
 * the palette's volume commands. Lifted out of `DualPaneExplorer`; the component
 * keeps the one-line `export function selectVolumeByName` delegate.
 *
 * Both routes fold onto `navigate({ to: { selectVolume }, source: 'user' })`, so
 * the standard volume-switch mechanics (focus shift, history push, new-tab-on-
 * pinned) apply uniformly. Matches `VolumeBreadcrumb`'s `handleVolumeSelect`: a
 * favorite navigates to its path on the containing volume; a real volume opens at
 * its root; the virtual `Network` volume isn't in the volumes list, so it's
 * special-cased. The switch arm shifts STORE focus but not DOM focus — re-
 * anchoring the container would drop a Space press during the multi-select-then-
 * delete sequence (regression guard: mtp.spec.ts).
 */

import { resolvePathVolume } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import type { VolumeInfo } from '../types'
import type { NavigateIntent, NavigateResult } from './navigate'

const log = getAppLogger('fileExplorer')

export interface VolumeSelectionDeps {
    getVolumes: () => VolumeInfo[]
    navigate: (intent: NavigateIntent) => NavigateResult
}

export interface VolumeSelection {
    /** Select a volume by zero-based index into the volumes array. */
    selectVolumeByIndex: (pane: 'left' | 'right', index: number) => Promise<boolean>
    /** Select a volume by name (MCP `select_volume`). "Network" is virtual. */
    selectVolumeByName: (pane: 'left' | 'right', name: string) => Promise<boolean>
}

export function createVolumeSelection(deps: VolumeSelectionDeps): VolumeSelection {
    async function selectVolumeByIndex(pane: 'left' | 'right', index: number): Promise<boolean> {
        const volumes = deps.getVolumes()
        if (index < 0 || index >= volumes.length) {
            log.warn('Invalid volume index: {index} (valid range: 0-{max})', { index, max: volumes.length - 1 })
            return false
        }

        const volume = volumes[index]

        // Handle favorites differently from actual volumes (same as VolumeBreadcrumb).
        if (volume.category === 'favorite') {
            // For favorites, navigate to the favorite's path on its containing volume.
            const { volume: containingVolume } = await resolvePathVolume(volume.path)
            const volumeId = containingVolume?.id ?? 'root'
            deps.navigate({ pane, to: { selectVolume: { volumeId, path: volume.path } }, source: 'user' })
        } else {
            // For actual volumes, navigate to the volume's root.
            deps.navigate({ pane, to: { selectVolume: { volumeId: volume.id, path: volume.path } }, source: 'user' })
        }

        return true
    }

    async function selectVolumeByName(pane: 'left' | 'right', name: string): Promise<boolean> {
        // "Network" is a virtual volume not in the volumes list
        if (name === 'Network') {
            deps.navigate({ pane, to: { selectVolume: { volumeId: 'network', path: 'smb://' } }, source: 'user' })
            return true
        }

        const index = deps.getVolumes().findIndex((v) => v.name === name)
        if (index !== -1) {
            return selectVolumeByIndex(pane, index)
        }

        log.warn('Volume not found: {name}', { name })
        return false
    }

    return { selectVolumeByIndex, selectVolumeByName }
}
