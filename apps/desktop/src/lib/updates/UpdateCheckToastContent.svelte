<script lang="ts">
    import { updateState } from './update-state.svelte'
    import { formatUpdateStatus } from './update-status-text'
    import Button from '$lib/ui/Button.svelte'
    import { openErrorReportDialog } from '$lib/error-reporter/error-report-flow.svelte'
    import { t, tString } from '$lib/intl/messages.svelte'

    const statusText = $derived(formatUpdateStatus(updateState))

    function handleSendErrorReport() {
        openErrorReportDialog(`Update check failed: ${updateState.error ?? ''}`)
    }
</script>

<div class="content">
    {#if updateState.error !== null}
        <span class="message">{t('updates.checkToast.errorPrefix', { message: updateState.error })}</span>
        <div class="actions">
            <Button size="mini" variant="secondary" onclick={handleSendErrorReport}
                >{tString('updates.checkToast.sendErrorReport')}</Button
            >
        </div>
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

    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }
</style>
