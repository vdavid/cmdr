<script lang="ts">
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { getLastSentReportId, getLastSentReportKind } from './error-report-toast-state.svelte'

    const toastId = 'error-report-sent'
    let copied = $state(false)

    // One toast, two outcomes: a report that shipped, and a note that joined the report
    // Flow B had already sent. Only the lead sentence differs.
    const messageKey = $derived(
        getLastSentReportKind() === 'amended'
            ? ('errorReporter.amendedToast.message' as const)
            : ('errorReporter.sentToast.message' as const),
    )

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
        {tString(messageKey)}
        <span class="id-badge">{getLastSentReportId()}</span>
    </span>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={handleDismiss}
            >{tString('errorReporter.sentToast.dismiss')}</Button
        >
        <Button size="mini" variant="primary" onclick={() => void handleCopy()}>
            {copied ? tString('errorReporter.sentToast.copied') : tString('errorReporter.sentToast.copyId')}
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
