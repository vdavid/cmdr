<script lang="ts">
    // The top-right hourglass: shows whenever ANY drive is indexing (scan,
    // replay, or aggregation), local or SMB or MTP. Its tooltip lists every
    // active drive as a live block (name + status + progress where meaningful);
    // each block is an IndexingDriveRow that owns its own ETA window.
    import Icon from '$lib/ui/Icon.svelte'
    import {
        getActiveIndexVolumes,
        isAnyVolumeIndexing,
        getVolumeAggregation,
        getAggregatingVolumeIds,
        getActivePhaseVolumeIds,
        placeholderActivity,
        type VolumeIndexActivity,
        type AggregationActivity,
    } from './index-state.svelte'
    import { getEnrichingVolumes, isAnyVolumeEnriching, type VolumeEnrichActivity } from './media-enrich-state.svelte'
    import IndexingDriveRow from './IndexingDriveRow.svelte'
    import IndexingDriveSummary from './IndexingDriveSummary.svelte'
    import IndexingEnrichRow from './IndexingEnrichRow.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { tString } from '$lib/intl/messages.svelte'
    import { getVolumes } from '$lib/stores/volume-store.svelte'

    // The hourglass shows whenever a drive is indexing OR any volume is actively
    // enriching images (the plan-M5 second publisher ORs into the gate). A paused-only
    // enrichment doesn't light it on its own — see `isAnyVolumeEnriching`.
    const visible = $derived(isAnyVolumeIndexing() || isAnyVolumeEnriching())
    const enrichVolumes = $derived<VolumeEnrichActivity[]>(getEnrichingVolumes())
    const activeVolumes = $derived(getActiveIndexVolumes())
    const aggregatingVolumeIds = $derived(getAggregatingVolumeIds())
    const phaseVolumeIds = $derived(getActivePhaseVolumeIds())
    const volumes = $derived(getVolumes())

    // Resolve a volume id to a human display name from the shared volume store.
    // Falls back to the id itself if the volume isn't in the list (e.g. a drive
    // that vanished mid-scan, or before the store hydrated) — honest over blank.
    function driveName(volumeId: string): string {
        return volumes.find((v) => v.id === volumeId)?.name ?? volumeId
    }

    // The rows to render, in this order: every actively scanning/replaying volume
    // (its aggregation folded in), then any volume aggregating with no live scan/
    // replay entry (its scan already finished), then any volume mid-pipeline in a
    // phase with no live entry at all (the reconcile step). Each volume's
    // aggregation is its own, keyed by volumeId, so two drives stay separate.
    interface DriveRow {
        activity: VolumeIndexActivity
        aggregation: AggregationActivity | undefined
    }

    const rows = $derived.by<DriveRow[]>(() => {
        const result: DriveRow[] = activeVolumes.map((activity) => ({
            activity,
            aggregation: getVolumeAggregation(activity.volumeId),
        }))
        const seen = result.map((r) => r.activity.volumeId)
        // Aggregating with no live scan/replay row: a synthetic aggregation-only row.
        for (const volumeId of aggregatingVolumeIds) {
            if (!seen.includes(volumeId)) {
                result.push({ activity: placeholderActivity(volumeId), aggregation: getVolumeAggregation(volumeId) })
                seen.push(volumeId)
            }
        }
        // Mid-pipeline with no live scan/aggregation entry (reconcile): a phase-only
        // row, so the checklist's catch-up step stays visible.
        for (const volumeId of phaseVolumeIds) {
            if (!seen.includes(volumeId)) {
                result.push({ activity: placeholderActivity(volumeId), aggregation: undefined })
                seen.push(volumeId)
            }
        }
        return result
    })

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
            {#each rows as row, i (row.activity.volumeId)}
                {#if i === 0}
                    <!-- The primary (first) drive expands to its full checklist. -->
                    <IndexingDriveRow
                        activity={row.activity}
                        driveName={driveName(row.activity.volumeId)}
                        showHeading={true}
                        aggregation={row.aggregation}
                    />
                {:else}
                    <!-- Secondary drives collapse to one line, so N drives don't
                         stack into N checklists. -->
                    <IndexingDriveSummary
                        activity={row.activity}
                        aggregation={row.aggregation}
                        driveName={driveName(row.activity.volumeId)}
                    />
                {/if}
            {/each}
            <!-- Image indexing joins as a sibling row kind (plan M5): one block per
                 actively-enriching or paused volume, below the drive rows. -->
            {#each enrichVolumes as enrich (enrich.volumeId)}
                <IndexingEnrichRow activity={enrich} driveName={driveName(enrich.volumeId)} showHeading={true} />
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
