<script lang="ts">
    // The collapsed one-line summary for a SECONDARY drive in the corner indicator
    // when several drives index at once: the primary (first) drive expands to its
    // full `IndexingDriveRow` checklist, every other drive collapses to this line
    // so four drives don't stack into a wall of checklists. The breadcrumb badge
    // always shows its own volume's full checklist, never this.
    //
    // One line: drive name + the current step + a compact metric (a percent where
    // there's a trustworthy denominator, else the honest running count).
    import {
        getVolumePhase,
        type VolumeIndexActivity,
        type AggregationActivity,
    } from './index-state.svelte'
    import { isNetworkIndexRun } from './index-run-kind'
    import {
        deriveSteps,
        activeStep,
        stepKindToLabelKey,
        type IndexRunKind,
        type AggregationSubPhase,
    } from './indexing-steps'
    import { computeScanProgress } from './eta'
    import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
    import { getVolumes } from '$lib/stores/volume-store.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        activity: VolumeIndexActivity
        aggregation: AggregationActivity | undefined
        driveName: string
    }

    const { activity, aggregation, driveName }: Props = $props()

    const phase = $derived(getVolumePhase(activity.volumeId))
    const isNetwork = $derived(isNetworkIndexRun(activity.volumeId, getVolumes()))
    const runKind = $derived<IndexRunKind>(
        activity.phase === 'replaying' ? 'replay' : isNetwork ? 'network' : 'local',
    )
    const aggSubPhase = $derived(aggregation?.phase as AggregationSubPhase | undefined)
    const steps = $derived(deriveSteps({ runKind, phase, aggregationSubPhase: aggSubPhase }))
    const active = $derived(activeStep(steps))
    const activeLabel = $derived(active ? tString(stepKindToLabelKey[active.kind]) : '')

    const aggFraction = $derived(
        aggregation && aggregation.total > 0 ? Math.min(1, aggregation.current / aggregation.total) : null,
    )
    const aggDeterminate = $derived(
        aggSubPhase === 'saving_entries' || aggSubPhase === 'computing' || aggSubPhase === 'writing',
    )
    const scanInfo = $derived(
        computeScanProgress(
            activity.entriesScanned,
            activity.bytesScanned,
            activity.priorTotalEntries,
            activity.volumeUsedBytes,
        ),
    )

    function pct(fraction: number): string {
        return `${String(Math.min(100, Math.round(fraction * 100)))}%`
    }

    // A percent where the denominator is trustworthy, else the running count
    // (first scan), else nothing (indeterminate catch-up / no signal yet).
    const metric = $derived.by<string | null>(() => {
        switch (active?.kind) {
            case 'findFiles':
                if (scanInfo && !scanInfo.rough) return pct(scanInfo.fraction)
                if (activity.entriesScanned > 0)
                    return tString('indexing.summary.found', { countText: formatNumber(activity.entriesScanned) })
                return null
            case 'saveFileList':
            case 'computeFolderSizes':
                return aggDeterminate && aggFraction != null ? pct(aggFraction) : null
            case 'updateIndex':
                return activity.replayEstimatedTotal > 0
                    ? pct(activity.replayEventsProcessed / activity.replayEstimatedTotal)
                    : null
            default:
                return null
        }
    })
</script>

<div class="drive-summary">
    <span class="drive-heading">{tString('indexing.drive.heading', { name: driveName })}</span>
    <span class="summary-line">
        <span class="summary-step">{activeLabel}</span>
        {#if metric}
            <span class="summary-metric">{metric}</span>
        {/if}
    </span>
</div>

<style>
    .drive-summary {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    /* Matches the expanded row's heading so primary and secondary drives read as
       one family (the corner indicator scopes `.drive-heading`'s weight/color). */
    .drive-heading {
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .summary-line {
        display: flex;
        align-items: baseline;
        gap: var(--spacing-xs);
    }

    .summary-step {
        color: var(--color-text-secondary);
    }

    .summary-metric {
        color: var(--color-text-tertiary);
        font-variant-numeric: tabular-nums;
    }
</style>
