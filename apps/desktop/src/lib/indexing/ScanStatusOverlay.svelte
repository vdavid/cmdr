<script lang="ts">
    import { isScanning, getEntriesScanned, getDirsFound } from './index-state.svelte'
    import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'

    const scanning = $derived(isScanning())
    const entriesScanned = $derived(getEntriesScanned())
    const dirsFound = $derived(getDirsFound())

    const progressLabel = $derived(
        entriesScanned > 0
            ? `Scanning... ${formatNumber(entriesScanned)} entries, ${formatNumber(dirsFound)} dirs`
            : 'Scanning...',
    )
</script>

{#if scanning}
    <div class="scan-overlay" role="status" aria-label="Scanning drive">
        <div class="scan-spinner"></div>
        <span class="scan-label">{progressLabel}</span>
    </div>
{/if}

<style>
    .scan-overlay {
        position: absolute;
        top: var(--spacing-sm);
        right: var(--spacing-sm);
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xxs) var(--spacing-sm);
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border);
        border-radius: 4px;
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        pointer-events: none;
        opacity: 0.85;
        z-index: 10;
    }

    .scan-spinner {
        width: 12px;
        height: 12px;
        border: 2px solid var(--color-border);
        border-top-color: var(--color-accent);
        border-radius: 50%;
        animation: scan-spin 0.8s linear infinite;
    }

    .scan-label {
        white-space: nowrap;
    }

    @keyframes scan-spin {
        to {
            transform: rotate(360deg);
        }
    }
</style>
