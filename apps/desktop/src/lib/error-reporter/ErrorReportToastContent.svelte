<script lang="ts">
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import { getLastSentReportId } from './error-report-toast-state.svelte'

    const toastId = 'error-report-sent'
    let copied = $state(false)

    async function handleCopy() {
        await navigator.clipboard.writeText(getLastSentReportId())
        copied = true
        setTimeout(() => (copied = false), 2000)
    }

    function handleDismiss() {
        dismissToast(toastId)
    }
</script>

<div class="content">
    <span class="message">
        Error report sent. Your reference ID is
        <span class="id-badge">{getLastSentReportId()}</span>
    </span>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={handleDismiss}>Dismiss</Button>
        <Button size="mini" variant="primary" onclick={() => void handleCopy()}>
            {copied ? 'Copied' : 'Copy ID'}
        </Button>
    </div>
</div>

<style>
    .content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
    }

    .message {
        color: var(--color-text-primary);
        line-height: 1.4;
    }

    .id-badge {
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        background: var(--color-bg-tertiary);
        padding: 0 var(--spacing-xs);
        border-radius: var(--radius-sm);
        white-space: nowrap;
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }
</style>
