<script lang="ts">
    /**
     * The main window's notice that one backgrounded operation stopped before it
     * was done.
     *
     * It never auto-dismisses (see `operation-failure-watch.svelte.ts`): a copy
     * that died while the user was at lunch has to still be here when they get
     * back. It's a PREVIEW of the failed row, not a replacement for it — the
     * queue window carries the full reason, the suggestion, and the Dismiss.
     */
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import { dismissToast } from '$lib/ui/toast'
    import { tString } from '$lib/intl/messages.svelte'
    import { openQueueWindow } from '$lib/file-operations/queue/queue-window'
    import { failureReasonFor } from '$lib/file-operations/queue/failure-reason'
    import type { OperationSnapshot } from '$lib/tauri-commands'

    interface Props {
        /** Dedup id of this toast; lets the component get out of its own way. */
        toastId: string
        /** The retained failure, snapshotted at toast time. It can't change: a
         *  failure is settled, and only a dismissal ends it. */
        snapshot: OperationSnapshot
    }

    const { toastId, snapshot }: Props = $props()

    const title = $derived(tString('queue.failureToast.title', { type: snapshot.operationType }))
    /** `reason.message` is markup from the error pipeline (escaped names and
     *  paths, size tiers), the same value the error dialog renders. */
    const reason = $derived(failureReasonFor(snapshot))

    function showInQueue(): void {
        void openQueueWindow()
        // The queue window now owns this conversation, in full. Leaving the
        // toast up behind it would be two voices saying the same thing.
        dismissToast(toastId)
    }
</script>

<div class="toast-body">
    <span class="title">
        <span class="glyph" aria-hidden="true"><Icon name="triangle-alert" size={15} /></span>
        {title}
    </span>
    {#if reason}
        <!-- eslint-disable-next-line svelte/no-at-html-tags -- markup from the typed error via `failureReasonFor`: escaped names/paths plus size tiers, no user input. Same boundary as `FallbackErrorContent`. -->
        <p class="reason">{@html reason.message}</p>
    {/if}
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
        color: var(--color-warning);
    }

    /*
      The queue row prints the reason in full; a 360px toast can't, and the
      pipeline's prose was written for a dialog. Three lines covers every stock
      message, so in practice nothing is cut — the clamp is here for the
      variants that interpolate a path or a device name, which have no length
      limit at all. The rest of the reason (and the suggestion, which the toast
      leaves out) is one button-press away.
    */
    .reason {
        margin: 0;
        color: var(--color-text-secondary);
        overflow-wrap: anywhere;
        display: -webkit-box;
        -webkit-line-clamp: 3;
        line-clamp: 3;
        -webkit-box-orient: vertical;
        overflow: hidden;
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        margin-top: var(--spacing-xs);
    }
</style>
