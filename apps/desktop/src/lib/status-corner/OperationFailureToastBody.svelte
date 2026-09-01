<script lang="ts">
    /**
     * The shared body of the two failure notices (`OperationFailedToastContent`,
     * `OperationFailuresToastContent`): a red-glyph title, whatever the notice adds under
     * it, and the one action, "Show in operation queue". That action also dismisses the
     * toast: the queue window then owns this conversation in full, and leaving the toast up
     * behind it would be two voices saying the same thing.
     */
    import type { Snippet } from 'svelte'
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import { dismissToast } from '$lib/ui/toast'
    import { tString } from '$lib/intl/messages.svelte'
    import { openQueueWindow } from '$lib/file-operations/queue/queue-window'

    interface Props {
        /** Dedup id of this toast; lets the notice get out of its own way. */
        toastId: string
        title: string
        /** What the notice adds under the title (one failure's reason); the summary adds nothing. */
        children?: Snippet
    }

    const { toastId, title, children }: Props = $props()

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
    {@render children?.()}
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
