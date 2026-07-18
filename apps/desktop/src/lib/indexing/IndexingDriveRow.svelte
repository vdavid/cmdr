<script lang="ts">
    // One block in the multi-drive indexing tooltip: a single volume's heading +
    // shared status body (the label / counters / detail / bar+percent+ETA). This
    // is the WRAPPER around the presentational `IndexingStatusBody`: it owns the
    // stateful glue the body deliberately doesn't — this volume's ETA sliding
    // windows (scan + replay) and a 1 Hz tick. Each wrapper instance keeps its
    // own window, so two surfaces rendering the same volume (corner + that
    // volume's open badge tooltip) never collide on window state.
    //
    // Reused by both surfaces: the top-right indicator renders one per active
    // drive (with the heading), and the breadcrumb badge renders one for its
    // single volume (heading off).
    import {
        formatEta,
        computeElapsedEta,
        computeWindowEta,
        blendEtas,
        pruneSnapshots,
        computeScanProgress,
        type EtaSnapshot,
    } from './eta'
    import {
        getVolumePhase,
        getVolumeScanKind,
        type VolumeIndexActivity,
        type AggregationActivity,
    } from './index-state.svelte'
    import { isNetworkIndexRun } from './index-run-kind'
    import IndexingStatusBody from './IndexingStatusBody.svelte'
    import { getVolumes } from '$lib/stores/volume-store.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        activity: VolumeIndexActivity
        driveName: string
        /** Show the drive-name heading. On for the corner indicator (names which
         *  drive is indexing); off for the breadcrumb badge (the drive is already
         *  named in the breadcrumb). */
        showHeading: boolean
        /** This volume's aggregation progress, folded into the body when present.
         *  `undefined` when this drive isn't aggregating. */
        aggregation: AggregationActivity | undefined
    }

    const { activity, driveName, showHeading, aggregation }: Props = $props()

    const aggregating = $derived(aggregation != null)
    const scanning = $derived(activity.phase === 'scanning')
    const replaying = $derived(activity.phase === 'replaying')

    // Shared sliding-window span (~5s) for both the scan and replay rate estimates.
    const windowDurationMs = 5000

    // ── Scan inputs (drive the scan ETA window) ───────────────────────
    const entriesScanned = $derived(activity.entriesScanned)
    const bytesScanned = $derived(activity.bytesScanned)
    const scanStartedAt = $derived(activity.scanStartedAt)
    const priorTotalEntries = $derived(activity.priorTotalEntries)
    const priorScanDurationMs = $derived(activity.priorScanDurationMs)
    const volumeUsedBytes = $derived(activity.volumeUsedBytes)

    const scanProgressInfo = $derived(
        computeScanProgress(entriesScanned, bytesScanned, priorTotalEntries, volumeUsedBytes),
    )
    const scanProgress = $derived(scanProgressInfo?.fraction ?? null)
    const scanRough = $derived(scanProgressInfo?.rough ?? false)

    const scanUnit = $derived(scanRough ? 'bytes' : 'entries')
    const scanProcessed = $derived(scanRough ? bytesScanned : entriesScanned)
    const scanTotal = $derived(scanRough ? (volumeUsedBytes ?? 0) : (priorTotalEntries ?? 0))

    // A 1 Hz clock that ticks ONLY while this drive is actively scanning or
    // aggregating, so the rough first-scan's elapsed time and the aggregation ETA
    // advance live even when progress events stall. `Date.now()` in a `$derived`
    // isn't reactive, so without this tick the clock would freeze on a stall.
    // Idle/replaying rows run no timer.
    let now = $state(Date.now())
    $effect(() => {
        if (!scanning && !aggregating) return
        now = Date.now()
        const id = setInterval(() => {
            now = Date.now()
        }, 1000)
        return () => {
            clearInterval(id)
        }
    })

    // ── Scan ETA sliding window ───────────────────────────────────────
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

    // ── Replay ETA sliding window ─────────────────────────────────────
    const eventsProcessed = $derived(activity.replayEventsProcessed)
    const estimatedTotal = $derived(activity.replayEstimatedTotal)
    const replayStartedAt = $derived(activity.replayStartedAt)

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

    // The windowed ETA for the body: scan or replay (aggregation computes its own
    // window-free ETA inside the body from `now`).
    const windowedEta = $derived(aggregating ? null : scanning ? scanEtaDisplay : replaying ? replayEta : null)

    // This volume's top-level pipeline phase (the checklist's authoritative driver
    // for the catch-up step) and whether it's a network drive (which skips the
    // Save and Catch-up steps). Network-ness is keyed on the volume's `category`
    // from the store, so a non-root LOCAL drive (a USB stick / SD card) gets the
    // local checklist, not the network one. Read here in the stateful wrapper; the
    // body stays presentational, taking both as props.
    const phase = $derived(getVolumePhase(activity.volumeId))
    const isNetwork = $derived(isNetworkIndexRun(activity.volumeId, getVolumes()))
    // First index build vs full rescan, for the run-kind header. Read here in the
    // stateful wrapper (like `phase`); the body stays presentational.
    const scanKind = $derived(getVolumeScanKind(activity.volumeId))
</script>

<div class="drive-row">
    {#if showHeading}
        <span class="drive-heading">{tString('indexing.drive.heading', { name: driveName })}</span>
    {/if}
    <IndexingStatusBody {activity} {aggregation} {now} {windowedEta} {phase} {isNetwork} {scanKind} />
</div>

<style>
    .drive-row {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    /* The drive-name heading, shown so each block names its drive. Reads as a
       real title above the status line: full-strength primary text, bolder than
       the secondary/tertiary status and detail lines under it. */
    .drive-heading {
        font-weight: 600;
        color: var(--color-text-primary);
    }
</style>
