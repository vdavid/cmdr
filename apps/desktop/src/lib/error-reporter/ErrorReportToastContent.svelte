<script module lang="ts">
    /**
     * Module-level slot for the most recently sent report ID. The dialog calls
     * `setLastSentReportId(id)` right before `addToast(ErrorReportToastContent, ...)`
     * so the toast can render the ID without prop bridging (the toast system mounts
     * components with no props).
     */
    let lastSentReportId = $state('')

    export function setLastSentReportId(id: string): void {
        lastSentReportId = id
    }

    export function getLastSentReportId(): string {
        return lastSentReportId
    }
</script>

<script lang="ts">
    import { dismissToast } from '$lib/ui/toast'

    const toastId = 'error-report-sent'
    let copied = $state(false)

    async function handleCopy() {
        await navigator.clipboard.writeText(lastSentReportId)
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
        <span class="id-badge">{lastSentReportId}</span>
    </span>
    <div class="actions">
        <button class="link-button" onclick={() => void handleCopy()}>
            {copied ? 'Copied' : 'Copy ID'}
        </button>
        <button class="link-button" onclick={handleDismiss}>Dismiss</button>
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
        gap: var(--spacing-md);
    }

    .link-button {
        background: none;
        border: none;
        padding: 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        cursor: default;
    }

    .link-button:hover {
        color: var(--color-text-secondary);
    }
</style>
