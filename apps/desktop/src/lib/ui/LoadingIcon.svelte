<script lang="ts">
    interface Props {
        openingFolder?: boolean
        loadedCount?: number
        finalizingCount?: number
        showCancelHint?: boolean
    }

    const { openingFolder = false, loadedCount, finalizingCount, showCancelHint = false }: Props = $props()

    function formatNumber(n: number): string {
        return n.toLocaleString()
    }
</script>

<div class="loading-container">
    <div class="spinner spinner-lg"></div>
    {#if finalizingCount !== undefined}
        <div class="loading-text">All {formatNumber(finalizingCount)} files loaded, just a moment now.</div>
    {:else if loadedCount !== undefined}
        <div class="loading-text">Loaded {formatNumber(loadedCount)} files...</div>
    {:else if openingFolder}
        <div class="loading-text">Opening folder...</div>
    {:else}
        <div class="loading-text">Loading...</div>
    {/if}
    {#if showCancelHint}
        <div class="cancel-hint">Press ESC to cancel and go back</div>
    {/if}
</div>

<style>
    .loading-container {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-lg);
        width: 100%;
        height: 100%;
        animation: fadeIn 400ms ease-in;
    }

    @keyframes fadeIn {
        0%,
        50% {
            opacity: 0;
        }
        100% {
            opacity: 1;
        }
    }

    .loading-text {
        color: var(--color-text-secondary);
        font-size: var(--font-size-md);
    }

    .cancel-hint {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }
</style>
