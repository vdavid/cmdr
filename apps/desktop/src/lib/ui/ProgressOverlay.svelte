<script lang="ts">
    import ProgressBar from './ProgressBar.svelte'

    const {
        visible,
        label,
        detail,
        progress,
        eta,
    }: {
        visible: boolean
        label: string
        detail?: string
        progress?: number | null
        eta?: string | null
    } = $props()

    const percent = $derived(progress != null ? Math.min(100, Math.round(progress * 100)) : null)
    const hasExtra = $derived(detail != null || progress !== undefined)
</script>

{#if visible}
    <div class="progress-overlay" role="status" aria-label={label}>
        <span class="spinner spinner-sm"></span>
        {#if hasExtra}
            <div class="content">
                <span class="label">{label}</span>
                {#if detail}
                    <span class="detail">{detail}</span>
                {/if}
                {#if percent != null}
                    <div class="progress-row">
                        <ProgressBar value={progress ?? 0} size="sm" />
                        <span class="progress-text">{percent}%</span>
                        {#if eta}
                            <span class="progress-eta">{eta}</span>
                        {/if}
                    </div>
                {/if}
            </div>
        {:else}
            <span class="label">{label}</span>
        {/if}
    </div>
{/if}

<style>
    .progress-overlay {
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
        opacity: 0.8;
        z-index: var(--z-sticky);
    }

    .label {
        white-space: nowrap;
    }

    .content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        min-width: 160px;
    }

    .detail {
        white-space: nowrap;
    }

    .progress-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
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
