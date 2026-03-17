<script lang="ts">
    import {
        isReplaying,
        getReplayEventsProcessed,
        getReplayEstimatedTotal,
        getReplayStartedAt,
        isScanning,
        isAggregating,
    } from './index-state.svelte'
    import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
    import ProgressOverlay from '$lib/ui/ProgressOverlay.svelte'

    const replaying = $derived(isReplaying())
    const eventsProcessed = $derived(getReplayEventsProcessed())
    const estimatedTotal = $derived(getReplayEstimatedTotal())
    const startedAt = $derived(getReplayStartedAt())
    const scanning = $derived(isScanning())
    const aggregating = $derived(isAggregating())

    const showDelayMs = 4000

    let delayElapsed = $state(false)
    let delayTimer: ReturnType<typeof setTimeout> | undefined

    $effect(() => {
        if (replaying && startedAt > 0) {
            const remaining = showDelayMs - (Date.now() - startedAt)
            if (remaining <= 0) {
                delayElapsed = true
            } else {
                delayTimer = setTimeout(() => {
                    delayElapsed = true
                }, remaining)
            }
        } else {
            delayElapsed = false
            if (delayTimer !== undefined) {
                clearTimeout(delayTimer)
                delayTimer = undefined
            }
        }

        return () => {
            if (delayTimer !== undefined) {
                clearTimeout(delayTimer)
                delayTimer = undefined
            }
        }
    })

    const visible = $derived(replaying && !scanning && !aggregating && startedAt > 0 && delayElapsed)

    const progress = $derived(estimatedTotal > 0 ? Math.min(1, eventsProcessed / estimatedTotal) : 0)

    const detail = $derived(`${formatNumber(eventsProcessed)} events processed`)

    // Sliding window for rate estimation: snapshots of { timestamp, eventsProcessed }
    // over the last ~5 seconds, pruned on each update.
    const windowDurationMs = 5000

    let windowSnapshots = $state<{ timestamp: number; eventsProcessed: number }[]>([])
    let lastSnapshotProcessed = -1

    /** Update the sliding window when eventsProcessed changes; reset when replay stops. */
    $effect(() => {
        if (!replaying) {
            windowSnapshots = []
            lastSnapshotProcessed = -1
            return
        }
        const processed = eventsProcessed
        if (processed !== lastSnapshotProcessed) {
            const now = Date.now()
            windowSnapshots.push({ timestamp: now, eventsProcessed: processed })
            lastSnapshotProcessed = processed
            // Prune old snapshots
            const cutoff = now - windowDurationMs
            const firstValidIndex = windowSnapshots.findIndex((s) => s.timestamp >= cutoff)
            if (firstValidIndex > 0) {
                windowSnapshots = windowSnapshots.slice(firstValidIndex)
            }
        }
    })

    /** Compute sliding-window rate ETA from recent snapshots. */
    function computeWindowEta(
        snapshots: { timestamp: number; eventsProcessed: number }[],
        remaining: number,
    ): number | null {
        if (snapshots.length < 2) return null
        const oldest = snapshots[0]
        const newest = snapshots[snapshots.length - 1]
        const windowElapsed = (newest.timestamp - oldest.timestamp) / 1000
        if (windowElapsed <= 0) return null
        const windowRate = (newest.eventsProcessed - oldest.eventsProcessed) / windowElapsed
        return windowRate > 0 ? remaining / windowRate : null
    }

    /** Blend two ETA estimates 50-50, falling back to whichever is available. */
    function blendEtas(a: number | null, b: number | null): number | null {
        if (a != null && b != null) return (a + b) / 2
        return a ?? b
    }

    /** Format an ETA in seconds to a human-readable string. */
    function formatEta(seconds: number): string {
        if (seconds < 2) return 'Almost done'
        if (seconds < 60) return `${String(Math.round(seconds))}s left`
        return `${String(Math.round(seconds / 60))}m left`
    }

    const eta = $derived.by(() => {
        if (!replaying || eventsProcessed === 0 || estimatedTotal === 0 || startedAt === 0) return null

        const remaining = estimatedTotal - eventsProcessed
        if (remaining <= 0) return 'Almost done'

        const elapsedSec = (Date.now() - startedAt) / 1000
        const totalBasedEta = elapsedSec > 0 ? elapsedSec * (remaining / eventsProcessed) : null
        const windowBasedEta = computeWindowEta(windowSnapshots, remaining)
        const blended = blendEtas(totalBasedEta, windowBasedEta)

        return blended != null ? formatEta(blended) : null
    })
</script>

<ProgressOverlay {visible} label="Updating index..." {detail} {progress} {eta} />
