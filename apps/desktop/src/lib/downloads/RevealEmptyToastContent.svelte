<script lang="ts">
    /**
     * INFO toast shown when ⌘J / palette / MCP reveal can't find any
     * eligible download. Pure-prop-driven: the "Go to Downloads" action and
     * the toast id arrive as props, captured at toast-creation time and
     * never re-read. Same shape as `DownloadToastContent`.
     */
    import { dismissToast } from '$lib/ui/toast'

    interface Props {
        /** Dedup id of this toast; lets the component self-dismiss on action. */
        toastId: string
        /**
         * Closure over the focused-pane + Downloads dir captured at toast-add
         * time. A remap of the focused pane after the toast appears does NOT
         * change the destination — the user sees the same target they would
         * have seen when they pressed ⌘J.
         */
        onGoToDownloads: () => void
    }

    const { toastId, onGoToDownloads }: Props = $props()

    function handleGoToDownloads() {
        onGoToDownloads()
        dismissToast(toastId)
    }

    function handleDismiss() {
        dismissToast(toastId)
    }
</script>

<div class="content">
    <span class="message">Your Downloads folder is empty. Go there anyway?</span>
    <div class="actions">
        <button class="link-button" onclick={handleGoToDownloads}>Go to Downloads</button>
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
    }

    .link-button:hover {
        color: var(--color-text-secondary);
    }
</style>
