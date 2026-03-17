import { SvelteMap, SvelteSet } from 'svelte/reactivity'
import { getVolumeSpace, type VolumeSpaceInfo } from '$lib/tauri-commands'
import { withTimeout } from '$lib/utils/timing'
import type { VolumeInfo } from '../types'

const volumeSpaceTimeoutMs = 3000
const autoRetryDelayMs = 5000
const shakeAnimationMs = 400

export interface VolumeSpaceManager {
    volumeSpaceMap: SvelteMap<string, VolumeSpaceInfo>
    spaceTimedOutSet: SvelteSet<string>
    spaceRetryingSet: SvelteSet<string>
    spaceRetryFailedSet: SvelteSet<string>
    spaceRetryAttemptedSet: SvelteSet<string>
    spaceAutoRetryingSet: SvelteSet<string>
    fetchVolumeSpaces: (vols: VolumeInfo[]) => Promise<void>
    retryVolumeSpace: (volume: VolumeInfo) => void
    clearAll: () => void
    destroy: () => void
}

export function createVolumeSpaceManager(): VolumeSpaceManager {
    const volumeSpaceMap = new SvelteMap<string, VolumeSpaceInfo>()
    const spaceTimedOutSet = new SvelteSet<string>()
    const spaceRetryingSet = new SvelteSet<string>()
    const spaceRetryFailedSet = new SvelteSet<string>()
    const spaceRetryAttemptedSet = new SvelteSet<string>()
    const spaceAutoRetryingSet = new SvelteSet<string>()
    const autoRetryTimers: ReturnType<typeof setTimeout>[] = []

    function handleRetryFailure(volId: string) {
        spaceRetryingSet.delete(volId)
        spaceAutoRetryingSet.delete(volId)
        spaceRetryFailedSet.add(volId)
        // Clear the shake trigger after the animation completes (~300ms)
        setTimeout(() => {
            spaceRetryFailedSet.delete(volId)
        }, shakeAnimationMs)
    }

    async function doRetryVolumeSpace(volume: VolumeInfo, isAutoRetry: boolean) {
        spaceRetryingSet.add(volume.id)
        spaceRetryFailedSet.delete(volume.id)
        spaceRetryAttemptedSet.add(volume.id)
        if (isAutoRetry) spaceAutoRetryingSet.add(volume.id)

        try {
            const result = await withTimeout(getVolumeSpace(volume.path), volumeSpaceTimeoutMs, null)
            if (!result) {
                handleRetryFailure(volume.id)
                return
            }
            const { data: space, timedOut } = result
            if (!timedOut && space) {
                spaceTimedOutSet.delete(volume.id)
                spaceRetryingSet.delete(volume.id)
                spaceAutoRetryingSet.delete(volume.id)
                volumeSpaceMap.set(volume.id, space)
            } else {
                handleRetryFailure(volume.id)
            }
        } catch {
            handleRetryFailure(volume.id)
        }
    }

    function scheduleAutoRetry(volume: VolumeInfo) {
        const timer = setTimeout(() => {
            // Only auto-retry if still timed out and not already retrying
            if (spaceTimedOutSet.has(volume.id) && !spaceRetryingSet.has(volume.id)) {
                void doRetryVolumeSpace(volume, true)
            }
        }, autoRetryDelayMs)
        autoRetryTimers.push(timer)
    }

    async function fetchVolumeSpaces(vols: VolumeInfo[]): Promise<void> {
        const physicalVolumes = vols.filter(
            (v) => v.category === 'main_volume' || v.category === 'attached_volume' || v.category === 'mobile_device',
        )
        await Promise.all(
            physicalVolumes
                .filter((v) => !volumeSpaceMap.has(v.id))
                .map(async (v) => {
                    const result = await withTimeout(getVolumeSpace(v.path), volumeSpaceTimeoutMs, null)
                    if (!result) {
                        // Frontend timeout (withTimeout returned fallback null)
                        spaceTimedOutSet.add(v.id)
                        scheduleAutoRetry(v)
                        return
                    }
                    const { data: space, timedOut } = result
                    if (timedOut || !space) {
                        spaceTimedOutSet.add(v.id)
                        scheduleAutoRetry(v)
                    } else {
                        spaceTimedOutSet.delete(v.id)
                        volumeSpaceMap.set(v.id, space)
                    }
                }),
        )
    }

    function retryVolumeSpace(volume: VolumeInfo) {
        // Debounce: ignore clicks while a retry is in flight
        if (spaceRetryingSet.has(volume.id)) return
        void doRetryVolumeSpace(volume, false)
    }

    function clearAll() {
        volumeSpaceMap.clear()
        spaceTimedOutSet.clear()
        spaceRetryingSet.clear()
        spaceRetryFailedSet.clear()
        spaceRetryAttemptedSet.clear()
        spaceAutoRetryingSet.clear()
    }

    function destroy() {
        for (const timer of autoRetryTimers) clearTimeout(timer)
    }

    return {
        volumeSpaceMap,
        spaceTimedOutSet,
        spaceRetryingSet,
        spaceRetryFailedSet,
        spaceRetryAttemptedSet,
        spaceAutoRetryingSet,
        fetchVolumeSpaces,
        retryVolumeSpace,
        clearAll,
        destroy,
    }
}
