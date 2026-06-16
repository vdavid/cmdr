<script lang="ts">
    /**
     * INFO toast shown when ⌘J / palette / MCP go-to-latest can't find any
     * eligible download. Pure-prop-driven: the "Go to Downloads" action and
     * the toast id arrive as props, captured at toast-creation time and
     * never re-read. Same shape as `DownloadToastContent`.
     */
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import { tString } from '$lib/intl/messages.svelte'

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
    <span class="message">{tString('downloads.empty.message')}</span>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={handleDismiss}>{tString('downloads.empty.dismiss')}</Button>
        <Button size="mini" variant="primary" onclick={handleGoToDownloads}
            >{tString('downloads.empty.goToDownloads')}</Button
        >
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
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }
</style>
