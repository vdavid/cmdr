/**
 * Reactive state for drive index scanning status.
 * Tracks whether a scan is running and provides progress info.
 */

import { listen, type UnlistenFn } from '$lib/tauri-commands'

// Scan state
let scanning = $state(false)
let entriesScanned = $state(0)
let dirsFound = $state(0)

// Reactive getters
export function isScanning(): boolean {
    return scanning
}

export function getEntriesScanned(): number {
    return entriesScanned
}

export function getDirsFound(): number {
    return dirsFound
}

/** Reset scan counters (called on new scan start). */
function resetCounters() {
    entriesScanned = 0
    dirsFound = 0
}

// Event listener cleanup handles
const unlistenHandles: UnlistenFn[] = []

/** Set up listeners for index scan events. Call once during app init. */
export async function initIndexState(): Promise<void> {
    const unlistenStarted = await listen<{ volumeId: string }>('index-scan-started', () => {
        scanning = true
        resetCounters()
    })
    unlistenHandles.push(unlistenStarted)

    const unlistenProgress = await listen<{
        volumeId: string
        entriesScanned: number
        dirsFound: number
    }>('index-scan-progress', (event) => {
        entriesScanned = event.payload.entriesScanned
        dirsFound = event.payload.dirsFound
    })
    unlistenHandles.push(unlistenProgress)

    const unlistenComplete = await listen<{
        volumeId: string
        totalEntries: number
        totalDirs: number
        durationMs: number
    }>('index-scan-complete', (event) => {
        scanning = false
        entriesScanned = event.payload.totalEntries
        dirsFound = event.payload.totalDirs
    })
    unlistenHandles.push(unlistenComplete)
}

/** Clean up all listeners. Call during app teardown. */
export function destroyIndexState(): void {
    for (const unlisten of unlistenHandles) {
        unlisten()
    }
    unlistenHandles.length = 0
}
