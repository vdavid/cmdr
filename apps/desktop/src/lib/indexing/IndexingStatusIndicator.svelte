<script lang="ts">
    import IconHourglass from '~icons/lucide/hourglass'
    import {
        isScanning,
        getEntriesScanned,
        getDirsFound,
        getBytesScanned,
        getScanStartedAt,
        getPriorTotalEntries,
        getPriorScanDurationMs,
        getVolumeUsedBytes,
        isAggregating,
        getAggregationPhase,
        getAggregationCurrent,
        getAggregationTotal,
        getAggregationStartedAt,
        isReplaying,
        getReplayEventsProcessed,
        getReplayEstimatedTotal,
        getReplayStartedAt,
    } from './index-state.svelte'
    import {
        formatEta,
        computeElapsedEta,
        computeWindowEta,
        blendEtas,
        pruneSnapshots,
        computeScanProgress,
        type EtaSnapshot,
    } from './eta'
    import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
    import { tooltip } from '$lib/tooltip/tooltip'
    import ProgressBar from '$lib/ui/ProgressBar.svelte'

    const scanning = $derived(isScanning())
    const entriesScanned = $derived(getEntriesScanned())
    const dirsFound = $derived(getDirsFound())
    const bytesScanned = $derived(getBytesScanned())
    const scanStartedAt = $derived(getScanStartedAt())
    const priorTotalEntries = $derived(getPriorTotalEntries())
    const priorScanDurationMs = $derived(getPriorScanDurationMs())
    const volumeUsedBytes = $derived(getVolumeUsedBytes())
    const aggregating = $derived(isAggregating())
    const aggPhase = $derived(getAggregationPhase())
    const aggCurrent = $derived(getAggregationCurrent())
    const aggTotal = $derived(getAggregationTotal())
    const aggStartedAt = $derived(getAggregationStartedAt())
    const replaying = $derived(isReplaying())
    const eventsProcessed = $derived(getReplayEventsProcessed())
    const estimatedTotal = $derived(getReplayEstimatedTotal())
    const replayStartedAt = $derived(getReplayStartedAt())

    // The icon shows for any index activity. Scan/aggregation own the message when they're
    // running; replay fills the corner only when nothing more specific is happening, so one
    // indicator carries all three states without two components fighting for the corner.
    const visible = $derived(scanning || aggregating || replaying)

    const scanCounters = $derived(
        entriesScanned > 0
            ? ` ${formatNumber(entriesScanned)} entries, ${formatNumber(dirsFound)} dirs`
            : '',
    )

    // Two-tier scan progress: calibrated (entries vs the prior scan's total) after the first
    // scan, rough (bytes vs the volume's used bytes) on the first scan. Null → counter-only.
    const scanProgressInfo = $derived(
        computeScanProgress(entriesScanned, bytesScanned, priorTotalEntries, volumeUsedBytes),
    )
    const scanProgress = $derived(scanProgressInfo?.fraction ?? null)
    const scanRough = $derived(scanProgressInfo?.rough ?? false)

    // The scan label names the tier so the rough first-scan reads as approximate. The counters
    // ride along on both. "..." matches the indicator's three-ASCII-dots convention.
    const scanLabel = $derived(
        (scanRough ? 'Scanning your drive (first scan)...' : 'Scanning your drive...') + scanCounters,
    )

    // The ETA unit must match the progress tier: entries for tier 1, bytes for tier 2. The
    // window samples the same counter the tier divides by, so the rate and the remaining work
    // never mix units.
    const scanUnit = $derived(scanRough ? 'bytes' : 'entries')
    const scanProcessed = $derived(scanRough ? bytesScanned : entriesScanned)
    const scanTotal = $derived(scanRough ? (volumeUsedBytes ?? 0) : (priorTotalEntries ?? 0))

    // Shared sliding-window span (~5s) for both the scan and replay rate estimates. Early
    // total-rate extrapolation alone is wildly wrong, so the window-rate blend smooths it.
    const windowDurationMs = 5000

    // Sliding window over the scan's progress, same machinery the replay branch uses. Keyed on
    // the active unit so a tier flip (it won't mid-scan, but be safe) resets cleanly.
    let scanWindowSnapshots = $state<EtaSnapshot[]>([])
    let lastScanSnapshotProcessed = -1
    let lastScanUnit = ''

    $effect(() => {
        if (!scanning || scanProgress == null) {
            scanWindowSnapshots = []
            lastScanSnapshotProcessed = -1
            lastScanUnit = scanUnit
            return
        }
        if (scanUnit !== lastScanUnit) {
            scanWindowSnapshots = []
            lastScanSnapshotProcessed = -1
            lastScanUnit = scanUnit
        }
        const processed = scanProcessed
        if (processed !== lastScanSnapshotProcessed) {
            scanWindowSnapshots.push({ timestamp: Date.now(), eventsProcessed: processed })
            lastScanSnapshotProcessed = processed
            scanWindowSnapshots = pruneSnapshots(scanWindowSnapshots, windowDurationMs)
        }
    })

    const scanEta = $derived.by(() => {
        if (!scanning || scanProgress == null || scanTotal <= 0 || scanProcessed <= 0) return null
        const remaining = scanTotal - scanProcessed

        const elapsedSec = scanStartedAt > 0 ? (Date.now() - scanStartedAt) / 1000 : 0
        const elapsedBasedEta = computeElapsedEta(elapsedSec, scanProcessed, remaining)
        const windowBasedEta = computeWindowEta(scanWindowSnapshots, remaining)
        const blended = blendEtas(elapsedBasedEta, windowBasedEta)
        if (blended != null) return formatEta(blended)

        // Early signal (tier 1 only): before the blend has data, seed from the prior scan's
        // duration minus elapsed. ms → seconds for formatEta, which floors negatives to
        // "Almost done" (a scan outrunning the prior duration degrades honestly).
        if (!scanRough && priorScanDurationMs != null && scanStartedAt > 0) {
            const seedSeconds = (priorScanDurationMs - (Date.now() - scanStartedAt)) / 1000
            return formatEta(seedSeconds)
        }
        return null
    })

    const phaseToLabel: Record<string, string> = {
        saving_entries: 'Saving entries...',
        loading: 'Loading directories...',
        sorting: 'Sorting directories...',
        computing: 'Computing directory sizes...',
        writing: 'Saving directory sizes...',
    }

    const aggLabel = $derived(phaseToLabel[aggPhase] ?? 'Computing directory sizes...')

    const aggHasProgress = $derived(
        aggPhase === 'saving_entries' || aggPhase === 'computing' || aggPhase === 'writing',
    )

    const aggProgress = $derived(aggHasProgress && aggTotal > 0 ? Math.min(1, aggCurrent / aggTotal) : null)

    const aggEta = $derived.by(() => {
        if (aggTotal === 0 || aggCurrent === 0 || aggStartedAt === 0) return null
        const elapsed = (Date.now() - aggStartedAt) / 1000
        const remaining = computeElapsedEta(elapsed, aggCurrent, aggTotal - aggCurrent)
        return remaining != null ? formatEta(remaining) : null
    })

    // Sliding window of replay-progress snapshots over the last ~5 seconds, fed through the
    // pure window-rate helper. The final ETA blends the window rate with the total-rate
    // extrapolation 50-50 (see `windowDurationMs` near the scan window above).
    let windowSnapshots = $state<EtaSnapshot[]>([])
    let lastSnapshotProcessed = -1

    $effect(() => {
        if (!replaying) {
            windowSnapshots = []
            lastSnapshotProcessed = -1
            return
        }
        const processed = eventsProcessed
        if (processed !== lastSnapshotProcessed) {
            windowSnapshots.push({ timestamp: Date.now(), eventsProcessed: processed })
            lastSnapshotProcessed = processed
            windowSnapshots = pruneSnapshots(windowSnapshots, windowDurationMs)
        }
    })

    const replayProgress = $derived(estimatedTotal > 0 ? Math.min(1, eventsProcessed / estimatedTotal) : 0)
    const replayDetail = $derived(`${formatNumber(eventsProcessed)} events processed`)

    const replayEta = $derived.by(() => {
        if (!replaying || eventsProcessed === 0 || estimatedTotal === 0 || replayStartedAt === 0) return null
        const remaining = estimatedTotal - eventsProcessed
        if (remaining <= 0) return 'Almost done'

        const elapsedSec = (Date.now() - replayStartedAt) / 1000
        const totalBasedEta = computeElapsedEta(elapsedSec, eventsProcessed, remaining)
        const windowBasedEta = computeWindowEta(windowSnapshots, remaining)
        const blended = blendEtas(totalBasedEta, windowBasedEta)
        return blended != null ? formatEta(blended) : null
    })

    // The mode the tooltip content reflects. Scan/aggregation win over replay.
    type Mode = 'scan' | 'aggregation' | 'replay'
    const mode = $derived<Mode>(aggregating ? 'aggregation' : scanning ? 'scan' : 'replay')

    // Tier 2 wraps its ETA in "roughly" since the bytes-vs-used-bytes denominator is approximate.
    // "Almost done" is already a terminal phrase, so it stays unprefixed (not "roughly Almost done").
    const scanEtaDisplay = $derived(
        scanEta != null && scanRough && scanEta !== 'Almost done' ? `roughly ${scanEta}` : scanEta,
    )

    const label = $derived(mode === 'aggregation' ? aggLabel : mode === 'scan' ? scanLabel : 'Updating index...')
    const detail = $derived(mode === 'replay' ? replayDetail : null)
    const progress = $derived(mode === 'aggregation' ? aggProgress : mode === 'scan' ? scanProgress : replayProgress)
    const eta = $derived(mode === 'aggregation' ? aggEta : mode === 'scan' ? scanEtaDisplay : replayEta)

    const percent = $derived(progress != null ? Math.min(100, Math.round(progress * 100)) : null)

    // The tooltip action adopts `tooltipContent` (not the hidden wrapper) so it renders visibly
    // inside the tooltip: an adopted element keeps its own `hidden` attribute, so a hidden host
    // passed as `contentEl` would render an empty tooltip.
    let tooltipContent = $state<HTMLDivElement>()
</script>

{#if visible}
    <span
        class="indexing-status"
        tabindex="0"
        role="img"
        aria-label="Drive indexing status"
        use:tooltip={{ contentEl: tooltipContent }}
    >
        <IconHourglass width="14" height="14" />
    </span>

    <div hidden>
        <div bind:this={tooltipContent} class="tooltip-content">
            <span class="tooltip-label">{label}</span>
            {#if detail}
                <span class="tooltip-detail">{detail}</span>
            {/if}
            {#if percent != null}
                <div class="tooltip-progress">
                    <ProgressBar value={progress ?? 0} size="sm" ariaLabel={label} />
                    <span class="tooltip-percent">{percent}%</span>
                    {#if eta}
                        <span class="tooltip-eta">{eta}</span>
                    {/if}
                </div>
            {/if}
        </div>
    </div>
{/if}

<style>
    .indexing-status {
        position: absolute;
        top: var(--spacing-sm);
        right: var(--spacing-sm);
        display: inline-flex;
        align-items: center;
        justify-content: center;
        color: var(--color-text-tertiary);
        z-index: var(--z-sticky);
    }

    .indexing-status:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
        border-radius: var(--radius-xs);
    }

    /* A gentle opacity pulse signals the app is doing something, without drawing the eye. */
    @media (prefers-reduced-motion: no-preference) {
        .indexing-status {
            animation: indexing-pulse 2s ease-in-out infinite;
        }
    }

    @keyframes indexing-pulse {
        0%,
        100% {
            opacity: 0.5;
        }
        50% {
            opacity: 1;
        }
    }

    .tooltip-content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        /* Stable width so the tooltip doesn't jitter as the counters tick (the tooltip
           action measures once on show and can't see later content growth). */
        min-width: 200px;
    }

    .tooltip-label {
        white-space: nowrap;
    }

    .tooltip-detail {
        white-space: nowrap;
        color: var(--color-text-tertiary);
    }

    .tooltip-progress {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .tooltip-percent {
        font-variant-numeric: tabular-nums;
        min-width: 28px;
        text-align: right;
    }

    .tooltip-eta {
        color: var(--color-text-tertiary);
    }
</style>
