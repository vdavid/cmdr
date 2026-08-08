<script lang="ts">
    /**
     * What a burst of failures collapses into: one notice with the count.
     *
     * It reads the count off the main window's store rather than taking it as a
     * prop, for one mechanical reason: the toast store's dedup path replaces a
     * toast's content and level but NOT its props, so a prop-carried count
     * would freeze at whatever the fourth failure saw. Reading live also keeps
     * it honest while the user clears rows in the queue window.
     */
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import { dismissToast } from '$lib/ui/toast'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { openQueueWindow } from '$lib/file-operations/queue/queue-window'
    import { getMainWindowOperationRows } from '$lib/file-operations/queue/main-window-operations.svelte'

    interface Props {
        /** Dedup id of this toast; lets the component get out of its own way. */
        toastId: string
    }

    const { toastId }: Props = $props()

    const count = $derived(getMainWindowOperationRows().filter((row) => row.snapshot.status === 'failed').length)
    const title = $derived(tString('queue.failureToast.summary', { count, countText: formatInteger(count) }))

    function showInQueue(): void {
        void openQueueWindow()
        dismissToast(toastId)
    }
</script>

<div class="toast-body">
    <span class="title">
        <span class="glyph" aria-hidden="true"><Icon name="triangle-alert" size={15} /></span>
        {title}
    </span>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={showInQueue}>{tString('queue.failureToast.action')}</Button>
    </div>
</div>

<style>
    .toast-body {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
    }

    .title {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        color: var(--color-text-primary);
        font-weight: 600;
    }

    .glyph {
        display: inline-flex;
        flex-shrink: 0;
        /* Error red, matching the toast's own stripe: this notice names the
           failure and shows its reason, so it carries the full severity. */
        color: var(--color-error);
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        margin-top: var(--spacing-xs);
    }
</style>
