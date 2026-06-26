<script lang="ts">
    // The top-right hourglass: shows whenever ANY drive is indexing (scan,
    // replay, or aggregation), local or SMB or MTP. Its tooltip lists every
    // active drive as a live block (name + status + progress where meaningful);
    // each block is an IndexingDriveRow that owns its own ETA window.
    import Icon from '$lib/ui/Icon.svelte'
    import {
        getActiveIndexVolumes,
        isAnyVolumeIndexing,
        isAggregating,
        getAggregationPhase,
        getAggregationCurrent,
        getAggregationTotal,
        getAggregationStartedAt,
        getAggregatingVolumeId,
        type VolumeIndexActivity,
    } from './index-state.svelte'
    import IndexingDriveRow from './IndexingDriveRow.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { tString } from '$lib/intl/messages.svelte'
    import { getVolumes } from '$lib/stores/volume-store.svelte'

    const visible = $derived(isAnyVolumeIndexing())
    const activeVolumes = $derived(getActiveIndexVolumes())
    const aggregating = $derived(isAggregating())
    const aggVolumeId = $derived(getAggregatingVolumeId())
    const volumes = $derived(getVolumes())

    // Resolve a volume id to a human display name from the shared volume store.
    // Falls back to the id itself if the volume isn't in the list (e.g. a drive
    // that vanished mid-scan, or before the store hydrated) — honest over blank.
    function driveName(volumeId: string): string {
        return volumes.find((v) => v.id === volumeId)?.name ?? volumeId
    }

    // The rows to render: every actively scanning/replaying volume, plus the
    // volume aggregation is attributed to if it isn't already in that set (its
    // scan completed, aggregation followed). Aggregation carries no volumeId, so
    // it's folded into exactly one row (the last scan to complete).
    interface DriveRow {
        activity: VolumeIndexActivity
        aggregating: boolean
    }

    const rows = $derived.by<DriveRow[]>(() => {
        const result: DriveRow[] = activeVolumes.map((activity) => ({
            activity,
            aggregating: aggregating && activity.volumeId === aggVolumeId,
        }))
        // Aggregation attributed to a volume with no live scan/replay row: add a
        // synthetic aggregation-only row so the phase is still visible.
        if (aggregating && !activeVolumes.some((a) => a.volumeId === aggVolumeId)) {
            result.push({ activity: aggregationOnlyActivity(aggVolumeId), aggregating: true })
        }
        return result
    })

    // A placeholder activity for an aggregation-only row (no live scan/replay).
    // The row reads only `phase`/`volumeId` from it when aggregating; the scan/
    // replay fields stay at zero and aren't shown.
    function aggregationOnlyActivity(volumeId: string): VolumeIndexActivity {
        return {
            volumeId,
            phase: 'scanning',
            entriesScanned: 0,
            dirsFound: 0,
            bytesScanned: 0,
            scanStartedAt: 0,
            priorTotalEntries: null,
            priorScanDurationMs: null,
            volumeUsedBytes: null,
            replayEventsProcessed: 0,
            replayEstimatedTotal: 0,
            replayStartedAt: 0,
        }
    }

    // Show the per-drive heading only when more than one drive is indexing, so
    // the common single-drive case stays as terse as before.
    const showHeadings = $derived(rows.length > 1)

    const aggPhase = $derived(getAggregationPhase())
    const aggCurrent = $derived(getAggregationCurrent())
    const aggTotal = $derived(getAggregationTotal())
    const aggStartedAt = $derived(getAggregationStartedAt())

    // The tooltip action adopts `tooltipContent` (not the hidden wrapper) so it
    // renders visibly inside the tooltip: an adopted element keeps its own
    // `hidden` attribute, so a hidden host passed as `contentEl` would render
    // an empty tooltip.
    let tooltipContent = $state<HTMLDivElement>()
</script>

{#if visible}
    <span
        class="indexing-status"
        tabindex="0"
        role="img"
        aria-label={tString('indexing.status.ariaLabel')}
        use:tooltip={{ contentEl: tooltipContent }}
    >
        <Icon name="hourglass" size={14} />
    </span>

    <div hidden>
        <div bind:this={tooltipContent} class="tooltip-content">
            {#each rows as row (row.activity.volumeId)}
                <IndexingDriveRow
                    activity={row.activity}
                    driveName={driveName(row.activity.volumeId)}
                    showHeading={showHeadings}
                    aggregating={row.aggregating}
                    {aggPhase}
                    {aggCurrent}
                    {aggTotal}
                    {aggStartedAt}
                />
            {/each}
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
        /* Larger gap between drive blocks than within a block (the row's own
           internal gap is `--spacing-xxs`), so multiple drives read as distinct. */
        gap: var(--spacing-sm);
        /* Stable width so the tooltip doesn't jitter as the counters tick (the
           tooltip action measures once on show and can't see later content
           growth). Rows wrap within the tooltip's own `max-width` (set on
           `.cmdr-tooltip`), so a long first line wraps onto a second line
           instead of overflowing past the right-anchored tooltip box. */
        min-width: 200px;
    }
</style>
