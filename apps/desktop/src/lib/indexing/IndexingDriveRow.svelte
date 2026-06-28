<script lang="ts">
    // One block in the multi-drive indexing tooltip: a single volume's live
    // status (scan / replay), plus this volume's aggregation phase when it's
    // aggregating (each volume's aggregation is its own; see index-state.svelte.ts).
    //
    // Each row owns its own sliding-window ETA snapshots, so several drives
    // indexing at once each get an independent rate estimate.
    import {
        formatEta,
        computeElapsedEta,
        computeWindowEta,
        blendEtas,
        pruneSnapshots,
        computeScanProgress,
        type EtaSnapshot,
    } from './eta'
    import { formatElapsedClock } from './elapsed'
    import type { VolumeIndexActivity, AggregationActivity } from './index-state.svelte'
    import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
    import ProgressBar from '$lib/ui/ProgressBar.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import type { MessageKey } from '$lib/intl/keys.gen'

    interface Props {
        activity: VolumeIndexActivity
        driveName: string
        /** Show the drive-name heading (only when more than one drive is active). */
        showHeading: boolean
        /** This volume's aggregation progress, folded into the row when present.
         *  `undefined` when this drive isn't aggregating. */
        aggregation: AggregationActivity | undefined
    }

    const { activity, driveName, showHeading, aggregation }: Props = $props()

    const aggregating = $derived(aggregation != null)
    const aggPhase = $derived(aggregation?.phase ?? '')
    const aggCurrent = $derived(aggregation?.current ?? 0)
    const aggTotal = $derived(aggregation?.total ?? 0)
    const aggStartedAt = $derived(aggregation?.startedAt ?? 0)

    const scanning = $derived(activity.phase === 'scanning')
    const replaying = $derived(activity.phase === 'replaying')

    // Shared sliding-window span (~5s) for both the scan and replay rate estimates.
    const windowDurationMs = 5000

    // ── Scan ──────────────────────────────────────────────────────────
    const entriesScanned = $derived(activity.entriesScanned)
    const dirsFound = $derived(activity.dirsFound)
    const bytesScanned = $derived(activity.bytesScanned)
    const scanStartedAt = $derived(activity.scanStartedAt)
    const priorTotalEntries = $derived(activity.priorTotalEntries)
    const priorScanDurationMs = $derived(activity.priorScanDurationMs)
    const volumeUsedBytes = $derived(activity.volumeUsedBytes)

    // The live entry/dir tally on its own detail line under the label (always
    // shown once the first progress event lands — the honest primary signal),
    // empty before then so the row falls back to the bare label, never "0
    // entries, 0 dirs".
    const scanCounters = $derived(
        entriesScanned > 0
            ? tString('indexing.scan.counters', {
                  entriesText: formatNumber(entriesScanned),
                  dirsText: formatNumber(dirsFound),
              })
            : '',
    )

    const scanProgressInfo = $derived(
        computeScanProgress(entriesScanned, bytesScanned, priorTotalEntries, volumeUsedBytes),
    )
    const scanProgress = $derived(scanProgressInfo?.fraction ?? null)
    const scanRough = $derived(scanProgressInfo?.rough ?? false)

    const scanLabel = $derived(tString(scanRough ? 'indexing.scan.labelFirst' : 'indexing.scan.label'))

    // A 1 Hz clock that ticks ONLY while this drive is scanning, so the rough
    // first-scan's elapsed time advances live even when progress events stall
    // (the count itself comes from ~500 ms backend events). `Date.now()` in a
    // `$derived` isn't reactive, so without this tick the clock would freeze on
    // a stalled scan. Idle/replaying rows run no timer.
    let now = $state(Date.now())
    $effect(() => {
        if (!scanning) return
        now = Date.now()
        const id = setInterval(() => {
            now = Date.now()
        }, 1000)
        return () => {
            clearInterval(id)
        }
    })

    // The rough first scan has no trustworthy percent (the byte-ratio sits near
    // 0 on a big volume), so it shows count + elapsed instead of a bar — honest
    // liveness without a misleading number. `null` under a second so the clock
    // never flashes "0:00".
    const scanElapsed = $derived(scanStartedAt > 0 ? formatElapsedClock(now - scanStartedAt) : null)

    // The scan detail line: the counters, plus the elapsed clock for the rough
    // first-scan tier (a calibrated rescan shows its percent + ETA on the bar
    // instead). Empty counters (pre-first-event) → no detail line at all.
    const scanDetail = $derived.by(() => {
        if (scanCounters === '') return null
        if (scanRough && scanElapsed != null) {
            return tString('indexing.scan.countersElapsed', { counters: scanCounters, elapsed: scanElapsed })
        }
        return scanCounters
    })

    const scanUnit = $derived(scanRough ? 'bytes' : 'entries')
    const scanProcessed = $derived(scanRough ? bytesScanned : entriesScanned)
    const scanTotal = $derived(scanRough ? (volumeUsedBytes ?? 0) : (priorTotalEntries ?? 0))

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

        // Early signal (tier 1 only): before the blend has data, seed from the prior
        // scan's duration minus elapsed. ms → seconds for formatEta.
        if (!scanRough && priorScanDurationMs != null && scanStartedAt > 0) {
            const seedSeconds = (priorScanDurationMs - (Date.now() - scanStartedAt)) / 1000
            return formatEta(seedSeconds)
        }
        return null
    })

    const scanEtaDisplay = $derived(
        scanEta != null && scanRough && scanEta !== tString('indexing.eta.almostDone')
            ? tString('indexing.scan.etaRough', { eta: scanEta })
            : scanEta,
    )

    // ── Replay ────────────────────────────────────────────────────────
    const eventsProcessed = $derived(activity.replayEventsProcessed)
    const estimatedTotal = $derived(activity.replayEstimatedTotal)
    const replayStartedAt = $derived(activity.replayStartedAt)

    const replayProgress = $derived(estimatedTotal > 0 ? Math.min(1, eventsProcessed / estimatedTotal) : 0)
    const replayDetail = $derived(tString('indexing.replay.detail', { eventsText: formatNumber(eventsProcessed) }))

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

    const replayEta = $derived.by(() => {
        if (!replaying || eventsProcessed === 0 || estimatedTotal === 0 || replayStartedAt === 0) return null
        const remaining = estimatedTotal - eventsProcessed
        if (remaining <= 0) return tString('indexing.eta.almostDone')

        const elapsedSec = (Date.now() - replayStartedAt) / 1000
        const totalBasedEta = computeElapsedEta(elapsedSec, eventsProcessed, remaining)
        const windowBasedEta = computeWindowEta(windowSnapshots, remaining)
        const blended = blendEtas(totalBasedEta, windowBasedEta)
        return blended != null ? formatEta(blended) : null
    })

    // ── Aggregation (folded into this row when attributed here) ────────
    const phaseToLabelKey: Record<string, MessageKey> = {
        saving_entries: 'indexing.aggregation.savingEntries',
        loading: 'indexing.aggregation.loading',
        sorting: 'indexing.aggregation.sorting',
        computing: 'indexing.aggregation.computing',
        writing: 'indexing.aggregation.writing',
    }

    const aggLabel = $derived(tString(phaseToLabelKey[aggPhase] ?? 'indexing.aggregation.computing'))
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

    // ── Mode selection (mirrors the single-drive priority: aggregation >
    //    scan > replay) ───────────────────────────────────────────────
    type Mode = 'aggregation' | 'scan' | 'replay'
    const mode = $derived<Mode>(aggregating ? 'aggregation' : scanning ? 'scan' : 'replay')

    const label = $derived(
        mode === 'aggregation' ? aggLabel : mode === 'scan' ? scanLabel : tString('indexing.replay.label'),
    )
    const detail = $derived(mode === 'scan' ? scanDetail : mode === 'replay' ? replayDetail : null)
    // The scan bar/percent shows ONLY for the calibrated tier (`!scanRough`):
    // the rough first scan leads with count + elapsed and no fabricated percent.
    const scanBarProgress = $derived(scanRough ? null : scanProgress)
    const progress = $derived(
        mode === 'aggregation' ? aggProgress : mode === 'scan' ? scanBarProgress : replayProgress,
    )
    const eta = $derived(mode === 'aggregation' ? aggEta : mode === 'scan' ? scanEtaDisplay : replayEta)

    const percent = $derived(progress != null ? Math.min(100, Math.round(progress * 100)) : null)

    const percentDisplay = $derived(
        percent == null
            ? null
            : eta
              ? tString('indexing.progress.percentEta', { percent: String(percent), eta })
              : `${String(percent)}%`,
    )
</script>

<div class="drive-row">
    {#if showHeading}
        <span class="drive-heading">{tString('indexing.drive.heading', { name: driveName })}</span>
    {/if}
    <span>{label}</span>
    {#if detail}
        <span class="tooltip-detail">{detail}</span>
    {/if}
    {#if percent != null}
        <div class="tooltip-progress">
            <ProgressBar value={progress ?? 0} size="sm" ariaLabel={label} />
            <span class="tooltip-percent">{percentDisplay}</span>
        </div>
    {/if}
</div>

<style>
    .drive-row {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    /* The drive-name heading, always shown so each block names its drive. Reads
       as a real title above the status line: full-strength primary text, bolder
       than the secondary/tertiary status and detail lines under it. */
    .drive-heading {
        font-weight: 600;
        color: var(--color-text-primary);
    }

    /* The detail line under the label: the scan's live entry/dir counters (plus
       a "· M:SS" elapsed clock on a first scan) or replay's event count. The
       counters grow without bound, so no `white-space: nowrap`: the line wraps
       within the tooltip's own `max-width` (on `.cmdr-tooltip`) instead of
       overflowing past the right-anchored, viewport-clamped box and clipping
       off the window edge. */
    .tooltip-detail {
        color: var(--color-text-tertiary);
    }

    .tooltip-progress {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    /* Holds the combined "95%, roughly 8s left" line. `tabular-nums` keeps the
       leading percent from reflowing as it ticks. */
    .tooltip-percent {
        font-variant-numeric: tabular-nums;
    }
</style>
