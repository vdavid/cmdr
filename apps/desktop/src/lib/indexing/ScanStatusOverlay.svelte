<script lang="ts">
    import {
        isScanning,
        getEntriesScanned,
        getDirsFound,
        isAggregating,
        getAggregationPhase,
        getAggregationCurrent,
        getAggregationTotal,
        getAggregationStartedAt,
    } from './index-state.svelte'
    import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'

    const scanning = $derived(isScanning())
    const entriesScanned = $derived(getEntriesScanned())
    const dirsFound = $derived(getDirsFound())
    const aggregating = $derived(isAggregating())
    const aggPhase = $derived(getAggregationPhase())
    const aggCurrent = $derived(getAggregationCurrent())
    const aggTotal = $derived(getAggregationTotal())
    const aggStartedAt = $derived(getAggregationStartedAt())

    const visible = $derived(scanning || aggregating)

    const scanLabel = $derived(
        entriesScanned > 0
            ? `Scanning... ${formatNumber(entriesScanned)} entries, ${formatNumber(dirsFound)} dirs`
            : 'Scanning...',
    )

    const phaseToLabel: Record<string, string> = {
        saving_entries: 'Saving entries...',
        loading: 'Loading directories...',
        sorting: 'Sorting directories...',
        computing: 'Computing directory sizes...',
        writing: 'Saving directory sizes...',
    }

    const aggLabel = $derived(phaseToLabel[aggPhase] ?? 'Computing directory sizes...')

    const aggPercent = $derived(aggTotal > 0 ? Math.min(100, Math.round((aggCurrent / aggTotal) * 100)) : 0)

    const aggEta = $derived.by(() => {
        if (aggTotal === 0 || aggCurrent === 0 || aggStartedAt === 0) return ''
        const elapsed = (Date.now() - aggStartedAt) / 1000
        const rate = aggCurrent / elapsed
        if (rate <= 0) return ''
        const remaining = (aggTotal - aggCurrent) / rate
        if (remaining < 2) return 'Almost done'
        if (remaining < 60) return `${String(Math.round(remaining))}s left`
        return `${String(Math.round(remaining / 60))}m left`
    })

    const showProgressBar = $derived(
        aggPhase === 'saving_entries' || aggPhase === 'computing' || aggPhase === 'writing',
    )
</script>

{#if visible}
    <div class="scan-overlay" role="status" aria-label={aggregating ? 'Computing directory sizes' : 'Scanning drive'}>
        <span class="spinner spinner-sm"></span>
        {#if aggregating}
            <div class="agg-content">
                <span class="scan-label">{aggLabel}</span>
                {#if showProgressBar}
                    <div class="progress-row">
                        <div
                            class="progress-bar"
                            role="progressbar"
                            aria-valuenow={aggPercent}
                            aria-valuemin={0}
                            aria-valuemax={100}
                        >
                            <div class="progress-fill" style="width: {aggPercent}%"></div>
                        </div>
                        <span class="progress-text">{aggPercent}%</span>
                        {#if aggEta}
                            <span class="progress-eta">{aggEta}</span>
                        {/if}
                    </div>
                {/if}
            </div>
        {:else}
            <span class="scan-label">{scanLabel}</span>
        {/if}
    </div>
{/if}

<style>
    .scan-overlay {
        position: absolute;
        top: var(--spacing-sm);
        right: var(--spacing-sm);
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-xs);
        padding: var(--spacing-xxs) var(--spacing-sm);
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        pointer-events: none;
        opacity: 0.85;
        z-index: var(--z-sticky);
    }

    .scan-label {
        white-space: nowrap;
    }

    .agg-content {
        display: flex;
        flex-direction: column;
        gap: 3px;
        min-width: 160px;
    }

    .progress-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .progress-bar {
        flex: 1;
        height: 4px;
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-xs);
        overflow: hidden;
    }

    .progress-fill {
        height: 100%;
        background: var(--color-accent);
        border-radius: var(--radius-xs);
        transition: width 0.3s ease-out;
    }

    .progress-text {
        font-variant-numeric: tabular-nums;
        min-width: 28px;
        text-align: right;
    }

    .progress-eta {
        color: var(--color-text-tertiary);
    }
</style>
