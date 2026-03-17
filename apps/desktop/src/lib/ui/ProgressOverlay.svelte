<script lang="ts">
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
                        <div
                            class="progress-bar"
                            role="progressbar"
                            aria-valuenow={percent}
                            aria-valuemin={0}
                            aria-valuemax={100}
                        >
                            <div class="progress-fill" style="width: {percent}%"></div>
                        </div>
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
