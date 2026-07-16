<script lang="ts">
    // One "Image indexing" block in the multi-drive indexing tooltip: a volume's
    // image-enrichment progress. A sibling row kind to `IndexingDriveRow`,
    // rendered by `IndexingStatusIndicator` alongside the drive rows.
    //
    // Like `IndexingDriveRow`, this WRAPPER owns the stateful glue the presentation
    // doesn't: this volume's ETA/rate sliding window (over `done`) and a 1 Hz tick, so
    // the rate and ETA advance even when progress events are throttled. Each instance
    // keeps its own window, so two surfaces rendering the same volume can't collide.
    import {
        formatEta,
        computeElapsedEta,
        computeWindowEta,
        computeWindowRate,
        blendEtas,
        pruneSnapshots,
        type EtaSnapshot,
    } from './eta'
    import type { VolumeEnrichActivity } from './media-enrich-state.svelte'
    import ProgressBar from '$lib/ui/ProgressBar.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'

    interface Props {
        activity: VolumeEnrichActivity
        driveName: string
        /** Show the drive-name heading (on for the corner indicator). */
        showHeading: boolean
    }
    const { activity, driveName, showHeading }: Props = $props()

    const done = $derived(activity.done)
    const total = $derived(activity.total)
    const paused = $derived(activity.paused)

    const imagesFraction = $derived(total > 0 ? Math.min(1, done / total) : null)
    const bytesFraction = $derived(activity.bytesTotal > 0 ? Math.min(1, activity.bytesDone / activity.bytesTotal) : null)

    // A 1 Hz clock, ticking only while actively enriching (a paused row is static).
    let now = $state(Date.now())
    $effect(() => {
        if (paused !== null) return
        now = Date.now()
        const id = setInterval(() => {
            now = Date.now()
        }, 1000)
        return () => {
            clearInterval(id)
        }
    })

    // Sliding window over processed images, for the live rate + ETA.
    const windowDurationMs = 5000
    let snapshots = $state<EtaSnapshot[]>([])
    let lastSnapshotDone = -1
    $effect(() => {
        if (paused !== null) {
            snapshots = []
            lastSnapshotDone = -1
            return
        }
        if (done !== lastSnapshotDone) {
            snapshots.push({ timestamp: Date.now(), eventsProcessed: done })
            lastSnapshotDone = done
            snapshots = pruneSnapshots(snapshots, windowDurationMs)
        }
    })

    // Images per minute from the windowed rate (per second × 60), once the window fills.
    const imagesPerMin = $derived.by(() => {
        const rate = computeWindowRate(snapshots)
        return rate != null ? Math.max(1, Math.round(rate * 60)) : null
    })

    const eta = $derived.by(() => {
        if (paused !== null || imagesFraction == null || total <= 0 || done <= 0) return null
        const remaining = total - done
        if (remaining <= 0) return tString('indexing.eta.almostDone')
        const elapsedSec = activity.startedAt > 0 ? (now - activity.startedAt) / 1000 : 0
        const blended = blendEtas(computeElapsedEta(elapsedSec, done, remaining), computeWindowEta(snapshots, remaining))
        return blended != null ? formatEta(blended) : null
    })

    // The status line under the heading: the paused reason, else "N of M images".
    const statusLine = $derived.by(() => {
        if (paused === 'waitingForIdle') return tString('indexing.enrich.pausedIdle')
        if (paused === 'disconnected') return tString('indexing.enrich.pausedDisconnected')
        return tString('indexing.enrich.progress', {
            done,
            doneText: formatNumber(done),
            total,
            totalText: formatNumber(total),
        })
    })

    // The rate · ETA detail, joined only from the parts that are known.
    const rateEtaLine = $derived.by(() => {
        if (paused !== null) return null
        const rate = imagesPerMin != null ? tString('indexing.enrich.rate', { rateText: formatNumber(imagesPerMin) }) : null
        if (rate && eta) return tString('indexing.enrich.rateEta', { rate, eta })
        return rate ?? eta
    })

    const imagesBarLabel = $derived(tString('indexing.enrich.imagesBarLabel'))
    const bytesBarLabel = $derived(tString('indexing.enrich.bytesBarLabel'))
</script>

<div class="enrich-row">
    {#if showHeading}
        <span class="enrich-heading">{tString('indexing.drive.heading', { name: driveName })}</span>
    {/if}
    <span class="enrich-label">{tString('indexing.enrich.label')}</span>
    <span class="enrich-status">{statusLine}</span>
    {#if paused === null}
        {#if imagesFraction != null}
            <ProgressBar value={imagesFraction} size="sm" ariaLabel={imagesBarLabel} />
        {/if}
        {#if bytesFraction != null}
            <ProgressBar value={bytesFraction} size="sm" ariaLabel={bytesBarLabel} />
        {/if}
        {#if rateEtaLine}
            <span class="enrich-detail">{rateEtaLine}</span>
        {/if}
    {/if}
</div>

<style>
    .enrich-row {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    /* Reads as a real title above the status line, matching IndexingDriveRow's heading. */
    .enrich-heading {
        font-weight: 600;
        color: var(--color-text-primary);
    }

    /* The row's own name ("Image indexing"), full strength so it reads as the active
       work, like a drive row's active step label. */
    .enrich-label {
        color: var(--color-text-primary);
    }

    .enrich-status {
        color: var(--color-text-tertiary);
    }

    .enrich-detail {
        color: var(--color-text-tertiary);
        font-variant-numeric: tabular-nums;
    }
</style>
