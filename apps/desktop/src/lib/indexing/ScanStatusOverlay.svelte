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
    import ProgressOverlay from '$lib/ui/ProgressOverlay.svelte'

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

    const showProgressBar = $derived(
        aggPhase === 'saving_entries' || aggPhase === 'computing' || aggPhase === 'writing',
    )

    const aggProgress = $derived.by(() => {
        if (!showProgressBar) return null
        return aggTotal > 0 ? Math.min(1, aggCurrent / aggTotal) : 0
    })

    const aggEta = $derived.by(() => {
        if (aggTotal === 0 || aggCurrent === 0 || aggStartedAt === 0) return null
        const elapsed = (Date.now() - aggStartedAt) / 1000
        const rate = aggCurrent / elapsed
        if (rate <= 0) return null
        const remaining = (aggTotal - aggCurrent) / rate
        if (remaining < 2) return 'Almost done'
        if (remaining < 60) return `${String(Math.round(remaining))}s left`
        return `${String(Math.round(remaining / 60))}m left`
    })

    const label = $derived(aggregating ? aggLabel : scanLabel)
    const progress = $derived(aggregating ? aggProgress : undefined)
    const eta = $derived(aggregating ? aggEta : undefined)
</script>

<ProgressOverlay {visible} {label} {progress} {eta} />
