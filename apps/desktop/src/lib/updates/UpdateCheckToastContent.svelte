<script lang="ts">
    import { updateState } from './update-state.svelte'
    import { formatUpdateStatus } from './update-status-text'
    import { openErrorReportDialog } from '$lib/error-reporter/error-report-flow.svelte'

    const statusText = $derived(formatUpdateStatus(updateState))

    function handleSendErrorReport() {
        openErrorReportDialog(`Update check failed: ${updateState.error ?? ''}`)
    }
</script>

<div class="content">
    {#if updateState.error !== null}
        <span class="message">Error: {updateState.error}</span>
        <button class="link-button" onclick={handleSendErrorReport}>Send error report</button>
    {:else}
        <span class="message">{statusText}</span>
    {/if}
</div>

<style>
    .content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .message {
        line-height: 1.4;
    }

    .link-button {
        background: none;
        border: none;
        padding: 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        cursor: default;
        text-align: left;
    }

    .link-button:hover {
        color: var(--color-text-secondary);
    }
</style>
