<script lang="ts">
    import type { Snippet } from 'svelte'
    import { isModified, resetSetting, type SettingId } from '$lib/settings'

    interface Props {
        id: SettingId
        label: string
        description: string
        disabled?: boolean
        disabledReason?: string
        requiresRestart?: boolean
        children: Snippet
    }

    const {
        id,
        label,
        description,
        disabled = false,
        disabledReason,
        requiresRestart = false,
        children,
    }: Props = $props()

    const modified = $derived(isModified(id))

    function handleReset() {
        resetSetting(id)
    }
</script>

<div class="setting-row" class:disabled>
    <div class="setting-header">
        <div class="setting-label-wrapper">
            {#if modified}
                <span class="modified-indicator" title="Modified from default">●</span>
            {/if}
            <label class="setting-label" for={id}>{label}</label>
            {#if disabled && disabledReason}
                <span class="disabled-badge">{disabledReason}</span>
            {/if}
            {#if requiresRestart}
                <span class="restart-badge">Restart required</span>
            {/if}
        </div>
        <div class="setting-control">
            {@render children()}
        </div>
    </div>
    <p class="setting-description">{description}</p>
    {#if modified}
        <button class="reset-link" onclick={handleReset}> Reset to default </button>
    {/if}
</div>

<style>
    .setting-row {
        padding: var(--spacing-sm) 0;
        border-bottom: 1px solid var(--color-border-secondary);
    }

    .setting-row:last-child {
        border-bottom: none;
    }

    .setting-row.disabled {
        opacity: 0.6;
    }

    .setting-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-md);
    }

    .setting-label-wrapper {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .modified-indicator {
        color: var(--color-accent);
        font-size: 10px;
    }

    .setting-label {
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .disabled-badge,
    .restart-badge {
        font-size: 10px;
        padding: 2px 6px;
        border-radius: 3px;
        font-weight: 500;
    }

    .disabled-badge {
        background: var(--color-bg-tertiary);
        color: var(--color-text-muted);
    }

    .restart-badge {
        background: var(--color-warning);
        color: white;
    }

    .setting-control {
        flex-shrink: 0;
    }

    .setting-description {
        margin: var(--spacing-xs) 0 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-xs);
        line-height: 1.4;
    }

    .reset-link {
        margin-top: var(--spacing-xs);
        padding: 0;
        background: none;
        border: none;
        color: var(--color-accent);
        font-size: var(--font-size-xs);
        cursor: pointer;
        text-decoration: underline;
    }

    .reset-link:hover {
        color: var(--color-accent-hover);
    }
</style>
