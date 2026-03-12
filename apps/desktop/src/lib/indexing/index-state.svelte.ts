/**
 * Reactive state for drive index scanning status.
 * Tracks whether a scan is running and provides progress info.
 */

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast'

// Scan state
let scanning = $state(false)
let entriesScanned = $state(0)
let dirsFound = $state(0)

// Aggregation state
let aggregating = $state(false)
let aggregationPhase = $state('')
let aggregationCurrent = $state(0)
let aggregationTotal = $state(0)
let aggregationStartedAt = $state(0)

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

export function isAggregating(): boolean {
    return aggregating
}

export function getAggregationPhase(): string {
    return aggregationPhase
}

export function getAggregationCurrent(): number {
    return aggregationCurrent
}

export function getAggregationTotal(): number {
    return aggregationTotal
}

export function getAggregationStartedAt(): number {
    return aggregationStartedAt
}

/** Reset scan counters (called on new scan start). */
function resetCounters() {
    entriesScanned = 0
    dirsFound = 0
}

function resetAggregation() {
    aggregating = false
    aggregationPhase = ''
    aggregationCurrent = 0
    aggregationTotal = 0
    aggregationStartedAt = 0
}

const rescanReasonToMessage: Record<string, string> = {
    stale_index:
        "Your drive index is outdated — it looks like the app hasn't run for a while. Running a fresh scan to catch up.",
    journal_gap: "The system's file change log doesn't go back far enough. Running a fresh scan to rebuild the index.",
    replay_overflow:
        'A lot of file changes happened since last run. Running a fresh scan instead of replaying them one by one.',
    too_many_subdir_rescans:
        'Many directories changed significantly since last run. Running a fresh scan to get everything up to date.',
    watcher_start_failed: "Couldn't start the file change watcher. Running a fresh scan to get the index up to date.",
    reconciler_buffer_overflow:
        'Heavy filesystem activity overwhelmed the event buffer. Running a fresh scan to stay accurate.',
    incomplete_previous_scan:
        "The previous scan didn't finish (the app may have been closed). Restarting the scan from scratch.",
    watcher_channel_overflow:
        'A burst of filesystem activity overflowed the watcher channel. Running a fresh scan to stay accurate.',
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
        resetAggregation()
    })
    unlistenHandles.push(unlistenComplete)

    const unlistenAggregation = await listen<{
        phase: string
        current: number
        total: number
    }>('index-aggregation-progress', (event) => {
        const { phase, current, total } = event.payload
        if (!aggregating) {
            aggregating = true
            aggregationStartedAt = Date.now()
        }
        aggregationPhase = phase
        aggregationCurrent = current
        aggregationTotal = total
    })
    unlistenHandles.push(unlistenAggregation)

    const unlistenAggComplete = await listen<null>('index-aggregation-complete', () => {
        resetAggregation()
    })
    unlistenHandles.push(unlistenAggComplete)

    const unlistenRescan = await listen<{
        volumeId: string
        reason: string
        details: string
    }>('index-rescan-notification', (event) => {
        const message =
            rescanReasonToMessage[event.payload.reason] ?? 'Running a fresh drive scan to keep the index accurate.'
        addToast(message, { level: 'info', timeoutMs: 8000, id: 'index-rescan' })
    })
    unlistenHandles.push(unlistenRescan)

    // Query current status to catch scans already in progress before the frontend loaded.
    // The scan starts in Tauri's setup() hook, so the 'index-scan-started' event may fire
    // before the frontend's event listeners are registered.
    try {
        const status = await invoke<{
            initialized: boolean
            scanning: boolean
            entriesScanned: number
            dirsFound: number
            indexStatus: unknown
            dbFileSize: number | null
        }>('get_index_status')
        if (status.scanning) {
            scanning = true
            entriesScanned = status.entriesScanned
            dirsFound = status.dirsFound
        }
    } catch {
        // Indexing not initialized or unavailable — no-op
    }
}

/** Clean up all listeners. Call during app teardown. */
export function destroyIndexState(): void {
    for (const unlisten of unlistenHandles) {
        unlisten()
    }
    unlistenHandles.length = 0
}
